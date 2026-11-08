import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { classNames, copyText, loaderLabel, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import { confirmDialog } from "../lib/confirm";
import type {
  InstanceConfig,
  InstalledMod,
  JavaRuntime,
  ModUpdate,
  OptimizationResult,
} from "../lib/types";
import { Icon } from "./Icon";
import { ModrinthPanel } from "./ModrinthPanel";
import { ConsolePanel } from "./ConsolePanel";
import { PerformanceCard } from "./PerformanceCard";
import type { RunningMap } from "../hooks/useRunning";

type Tab = "overview" | "mods" | "console" | "settings";

const TYPE_LABEL: Record<string, string> = {
  mod: "Mod",
  resourcepack: "Resource pack",
  shader: "Shader",
  modpack: "Modpack",
};

export function InstanceDetail({
  instanceId,
  running,
  onBack,
  onChanged,
  onRunningChanged,
  onPackInstalled,
  onError,
  onNotify,
}: {
  instanceId: string;
  running: RunningMap;
  onBack: () => void;
  onChanged: () => void;
  onRunningChanged: () => void;
  onPackInstalled: (id: string) => void;
  onError?: (cause: unknown) => void;
  onNotify?: (title: string, detail?: string) => void;
}) {
  const [config, setConfig] = useState<InstanceConfig | null>(null);
  const [mods, setMods] = useState<InstalledMod[]>([]);
  const [updates, setUpdates] = useState<ModUpdate[] | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState<
    "install" | "launch" | "stop" | "remove" | "export-zip" | "export-mrpack" | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  /**
   * Failures land in two places on purpose: the inline line keeps the reason
   * next to the button that caused it, and the toast catches the case where the
   * user has already scrolled somewhere else.
   */
  const fail = useCallback(
    (cause: unknown) => {
      setError(String(cause));
      onError?.(cause);
    },
    [onError],
  );
  const announce = useCallback(
    (title: string, detail?: string) => {
      onNotify?.(title, detail);
    },
    [onNotify],
  );

  const game = running[instanceId] ?? null;

  const load = useCallback(async () => {
    setError(null);
    try {
      const [next, installed] = await Promise.all([
        api.instances.get(instanceId),
        api.mods.installed(instanceId),
      ]);
      setConfig(next);
      setMods(installed);
    } catch (cause) {
      fail(cause);
    }
  }, [instanceId, fail]);

  useEffect(() => {
    setConfig(null);
    setMods([]);
    setUpdates(null);
    setTab("overview");
    void load();
  }, [load]);

  const ready = config?.game.version_id != null;

  const install = async () => {
    setBusy("install");
    setError(null);
    setNote(null);
    try {
      const result = await api.instances.install(instanceId);
      setConfig(result.instance);
      setNote(`Installed ${result.version_id} — ${result.files} files.`);
      announce(t("toast.installed"), `${result.version_id} · ${t("toast.files", { n: result.files })}`);
      onChanged();
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const launch = async () => {
    setBusy("launch");
    setError(null);
    setNote(null);
    try {
      const result = await api.instances.launch(instanceId);
      setNote(`Game running (pid ${result.pid}). Log: ${result.log_file}`);
      announce(t("toast.launched"), config?.name);
      onRunningChanged();
      onChanged();
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const stop = async () => {
    if (!config || !game) return;
    setBusy("stop");
    setError(null);
    setNote(null);
    try {
      const result = await api.instances.stop(instanceId);
      setNote(result.pid ? `Stopped the game (pid ${result.pid}).` : "The game had already exited.");
      announce(t("toast.stopped"), config.name);
      onRunningChanged();
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    if (!config) return;
    const runningNote = game ? ` ${t("confirm.delete_running")}` : "";
    const confirmed = await confirmDialog({
      title: t("confirm.delete_title", { name: config.name }),
      description: `${t("confirm.delete_body")}${runningNote}`,
      confirmLabel: t("common.delete"),
      cancelLabel: t("common.cancel"),
      tone: "danger",
    });
    if (!confirmed) return;
    setBusy("remove");
    setError(null);
    try {
      await api.instances.remove(instanceId);
      announce(t("toast.instance_deleted"), config.name);
      onRunningChanged();
      onChanged();
      onBack();
    } catch (cause) {
      fail(cause);
      setBusy(null);
    }
  };

  const copyFolder = async () => {
    try {
      const folder = await api.instances.folder(instanceId);
      await copyText(folder);
      setNote("Instance folder copied to the clipboard.");
      announce(t("toast.folder_copied"), folder);
    } catch (cause) {
      fail(cause);
    }
  };

  const exportAs = async (format: "zip" | "mrpack") => {
    setBusy(format === "zip" ? "export-zip" : "export-mrpack");
    setError(null);
    setNote(null);
    try {
      const result = await api.instances.export(instanceId, format);
      await copyText(result.path);
      setNote(
        `Exported ${result.files} files to ${result.path} — the path is on your clipboard.`,
      );
      announce(t("toast.exported"), result.path);
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  if (!config) {
    return (
      <div className="empty">
        {error ? <div className="error">{error}</div> : <span className="spinner" />}
      </div>
    );
  }

  return (
    <div>
      <button type="button" className="back" onClick={onBack}>
        <Icon name="back" size={14} /> All instances
      </button>

      <div className="detail-head">
        <div className="detail-icon">
          {config.icon ? <img src={config.icon} alt="" /> : config.name.slice(0, 1).toUpperCase()}
        </div>
        <div style={{ minWidth: 0 }}>
          <h1 className="detail-title">{config.name}</h1>
          <div className="instance-meta">
            <span className="badge">{config.game.version}</span>
            <span className="badge">{loaderLabel(config.game.loader, config.game.loader_version)}</span>
            <span className={classNames("badge", ready ? "badge-accent" : "badge-warn")}>
              {ready ? "Ready" : "Not installed"}
            </span>
            {game ? <span className="badge badge-accent">Running · pid {game.pid}</span> : null}
          </div>
        </div>
        <div className="detail-actions">
          <button type="button" className="btn" onClick={copyFolder} title="Show the game directory">
            <Icon name="folder" size={15} /> Folder
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => void exportAs("zip")}
            disabled={busy !== null || game !== null}
            title="Pack the whole instance (mods, configs, worlds) into one zip"
          >
            {busy === "export-zip" ? <span className="spinner" /> : <Icon name="package" size={15} />}
            Export
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => void exportAs("mrpack")}
            disabled={busy !== null || game !== null}
            title="Modrinth pack with CDN links — small, shareable, importable anywhere"
          >
            {busy === "export-mrpack" ? <span className="spinner" /> : <Icon name="download" size={15} />}
            .mrpack
          </button>
          <button
            type="button"
            className="btn"
            onClick={install}
            disabled={busy !== null || game !== null}
            title={game ? "Stop the game before repairing files" : undefined}
          >
            {busy === "install" ? <span className="spinner" /> : <Icon name="download" size={15} />}
            {ready ? "Repair" : "Install"}
          </button>
          {game ? (
            <button
              type="button"
              className="btn btn-danger"
              onClick={stop}
              disabled={busy !== null}
              title={`Stop the game (pid ${game.pid})`}
            >
              {busy === "stop" ? <span className="spinner" /> : <Icon name="stop" size={15} />}
              Stop
            </button>
          ) : (
            <button
              type="button"
              className="btn btn-primary"
              onClick={launch}
              disabled={busy !== null || !ready}
            >
              {busy === "launch" ? <span className="spinner" /> : <Icon name="play" size={15} />}
              Play
            </button>
          )}
        </div>
      </div>

      <div className="tabs" role="tablist">
        {(
          [
            ["overview", "Overview"],
            ["mods", `Mods${mods.length > 0 ? ` (${mods.length})` : ""}`],
            ["console", "Console"],
            ["settings", "Settings"],
          ] as Array<[Tab, string]>
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={classNames("tab", tab === id && "active")}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>

      {note ? <div className="notice" style={{ marginBottom: 14 }}>{note}</div> : null}
      {error ? <div className="error" style={{ marginBottom: 14 }}>{error}</div> : null}

      {tab === "overview" ? (
        <OverviewTab
          config={config}
          modCount={mods.length}
          ready={ready}
          onApplied={setConfig}
          onError={setError}
        />
      ) : null}

      {tab === "mods" ? (
        <ModsTab
          instanceId={instanceId}
          mods={mods}
          updates={updates}
          onMods={setMods}
          onUpdates={setUpdates}
          onError={setError}
          onChanged={onChanged}
          onPackInstalled={onPackInstalled}
          config={config}
        />
      ) : null}

      {tab === "console" ? <ConsolePanel instanceId={instanceId} onError={setError} height={460} /> : null}

      {tab === "settings" ? (
        <SettingsTab
          config={config}
          onSaved={(next) => {
            setConfig(next);
            onChanged();
          }}
          onRemove={remove}
          removing={busy === "remove"}
        />
      ) : null}
    </div>
  );
}

function OverviewTab({
  config,
  modCount,
  ready,
  onApplied,
  onError,
}: {
  config: InstanceConfig;
  modCount: number;
  ready: boolean;
  onApplied: (config: InstanceConfig) => void;
  onError: (message: string | null) => void;
}) {
  const folder = useMemo(() => config.id, [config.id]);

  return (
    <div className="panel-grid">
      <div className="card panel">
        <h3 className="panel-title">Game</h3>
        <dl className="kv">
          <dt>Minecraft</dt>
          <dd>{config.game.version}</dd>
          <dt>Loader</dt>
          <dd>{loaderLabel(config.game.loader, config.game.loader_version)}</dd>
          <dt>Version id</dt>
          <dd className="mono">{config.game.version_id ?? "not resolved yet"}</dd>
          <dt>Installed</dt>
          <dd>{ready ? "yes" : "no"}</dd>
          <dt>Memory</dt>
          <dd>
            {config.java.min_memory_mb}–{config.java.max_memory_mb} MB
          </dd>
          <dt>Java</dt>
          <dd className="mono">
            {config.java.mode === "manual" && config.java.path ? config.java.path : "auto-detect"}
          </dd>
          <dt>Window</dt>
          <dd>
            {config.window.width}×{config.window.height}
            {config.window.fullscreen ? " · fullscreen" : ""}
          </dd>
          <dt>Last played</dt>
          <dd>{relativeTime(config.last_played_at)}</dd>
          <dt>Mods managed</dt>
          <dd>{modCount}</dd>
        </dl>
      </div>

      <div className="stack">
        <PerformanceCard config={config} onApplied={onApplied} onError={onError} />

        <div className="card panel">
          <h3 className="panel-title">Files</h3>
          <p className="muted" style={{ fontSize: 12.5, margin: "0 0 10px" }}>
            Instance <span className="mono">{folder}</span> keeps its own mods, saves and config.
            Libraries, assets and versions are shared with every other instance.
          </p>
          <dl className="kv">
            <dt>JVM args</dt>
            <dd className="mono">
              {config.jvm_args.length > 0 ? config.jvm_args.join(" ") : "defaults"}
            </dd>
            <dt>Game args</dt>
            <dd className="mono">
              {config.game_args.length > 0 ? config.game_args.join(" ") : "defaults"}
            </dd>
            <dt>Env vars</dt>
            <dd className="mono">
              {Object.keys(config.env).length > 0 ? Object.keys(config.env).join(", ") : "none"}
            </dd>
          </dl>
        </div>

        <div className="card panel">
          <h3 className="panel-title">Notes</h3>
          <p className="soft" style={{ fontSize: 12.5, margin: 0, whiteSpace: "pre-wrap" }}>
            {config.notes.trim() || "No notes for this instance."}
          </p>
        </div>
      </div>
    </div>
  );
}

function ModsTab({
  instanceId,
  mods,
  updates,
  onMods,
  onUpdates,
  onError,
  onChanged,
  onPackInstalled,
  config,
}: {
  instanceId: string;
  mods: InstalledMod[];
  updates: ModUpdate[] | null;
  onMods: (mods: InstalledMod[]) => void;
  onUpdates: (updates: ModUpdate[] | null) => void;
  onError: (message: string | null) => void;
  onChanged: () => void;
  onPackInstalled: (id: string) => void;
  config: InstanceConfig;
}) {
  const [pending, setPending] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [optimizing, setOptimizing] = useState(false);
  const [optimizeResult, setOptimizeResult] = useState<OptimizationResult | null>(null);

  const optimize = async () => {
    setOptimizing(true);
    onError(null);
    setOptimizeResult(null);
    try {
      const result = await api.mods.optimize(instanceId);
      onMods(result.installed);
      setOptimizeResult(result);
      onChanged();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setOptimizing(false);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    onError(null);
    try {
      onUpdates(await api.mods.updates(instanceId));
    } catch (cause) {
      onError(String(cause));
    } finally {
      setChecking(false);
    }
  };

  const applyUpdate = async (update: ModUpdate) => {
    setPending(update.project_id);
    onError(null);
    try {
      onMods(await api.mods.install(instanceId, update.project_id, update.latest_version_id));
      onUpdates((updates ?? []).filter((entry) => entry.project_id !== update.project_id));
      onChanged();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setPending(null);
    }
  };

  const remove = async (mod: InstalledMod) => {
    setPending(mod.project_id);
    onError(null);
    try {
      onMods(await api.mods.remove(instanceId, mod.project_id));
      onUpdates((updates ?? []).filter((entry) => entry.project_id !== mod.project_id));
      onChanged();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setPending(null);
    }
  };

  const nonMods = mods.filter((mod) => mod.project_type !== "mod");

  return (
    <div className="stack">
      <div className="row-between">
        <div>
          <h3 className="section-title">Installed content</h3>
          <div className="section-sub">
            {mods.length} file{mods.length === 1 ? "" : "s"} tracked for this instance
            {nonMods.length > 0 ? ` · ${nonMods.length} pack/shader` : ""}
          </div>
        </div>
        <div className="row" style={{ gap: 8 }}>
          {config.game.loader !== "vanilla" ? (
            <button
              type="button"
              className="btn btn-primary"
              onClick={optimize}
              disabled={optimizing}
              title="Install Sodium/Lithium/Iris (or Embeddium/Oculus on Forge) matched to this version"
            >
              {optimizing ? <span className="spinner" /> : <Icon name="package" size={15} />}
              {optimizing ? "Optimizing…" : "Optimize"}
            </button>
          ) : null}
          <button type="button" className="btn" onClick={checkUpdates} disabled={checking}>
            {checking ? <span className="spinner" /> : <Icon name="refresh" size={15} />}
            Check updates
          </button>
        </div>
      </div>

      {optimizeResult ? (
        <div className="card panel">
          <h3 className="panel-title">Optimization stack</h3>
          <div className="log-list">
            {optimizeResult.installed
              .filter((mod) => mod.project_type === "mod")
              .map((mod) => (
                <div key={mod.project_id} className="log-row">
                  <span style={{ fontWeight: 600 }}>{mod.project_title}</span>
                  <span className="muted mono">{mod.version_number}</span>
                </div>
              ))}
            {optimizeResult.skipped.map((entry) => (
              <div key={entry} className="log-row">
                <span className="muted">skipped {entry}</span>
              </div>
            ))}
          </div>
          <div className="field-hint">
            {optimizeResult.installed.length} file
            {optimizeResult.installed.length === 1 ? "" : "s"} installed
            {optimizeResult.skipped.length > 0
              ? ` · ${optimizeResult.skipped.length} skipped (no build for ${config.game.version})`
              : ""}
            . Restart the game to pick them up.
          </div>
        </div>
      ) : null}

      {updates !== null && updates.length > 0 ? (
        <div className="card panel">
          <h3 className="panel-title">Updates available</h3>
          <div className="log-list">
            {updates.map((update) => (
              <div key={update.project_id} className="log-row">
                <span style={{ fontWeight: 600 }}>{update.title}</span>
                <span className="muted mono">
                  {update.installed_version} → {update.latest_version}
                </span>
                <span className="spacer" />
                <button
                  type="button"
                  className="btn btn-sm btn-primary"
                  onClick={() => applyUpdate(update)}
                  disabled={pending !== null}
                >
                  {pending === update.project_id ? <span className="spinner" /> : null}
                  Update
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {updates !== null && updates.length === 0 ? (
        <div className="notice">Everything tracked is on the newest compatible build.</div>
      ) : null}

      {mods.length === 0 ? (
        <div className="empty">
          <h3>Nothing installed yet</h3>
          <p style={{ margin: "0 auto", maxWidth: 420 }}>
            Search Modrinth below — dependencies are resolved automatically and the loader is
            matched to {config.game.version} / {loaderLabel(config.game.loader, config.game.loader_version)}.
          </p>
        </div>
      ) : (
        <div className="card">
          {mods.map((mod) => (
            <div key={mod.project_id} className="account-row">
              <div className="account-avatar">
                <Icon name={mod.project_type === "mod" ? "package" : "grid"} size={16} />
              </div>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div className="account-name">{mod.project_title}</div>
                <div className="muted" style={{ fontSize: 12 }}>
                  {TYPE_LABEL[mod.project_type] ?? mod.project_type} · {mod.version_number} ·{" "}
                  <span className="mono">{mod.file_name}</span>
                </div>
              </div>
              <button
                type="button"
                className="btn btn-sm btn-danger"
                onClick={() => remove(mod)}
                disabled={pending !== null}
              >
                {pending === mod.project_id ? <span className="spinner" /> : <Icon name="trash" size={14} />}
                Remove
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="card panel">
        <h3 className="panel-title">Add content</h3>
        <ModrinthPanel
          instances={[]}
          targetId={instanceId}
          onTargetChange={() => undefined}
          fixedTarget
          onInstalled={async () => {
            onMods(await api.mods.installed(instanceId));
            onChanged();
          }}
          onError={onError}
          onPackInstalled={onPackInstalled}
          gameVersion={config.game.version}
          loader={config.game.loader === "vanilla" ? null : config.game.loader}
        />
      </div>
    </div>
  );
}

function SettingsTab({
  config,
  onSaved,
  onRemove,
  removing,
}: {
  config: InstanceConfig;
  onSaved: (config: InstanceConfig) => void;
  onRemove: () => void;
  removing: boolean;
}) {
  const [draft, setDraft] = useState<InstanceConfig>(config);
  const [runtimes, setRuntimes] = useState<JavaRuntime[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(config);
  }, [config]);

  const patch = (next: Partial<InstanceConfig>) => setDraft((current) => ({ ...current, ...next }));

  const detect = async () => {
    setDetecting(true);
    setError(null);
    try {
      setRuntimes(await api.java.detect());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setDetecting(false);
    }
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const next = await api.instances.update({
        ...draft,
        name: draft.name.trim() || config.name,
      });
      onSaved(next);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="stack" style={{ maxWidth: 780 }}>
      <div className="card settings-card">
        <div className="form-grid">
          <div className="field">
            <label htmlFor="inst-name">Name</label>
            <input
              id="inst-name"
              className="input"
              value={draft.name}
              onChange={(event) => patch({ name: event.target.value })}
            />
          </div>
          <div className="field">
            <label htmlFor="inst-min">Min memory (MB)</label>
            <input
              id="inst-min"
              className="input"
              type="number"
              min={256}
              max={65536}
              step={256}
              value={draft.java.min_memory_mb}
              onChange={(event) =>
                patch({ java: { ...draft.java, min_memory_mb: Number(event.target.value) } })
              }
            />
          </div>
          <div className="field">
            <label htmlFor="inst-max">Max memory (MB)</label>
            <input
              id="inst-max"
              className="input"
              type="number"
              min={512}
              max={65536}
              step={256}
              value={draft.java.max_memory_mb}
              onChange={(event) =>
                patch({ java: { ...draft.java, max_memory_mb: Number(event.target.value) } })
              }
            />
          </div>
          <div className="field">
            <label htmlFor="inst-width">Window width</label>
            <input
              id="inst-width"
              className="input"
              type="number"
              min={320}
              max={7680}
              value={draft.window.width}
              onChange={(event) =>
                patch({ window: { ...draft.window, width: Number(event.target.value) } })
              }
            />
          </div>
          <div className="field">
            <label htmlFor="inst-height">Window height</label>
            <input
              id="inst-height"
              className="input"
              type="number"
              min={240}
              max={4320}
              value={draft.window.height}
              onChange={(event) =>
                patch({ window: { ...draft.window, height: Number(event.target.value) } })
              }
            />
          </div>
        </div>

        <label className="checkbox">
          <input
            type="checkbox"
            checked={draft.window.fullscreen}
            onChange={(event) =>
              patch({ window: { ...draft.window, fullscreen: event.target.checked } })
            }
          />
          Start fullscreen
        </label>

        <div className="field">
          <label>Java runtime</label>
          <div className="chips" style={{ marginBottom: 0 }}>
            <button
              type="button"
              className={classNames("chip", draft.java.mode === "auto" && "active")}
              onClick={() => patch({ java: { ...draft.java, mode: "auto" } })}
            >
              Auto
            </button>
            <button
              type="button"
              className={classNames("chip", draft.java.mode === "manual" && "active")}
              onClick={() => patch({ java: { ...draft.java, mode: "manual" } })}
            >
              Manual
            </button>
            <button type="button" className="chip" onClick={detect} disabled={detecting}>
              {detecting ? "Detecting…" : "Detect runtimes"}
            </button>
          </div>
          {draft.java.mode === "manual" ? (
            <div className="stack-sm" style={{ marginTop: 8 }}>
              {runtimes.length > 0 ? (
                <select
                  className="select"
                  value={draft.java.path ?? ""}
                  onChange={(event) =>
                    patch({ java: { ...draft.java, path: event.target.value || null } })
                  }
                >
                  <option value="">Pick a runtime…</option>
                  {runtimes.map((runtime) => (
                    <option key={runtime.path} value={runtime.path}>
                      Java {runtime.major} · {runtime.vendor} {runtime.version}
                    </option>
                  ))}
                </select>
              ) : null}
              <input
                className="input mono"
                value={draft.java.path ?? ""}
                placeholder="C:\\Program Files\\Java\\jdk-21\\bin\\javaw.exe"
                onChange={(event) => patch({ java: { ...draft.java, path: event.target.value || null } })}
              />
            </div>
          ) : (
            <span className="field-hint">
              The newest detected runtime that satisfies this version's requirement is used.
            </span>
          )}
        </div>

        <div className="field">
          <label htmlFor="inst-jvm">Extra JVM arguments</label>
          <textarea
            id="inst-jvm"
            className="textarea mono"
            value={draft.jvm_args.join("\n")}
            placeholder={"-XX:+UseG1GC\n-XX:MaxGCPauseMillis=50"}
            onChange={(event) =>
              patch({ jvm_args: event.target.value.split("\n").map((line) => line.trim()).filter(Boolean) })
            }
          />
          <span className="field-hint">One flag per line, appended after the launcher's defaults.</span>
        </div>

        <div className="field">
          <label htmlFor="inst-game">Extra game arguments</label>
          <textarea
            id="inst-game"
            className="textarea mono"
            value={draft.game_args.join("\n")}
            placeholder="--demo"
            onChange={(event) =>
              patch({ game_args: event.target.value.split("\n").map((line) => line.trim()).filter(Boolean) })
            }
          />
        </div>

        <div className="field">
          <label htmlFor="inst-env">Environment variables</label>
          <textarea
            id="inst-env"
            className="textarea mono"
            value={Object.entries(draft.env)
              .map(([key, value]) => `${key}=${value}`)
              .join("\n")}
            placeholder="MESA_GL_VERSION_OVERRIDE=4.5"
            onChange={(event) =>
              patch({
                env: Object.fromEntries(
                  event.target.value
                    .split("\n")
                    .map((line) => line.split("="))
                    .filter((parts) => parts.length >= 2 && parts[0].trim().length > 0)
                    .map((parts) => [parts[0].trim(), parts.slice(1).join("=").trim()]),
                ),
              })
            }
          />
        </div>

        <div className="field">
          <label htmlFor="inst-notes">Notes</label>
          <textarea
            id="inst-notes"
            className="textarea"
            value={draft.notes}
            placeholder="Mod list, seed, anything worth remembering."
            onChange={(event) => patch({ notes: event.target.value })}
          />
        </div>

        {error ? <div className="error">{error}</div> : null}

        <div className="form-actions">
          <button type="button" className="btn btn-primary" onClick={save} disabled={saving}>
            {saving ? <span className="spinner" /> : <Icon name="check" size={15} />}
            Save changes
          </button>
        </div>
      </div>

      <div className="card settings-card">
        <div>
          <h3 className="settings-title" style={{ color: "var(--danger)" }}>
            Danger zone
          </h3>
          <p className="settings-sub" style={{ marginBottom: 0 }}>
            Deletes the instance directory, including saves, mods and screenshots. Shared libraries
            and assets stay on disk for the other instances. A running game is stopped first.
          </p>
        </div>
        <div className="form-actions" style={{ marginTop: 0 }}>
          <button type="button" className="btn btn-danger" onClick={onRemove} disabled={removing}>
            {removing ? <span className="spinner" /> : <Icon name="trash" size={15} />}
            Delete instance
          </button>
        </div>
      </div>
    </div>
  );
}
