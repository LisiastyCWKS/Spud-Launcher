import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, onInstallProgress } from "./lib/api";
import type {
  AccountsView,
  AuthStatus,
  DownloadEvent,
  LauncherSettings,
  InstanceSummary,
} from "./lib/types";
import { useTheme } from "./hooks/useTheme";
import { useAccent } from "./hooks/useAccent";
import { useRunning } from "./hooks/useRunning";
import { Sidebar, type View } from "./components/Sidebar";
import { InstanceGrid } from "./components/InstanceGrid";
import { InstanceDetail } from "./components/InstanceDetail";
import { CreateInstanceDialog } from "./components/CreateInstanceDialog";
import { ModrinthPanel } from "./components/ModrinthPanel";
import { AccountsPanel } from "./components/AccountsPanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { FriendsPanel } from "./components/FriendsPanel";
import { ImportPanel } from "./components/ImportPanel";
import { ProgressToast } from "./components/ProgressToast";
import { PluginsView } from "./components/PluginsView";
import { emitPluginEvent, reloadPlugins } from "./lib/plugins";
import { Icon } from "./components/Icon";

export default function App() {
  const { theme, resolved, setTheme } = useTheme();
  const { accent, setAccent } = useAccent(resolved);
  const { running, refresh: refreshRunning } = useRunning();

  const [view, setView] = useState<View>("instances");
  const [instances, setInstances] = useState<InstanceSummary[]>([]);
  const [settings, setSettings] = useState<LauncherSettings | null>(null);
  const [accounts, setAccounts] = useState<AccountsView | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [discoverTarget, setDiscoverTarget] = useState<string | null>(null);
  const [diagnosticsTarget, setDiagnosticsTarget] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [createOpen, setCreateOpen] = useState(false);
  const [progress, setProgress] = useState<DownloadEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);

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
      setError(String(cause));
    }
  }, []);

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
    void reloadPlugins().catch((cause) => setError(String(cause)));
  }, []);

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
      } catch (cause) {
        if (alive) setError(String(cause));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const attach = async (channel: "install://progress" | "mods://progress") => {
      const unlisten = await onInstallProgress((event) => {
        if (!disposed) setProgress(event);
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
  }, []);

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
          ? { title: selected.name, sub: `${selected.mc_version} · ${selected.ready ? "ready" : "not installed"}` }
          : {
              title: "Instances",
              sub: `${instances.length} instance${instances.length === 1 ? "" : "s"} · shared libraries and assets`,
            };
      case "discover":
        return { title: "Discover", sub: "Mods, modpacks, resource packs and shaders from Modrinth" };
      case "import":
        return {
          title: "Import",
          sub: "Other launchers on this PC, or a .zip / .mrpack from a friend",
        };
      case "friends":
        return {
          title: "Friends",
          sub: "Who is online, which server they are on, and one-click instance copies",
        };
      case "diagnostics":
        return { title: "Diagnostics", sub: "Live console and the crash logs left by the last runs" };
      case "plugins":
        return { title: "Plugins", sub: "Extensions you installed and what they added to the launcher" };
      case "accounts":
        return { title: "Accounts", sub: "Microsoft sign-in or a local offline nickname" };
      case "settings":
        return { title: "Settings", sub: "Downloads, Java runtimes, endpoints and data" };
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
        <header className="topbar">
          <div style={{ minWidth: 0 }}>
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
              title={fullscreen ? "Leave fullscreen (Esc)" : "Fullscreen (F11)"}
              onClick={() => void api.window.setFullscreen(!fullscreen).then(setFullscreen).catch(() => undefined)}
            >
              <Icon name={fullscreen ? "collapse" : "expand"} size={16} />
            </button>
            {view === "instances" ? (
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setCreateOpen(true)}
              >
                <Icon name="plus" size={15} /> New instance
              </button>
            ) : null}
            {view === "discover" ? (
              <button type="button" className="btn" onClick={() => void refreshInstances()}>
                <Icon name="refresh" size={15} /> Refresh
              </button>
            ) : null}
          </div>
        </header>

        <div className="content">
          {error ? (
            <div className="notice warn" style={{ marginBottom: 16 }}>
              <span className="row-between">
                <span>{error}</span>
                <button type="button" className="btn btn-sm" onClick={() => setError(null)}>
                  Dismiss
                </button>
              </span>
            </div>
          ) : null}

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
                />
              )
            ) : null}

            {view === "discover" ? (
              <ModrinthPanel
                instances={instances}
                targetId={discoverTarget}
                onTargetChange={setDiscoverTarget}
                onInstalled={() => void refreshInstances()}
                onError={setError}
                onPackInstalled={(id) => {
                  void refreshInstances().then(() => openInstance(id));
                }}
              />
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
                onSaved={(next) => {
                  setSettings(next);
                  void refreshAuthStatus();
                }}
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
          void refreshInstances().then(() => openInstance(instance.id));
        }}
      />

      <ProgressToast event={progress} />
    </div>
  );
}
