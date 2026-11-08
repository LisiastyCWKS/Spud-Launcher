import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, onInstallProgress } from "./lib/api";
import { t, setLocale } from "./lib/i18n";
import { reason, toast } from "./lib/toast";
import type {
  AccountsView,
  AuthStatus,
  DownloadEvent,
  LauncherSettings,
  InstanceSummary,
} from "./lib/types";
import { useTheme } from "./hooks/useTheme";
import { useAccent } from "./hooks/useAccent";
import { useAppearance, STOCK_APPEARANCE, type AppearanceSettings } from "./hooks/useAppearance";
import { useRunning } from "./hooks/useRunning";
import { Sidebar, type View } from "./components/Sidebar";
import { InstanceGrid } from "./components/InstanceGrid";
import { InstanceDetail } from "./components/InstanceDetail";
import { CreateInstanceDialog } from "./components/CreateInstanceDialog";
import { ModrinthPanel } from "./components/ModrinthPanel";
import { CurseForgePanel } from "./components/CurseForgePanel";
import { AccountsPanel } from "./components/AccountsPanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { FriendsPanel } from "./components/FriendsPanel";
import { ImportPanel } from "./components/ImportPanel";
import { ProgressToast } from "./components/ProgressToast";
import { PluginsView } from "./components/PluginsView";
import { UpdateBanner } from "./components/UpdateBanner";
import { Toaster } from "./components/Toaster";
import { ConfirmHost } from "./components/ConfirmHost";
import { emitPluginEvent, reloadPlugins } from "./lib/plugins";
import { Icon } from "./components/Icon";
import { classNames } from "./lib/format";

/**
 * Merge the saved settings with an unsaved preview from the appearance editor.
 *
 * The preview never touches `settings`: leaving the panel without saving simply
 * drops it, and the shell snaps back to what is on disk.
 */
function appearanceOf(
  settings: LauncherSettings | null,
  preview: Partial<LauncherSettings> | null,
): AppearanceSettings {
  const merged: Partial<LauncherSettings> = { ...(settings ?? {}), ...(preview ?? {}) };
  return {
    preset: merged.theme_preset ?? STOCK_APPEARANCE.preset,
    density: merged.density ?? STOCK_APPEARANCE.density,
    corners: merged.corners ?? STOCK_APPEARANCE.corners,
    background: merged.background ?? STOCK_APPEARANCE.background,
    backgroundOpacity: merged.background_opacity ?? STOCK_APPEARANCE.backgroundOpacity,
    backgroundBlur: merged.background_blur ?? STOCK_APPEARANCE.backgroundBlur,
    sidebarWidth: merged.sidebar_width ?? STOCK_APPEARANCE.sidebarWidth,
    contentWidth: merged.content_width ?? STOCK_APPEARANCE.contentWidth,
    reduceMotion: merged.reduce_motion ?? STOCK_APPEARANCE.reduceMotion,
  };
}

export default function App() {
  const { theme, resolved, setTheme } = useTheme();
  const { accent, setAccent } = useAccent(resolved);
  const { running, refresh: refreshRunning } = useRunning();
  /** Unsaved appearance edits, kept apart from `settings` on purpose. */
  const [appearancePreview, setAppearancePreview] = useState<Partial<LauncherSettings> | null>(null);

  const [view, setView] = useState<View>("instances");
  const [instances, setInstances] = useState<InstanceSummary[]>([]);
  const [settings, setSettings] = useState<LauncherSettings | null>(null);
  const [accounts, setAccounts] = useState<AccountsView | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [discoverTarget, setDiscoverTarget] = useState<string | null>(null);
  const [discoverSource, setDiscoverSource] = useState<"modrinth" | "curseforge">("modrinth");
  const [diagnosticsTarget, setDiagnosticsTarget] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [createOpen, setCreateOpen] = useState(false);
  const [progress, setProgress] = useState<DownloadEvent | null>(null);
  const [fullscreen, setFullscreen] = useState(false);

  const appearance = useMemo(
    () => appearanceOf(settings, appearancePreview),
    [settings, appearancePreview],
  );
  useAppearance(appearance);

  /**
   * One funnel for every failure a screen can raise. Errors keep the toast up
   * longer than a confirmation and carry the raw backend message, because a
   * recipe name and a Java path are both more useful than "something failed".
   */
  const report = useCallback((cause: unknown) => {
    // `null` is how a screen clears its own error line; only real failures toast.
    if (cause === null || cause === undefined || cause === "") return;
    toast.error(t("common.error"), reason(cause));
  }, []);

  const notify = useCallback((title: string, description?: string) => {
    toast.success(title, description);
  }, []);

  // A preview belongs to the Settings view. Leaving it drops the unsaved look
  // instead of letting an experiment leak into every other screen.
  useEffect(() => {
    if (view !== "settings") setAppearancePreview(null);
  }, [view]);

  const refreshInstances = useCallback(async () => {
    try {
      const list = await api.instances.list();
      setInstances(list);
      emitPluginEvent("instances:changed", { count: list.length });
      setSelectedId((current) =>
        current && list.some((entry) => entry.id === current) ? current : null,
      );
      setDiscoverTarget((current) =>
        current && list.some((entry) => entry.id === current) ? current : null,
      );
      setDiagnosticsTarget((current) =>
        current && list.some((entry) => entry.id === current) ? current : null,
      );
    } catch (cause) {
      report(cause);
    }
  }, [report]);

  const refreshAuthStatus = useCallback(async () => {
    try {
      setAuthStatus(await api.accounts.status());
    } catch {
      setAuthStatus(null);
    }
  }, []);

  useEffect(() => {
    void refreshAuthStatus();
  }, [refreshAuthStatus]);

  // Plugins boot once, after the first paint, so a slow or broken plugin never
  // delays the launcher window showing up.
  useEffect(() => {
    void reloadPlugins().catch(report);
  }, [report]);

  useEffect(() => {
    api.window
      .state()
      .then((state) => setFullscreen(state.fullscreen))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.fullscreen = fullscreen ? "true" : "false";
  }, [fullscreen]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "F11") {
        event.preventDefault();
        void api.window
          .setFullscreen(!fullscreen)
          .then(setFullscreen)
          .catch(() => undefined);
        return;
      }
      // Escape leaves fullscreen — but a dialog owns Escape while it is open.
      if (event.key === "Escape" && fullscreen && !document.querySelector(".modal-overlay")) {
        void api.window.setFullscreen(false).then(setFullscreen).catch(() => undefined);
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen]);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<boolean>("window://fullscreen", (event) => {
      if (!disposed) setFullscreen(event.payload);
    });
    return () => {
      disposed = true;
      void unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [list, loadedSettings, accountList] = await Promise.all([
          api.instances.list(),
          api.settings.get(),
          api.accounts.list().catch(() => null),
        ]);
        if (!alive) return;
        setInstances(list);
        setSettings(loadedSettings);
        setAccounts(accountList);
        setDiscoverTarget(list.length > 0 ? list[0].id : null);
        // Apply locale from settings so translations resolve immediately.
        setLocale(loadedSettings.locale);
      } catch (cause) {
        if (alive) report(cause);
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, [report]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const attach = async (channel: "install://progress" | "mods://progress") => {
      const unlisten = await onInstallProgress((event) => {
        if (disposed) return;
        setProgress(event);
        if (event.type === "finished") {
          notify(t("toast.download_done"), t("toast.download_done_text", { n: event.files ?? 0 }));
        }
      }, channel);
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };

    void attach("install://progress");
    void attach("mods://progress");

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [notify]);

  const selected = useMemo(
    () => instances.find((entry) => entry.id === selectedId) ?? null,
    [instances, selectedId],
  );

  const activeAccount = accounts?.accounts.find((entry) => entry.active) ?? null;

  // Plugins hear about game starts and stops by watching the same map the UI
  // renders, so a crash is reported the same way a clean exit is.
  const previousRunning = useRef<string[]>([]);
  useEffect(() => {
    const current = Object.keys(running);
    const before = previousRunning.current;
    for (const id of current.filter((id) => !before.includes(id))) {
      emitPluginEvent("game:started", { instanceId: id, game: running[id] });
    }
    for (const id of before.filter((id) => !current.includes(id))) {
      emitPluginEvent("game:stopped", { instanceId: id });
    }
    previousRunning.current = current;
  }, [running]);

  useEffect(() => {
    emitPluginEvent("theme:changed", { theme, resolved });
  }, [theme, resolved]);

  useEffect(() => {
    emitPluginEvent("view:changed", { view });
  }, [view]);

  const openInstance = (id: string) => {
    setSelectedId(id);
    setView("instances");
  };

  const header = useMemo(() => {
    switch (view) {
      case "instances":
        return selected
          ? {
              title: selected.name,
              sub: `${selected.mc_version} · ${selected.ready ? t("instances.ready") : t("instances.not_installed")}`,
            }
          : {
              title: t("instances.title"),
              sub:
                instances.length === 1
                  ? t("instances.subtitle_one")
                  : t("instances.subtitle_many", { n: instances.length }),
            };
      case "discover":
        return { title: t("discover.title"), sub: t("discover.subtitle") };
      case "import":
        return { title: t("import.title"), sub: t("import.subtitle") };
      case "friends":
        return { title: t("friends.title"), sub: t("friends.subtitle") };
      case "diagnostics":
        return { title: t("diagnostics.title"), sub: t("diagnostics.subtitle") };
      case "plugins":
        return { title: t("plugins.title"), sub: t("plugins.subtitle") };
      case "accounts":
        return { title: t("accounts.title"), sub: t("accounts.subtitle") };
      case "settings":
        return { title: t("settings.title"), sub: t("settings.subtitle") };
    }
  }, [view, selected, instances.length]);

  return (
    <div className="app">
      <Sidebar
        view={view}
        onNavigate={(next) => {
          if (next === "diagnostics" && selectedId) setDiagnosticsTarget(selectedId);
          setView(next);
          if (next !== "instances") setSelectedId(null);
        }}
        accountName={activeAccount?.username ?? null}
        accountKind={activeAccount?.kind ?? null}
        accounts={accounts}
        onAccounts={setAccounts}
        theme={theme}
        onCycleTheme={() =>
          setTheme(theme === "dark" ? "light" : theme === "light" ? "system" : "dark")
        }
      />

      <main className="main">
        <UpdateBanner />
        <header className="topbar">
          <div className="topbar-heading">
            <h1 className="topbar-title" key={header.title}>
              {header.title}
            </h1>
            <div className="topbar-sub" key={header.sub}>
              {header.sub}
            </div>
          </div>
          <div className="topbar-actions">
            <button
              type="button"
              className="icon-btn"
              title={fullscreen ? t("shell.leave_fullscreen") : t("shell.fullscreen")}
              aria-label={fullscreen ? t("shell.leave_fullscreen") : t("shell.fullscreen")}
              onClick={() =>
                void api.window
                  .setFullscreen(!fullscreen)
                  .then(setFullscreen)
                  .catch(report)
              }
            >
              <Icon name={fullscreen ? "collapse" : "expand"} size={16} />
            </button>
            {view === "instances" ? (
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setCreateOpen(true)}
              >
                <Icon name="plus" size={15} /> {t("instances.new")}
              </button>
            ) : null}
            {view === "discover" ? (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  void refreshInstances();
                  notify(t("toast.refreshed"));
                }}
              >
                <Icon name="refresh" size={15} /> {t("common.refresh")}
              </button>
            ) : null}
          </div>
        </header>

        <div className="content">
          <div
            className="view"
            key={view === "instances" && selectedId ? `detail:${selectedId}` : view}
          >
            {view === "instances" ? (
              selectedId ? (
                <InstanceDetail
                  instanceId={selectedId}
                  running={running}
                  onRunningChanged={() => void refreshRunning()}
                  onBack={() => setSelectedId(null)}
                  onChanged={() => void refreshInstances()}
                  onError={report}
                  onNotify={notify}
                  onPackInstalled={(id) => {
                    void refreshInstances().then(() => openInstance(id));
                  }}
                />
              ) : (
                <InstanceGrid
                  instances={instances}
                  running={running}
                  onRunningChanged={() => void refreshRunning()}
                  loading={loading}
                  onCreate={() => setCreateOpen(true)}
                  onOpen={openInstance}
                  onChanged={() => void refreshInstances()}
                  onError={report}
                  onNotify={notify}
                />
              )
            ) : null}

            {view === "discover" ? (
              <>
                <div className="chips source-switch">
                  <button
                    type="button"
                    className={classNames("chip", discoverSource === "modrinth" && "active")}
                    onClick={() => setDiscoverSource("modrinth")}
                  >
                    Modrinth
                  </button>
                  <button
                    type="button"
                    className={classNames("chip", discoverSource === "curseforge" && "active")}
                    onClick={() => setDiscoverSource("curseforge")}
                  >
                    CurseForge
                  </button>
                </div>
                {discoverSource === "modrinth" ? (
                  <ModrinthPanel
                    instances={instances}
                    targetId={discoverTarget}
                    onTargetChange={setDiscoverTarget}
                    onInstalled={() => void refreshInstances()}
                    onError={report}
                    onPackInstalled={(id) => {
                      void refreshInstances().then(() => openInstance(id));
                    }}
                  />
                ) : (
                  <CurseForgePanel
                    instances={instances}
                    targetId={discoverTarget}
                    onTargetChange={setDiscoverTarget}
                    onError={report}
                  />
                )}
              </>
            ) : null}

            {view === "accounts" ? (
              <AccountsPanel accounts={accounts} authStatus={authStatus} onAccounts={setAccounts} />
            ) : null}

            {view === "diagnostics" ? (
              <DiagnosticsPanel instances={instances} initialId={diagnosticsTarget} />
            ) : null}

            {view === "plugins" ? <PluginsView /> : null}

            {view === "friends" ? (
              <FriendsPanel
                instances={instances}
                onImported={(id) => {
                  void refreshInstances().then(() => openInstance(id));
                }}
              />
            ) : null}

            {view === "import" ? (
              <ImportPanel
                onImported={(result) => {
                  void refreshInstances().then(() => openInstance(result.instance.id));
                }}
              />
            ) : null}

            {view === "settings" ? (
              <SettingsPanel
                settings={settings}
                theme={theme}
                accent={accent}
                onTheme={setTheme}
                onAccent={setAccent}
                onPreview={(next) =>
                  setAppearancePreview((current) => ({ ...(current ?? {}), ...next }))
                }
                onSaved={(next) => {
                  setAppearancePreview(null);
                  setSettings(next);
                  setLocale(next.locale);
                  void refreshAuthStatus();
                  notify(t("toast.settings_saved"), t("toast.settings_saved_text"));
                }}
                onError={report}
              />
            ) : null}
          </div>
        </div>
      </main>

      <CreateInstanceDialog
        open={createOpen}
        settings={settings}
        onClose={() => setCreateOpen(false)}
        onCreated={(instance) => {
          setCreateOpen(false);
          notify(t("toast.instance_created"), instance.name);
          void refreshInstances().then(() => openInstance(instance.id));
        }}
      />

      <ProgressToast event={progress} />
      <Toaster />
      <ConfirmHost />
    </div>
  );
}
