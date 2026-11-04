/**
 * Plugin runtime.
 *
 * Plugins are JavaScript, not native libraries: a broken plugin throws an
 * exception the host catches, instead of taking the launcher down with it. The
 * trade-off is that enabled plugin code shares the webview, which is why the
 * backend only hands over source for plugins the user turned on and ships them
 * disabled by default.
 *
 * The bridge is versioned (`PLUGIN_API`). A manifest written for another major
 * version is listed and refused, never coerced.
 */
import { api } from "./api";
import type { PluginEntry } from "./types";

export const PLUGIN_API = 1;

/** Events the host emits. Plugins may subscribe to any of them. */
export const PLUGIN_EVENTS = [
  "launcher:ready",
  "view:changed",
  "instances:changed",
  "game:started",
  "game:stopped",
  "theme:changed",
] as const;

export type PluginEvent = (typeof PLUGIN_EVENTS)[number] | (string & {});

export interface PluginPanelDefinition {
  /** Unique per plugin; re-registering the same id replaces the panel. */
  id: string;
  title: string;
  /** Build the panel inside `element`. Return a cleanup function if needed. */
  mount: (element: HTMLElement) => void | (() => void);
}

export interface PluginRuntime {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  status: "running" | "failed" | "off";
  error: string | null;
  panels: Array<{ id: string; title: string }>;
  notices: string[];
  hooks: string[];
}

interface LivePlugin {
  entry: PluginEntry;
  panels: Map<string, PluginPanelDefinition>;
  handlers: Map<string, Set<(payload: unknown) => void>>;
  notices: string[];
  failures: number;
  failed: boolean;
  error: string | null;
}

/** A plugin that throws this many times is stopped, not retried forever. */
const MAX_FAILURES = 5;
const NOTICE_LIMIT = 20;
const STORAGE_PREFIX = "spud.plugin.";

let live = new Map<string, LivePlugin>();
const listeners = new Set<() => void>();
let snapshot: PluginRuntime[] = [];

function publish() {
  snapshot = [...live.values()].map((plugin) => ({
    id: plugin.entry.id,
    name: plugin.entry.name,
    version: plugin.entry.version,
    enabled: plugin.entry.enabled,
    status: plugin.failed ? "failed" : plugin.entry.enabled ? "running" : "off",
    error: plugin.error,
    panels: [...plugin.panels.values()].map((panel) => ({ id: panel.id, title: panel.title })),
    notices: [...plugin.notices],
    hooks: plugin.entry.hooks,
  }));
  for (const listener of listeners) listener();
}

export function subscribePlugins(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function pluginSnapshot(): PluginRuntime[] {
  return snapshot;
}

/** Every registered panel, in plugin order. */
export function pluginPanels(): Array<{ pluginId: string; pluginName: string; panel: PluginPanelDefinition }> {
  const out: Array<{ pluginId: string; pluginName: string; panel: PluginPanelDefinition }> = [];
  for (const plugin of live.values()) {
    if (plugin.failed) continue;
    for (const panel of plugin.panels.values()) {
      out.push({ pluginId: plugin.entry.id, pluginName: plugin.entry.name, panel });
    }
  }
  return out;
}

function pushNotice(plugin: LivePlugin, text: string) {
  plugin.notices = [text, ...plugin.notices].slice(0, NOTICE_LIMIT);
  publish();
}

/**
 * Run plugin code with the failure budget attached.
 *
 * Every call site goes through here so one plugin's exception can never abort
 * the host loop or another plugin's handler.
 */
function guard<T>(plugin: LivePlugin, where: string, run: () => T): T | undefined {
  if (plugin.failed) return undefined;
  try {
    return run();
  } catch (cause) {
    plugin.failures += 1;
    plugin.error = `${where}: ${cause instanceof Error ? cause.message : String(cause)}`;
    if (plugin.failures >= MAX_FAILURES) {
      plugin.failed = true;
      plugin.error = `stopped after ${MAX_FAILURES} errors — last: ${plugin.error}`;
    }
    pushNotice(plugin, plugin.error);
    return undefined;
  }
}

function pluginStorage(id: string) {
  const key = (name: string) => `${STORAGE_PREFIX}${id}.${name}`;
  return {
    get<T = unknown>(name: string, fallback: T | null = null): T | null {
      try {
        const raw = localStorage.getItem(key(name));
        return raw === null ? fallback : (JSON.parse(raw) as T);
      } catch {
        return fallback;
      }
    },
    set(name: string, value: unknown) {
      try {
        localStorage.setItem(key(name), JSON.stringify(value));
      } catch {
        // A full quota is not fatal; the plugin keeps running without storage.
      }
    },
    remove(name: string) {
      try {
        localStorage.removeItem(key(name));
      } catch {
        // ignore
      }
    },
  };
}

function createBridge(plugin: LivePlugin) {
  const { entry } = plugin;

  return {
    id: entry.id,
    name: entry.name,
    version: entry.version,
    api: PLUGIN_API,

    log: (...args: unknown[]) => {
      // Prefixed so a plugin's output is attributable in the devtools console.
      console.info(`[plugin:${entry.id}]`, ...args);
    },

    ui: {
      panel(definition: PluginPanelDefinition) {
        guard(plugin, "ui.panel", () => {
          if (!definition?.id || typeof definition.mount !== "function") {
            throw new Error("ui.panel needs { id, title, mount(element) }");
          }
          if (plugin.panels.size >= 8 && !plugin.panels.has(definition.id)) {
            throw new Error("a plugin may register at most 8 panels");
          }
          plugin.panels.set(definition.id, definition);
          publish();
        });
      },
      removePanel(id: string) {
        guard(plugin, "ui.removePanel", () => {
          plugin.panels.delete(id);
          publish();
        });
      },
      notify(text: string) {
        guard(plugin, "ui.notify", () => pushNotice(plugin, String(text)));
      },
    },

    events: {
      on(name: PluginEvent, handler: (payload: unknown) => void) {
        if (typeof handler !== "function") return () => undefined;
        const set = plugin.handlers.get(name) ?? new Set();
        set.add(handler);
        plugin.handlers.set(name, set);
        return () => {
          set.delete(handler);
        };
      },
    },

    storage: pluginStorage(entry.id),
  };
}

/** Compile and start one plugin. The entry point runs once, immediately. */
function start(entry: PluginEntry) {
  const plugin: LivePlugin = {
    entry,
    panels: new Map(),
    handlers: new Map(),
    notices: [],
    failures: 0,
    failed: false,
    error: entry.error ?? null,
  };

  if (entry.error || !entry.enabled || !entry.source) {
    live.set(entry.id, plugin);
    return;
  }

  try {
    const factory = new Function("Spud", `"use strict";\n${entry.source}\n`);
    factory(createBridge(plugin));
  } catch (cause) {
    plugin.failed = true;
    plugin.error = `entry threw: ${cause instanceof Error ? cause.message : String(cause)}`;
  }
  live.set(entry.id, plugin);
}

/** Stop one plugin: drop handlers and panels, keep its storage. */
function stop(id: string) {
  const plugin = live.get(id);
  if (!plugin) return;
  plugin.handlers.clear();
  plugin.panels.clear();
}

/** Re-read the folder and reload every enabled plugin from scratch. */
export async function reloadPlugins(): Promise<PluginRuntime[]> {
  const entries = await api.plugins.list();
  for (const id of [...live.keys()]) stop(id);
  live = new Map();

  for (const entry of entries) start(entry);
  publish();
  emitPluginEvent("launcher:ready");
  return snapshot;
}

export async function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
  const entries = await api.plugins.setEnabled(id, enabled);
  stop(id);
  live.delete(id);
  const entry = entries.find((row) => row.id === id);
  if (entry) start(entry);
  publish();
  if (enabled) emitPluginEvent("launcher:ready");
}

export function emitPluginEvent(name: PluginEvent, payload?: unknown) {
  for (const plugin of live.values()) {
    if (plugin.failed) continue;
    const handlers = plugin.handlers.get(name);
    if (!handlers) continue;
    for (const handler of [...handlers]) {
      guard(plugin, `events.on(${name})`, () => handler(payload));
    }
  }
}
