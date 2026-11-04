import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { classNames } from "../lib/format";
import type { PluginEntry } from "../lib/types";
import { setPluginEnabled } from "../lib/plugins";
import { Icon } from "./Icon";

/**
 * Plugin manager: what is on disk, what is on, what failed to load.
 *
 * Toggling goes through the runtime, not just the backend, so the effect is
 * immediate — no launcher restart to try a plugin on or off.
 */
export function PluginsPanel({ onError }: { onError: (message: string | null) => void }) {
  const [entries, setEntries] = useState<PluginEntry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [root, setRoot] = useState("");

  const load = useCallback(async () => {
    try {
      setEntries(await api.plugins.list());
    } catch (cause) {
      onError(String(cause));
    }
  }, [onError]);

  useEffect(() => {
    void load();
  }, [load]);

  const toggle = async (entry: PluginEntry) => {
    setBusy(entry.id);
    onError(null);
    try {
      await setPluginEnabled(entry.id, !entry.enabled);
      await load();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const openFolder = async () => {
    try {
      setRoot(await api.plugins.openFolder());
    } catch (cause) {
      onError(String(cause));
    }
  };

  return (
    <div className="card settings-card">
      {entries === null ? (
        <div className="row" style={{ gap: 8 }}>
          <span className="spinner" />
          <span className="muted">Scanning plugins…</span>
        </div>
      ) : entries.length === 0 ? (
        <p className="muted" style={{ fontSize: 12.5, margin: 0 }}>
          No plugins installed. Each plugin is a folder with{" "}
          <span className="mono">plugin.json</span> and an entry script.
        </p>
      ) : (
        <ul className="plugin-list">
          {entries.map((entry) => (
            <li key={entry.id} className="plugin-list-row">
              <label className="checkbox" style={{ margin: 0, flex: 1 }}>
                <input
                  type="checkbox"
                  checked={entry.enabled}
                  disabled={busy !== null || Boolean(entry.error)}
                  onChange={() => void toggle(entry)}
                />
                <span>
                  <span className="plugin-list-name">{entry.name}</span>{" "}
                  <span className="muted mono" style={{ fontSize: 11.5 }}>
                    {entry.id} · v{entry.version} · API {entry.api}
                  </span>
                </span>
              </label>
              {busy === entry.id ? <span className="spinner" /> : null}
            </li>
          ))}
        </ul>
      )}

      {entries
        ?.filter((entry) => entry.error)
        .map((entry) => (
          <div className="notice warn" key={`err-${entry.id}`} style={{ marginTop: 8 }}>
            <strong>{entry.id}</strong>: {entry.error}
          </div>
        ))}

      <div className="row" style={{ flexWrap: "wrap", gap: 8, marginTop: 10 }}>
        <button type="button" className="btn" onClick={() => void openFolder()}>
          <Icon name="folder" size={15} /> Open plugins folder
        </button>
        <button type="button" className="btn" onClick={() => void load()} disabled={busy !== null}>
          <Icon name="refresh" size={15} /> Rescan
        </button>
      </div>

      <span className={classNames("field-hint")}>
        Plugins are off until you enable them, and they run inside the launcher window — a plugin is
        code you trust. A plugin that keeps throwing is stopped automatically; its panels disappear
        and the error shows here.
      </span>
      {root ? <span className="field-hint mono">{root}</span> : null}
    </div>
  );
}
