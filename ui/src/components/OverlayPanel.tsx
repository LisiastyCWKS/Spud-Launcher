import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import type { LauncherSettings, OverlayStatus } from "../lib/types";
import { Icon } from "./Icon";

/**
 * Settings for the second, always-on-top window.
 *
 * The shortcut is part of the ordinary settings form (saved with the rest),
 * while the window itself is toggled through its own commands so the preview
 * button works before anything is saved.
 */
export function OverlayPanel({
  settings,
  patch,
  onError,
}: {
  settings: LauncherSettings;
  patch: (next: Partial<LauncherSettings>) => void;
  onError: (message: string | null) => void;
}) {
  const [status, setStatus] = useState<OverlayStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.overlay.status());
    } catch (cause) {
      onError(String(cause));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const preview = async () => {
    setBusy(true);
    onError(null);
    try {
      await api.overlay.toggle(!(status?.visible ?? false));
      await refresh();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="card settings-card">
      <label className="checkbox">
        <input
          type="checkbox"
          checked={settings.overlay_enabled}
          onChange={(event) => patch({ overlay_enabled: event.target.checked })}
        />
        Enable the in-game overlay
      </label>
      <span className="field-hint">
        A separate always-on-top window with live CPU/RAM, the running instance and its mod list.
        Off by default. The game must run in windowed or borderless mode — exclusive fullscreen puts
        the game above every other window, overlay included.
      </span>

      <div className="field" style={{ marginTop: 14 }}>
        <label htmlFor="set-overlay-hotkey">Shortcut</label>
        <input
          id="set-overlay-hotkey"
          className="input mono"
          value={settings.overlay_hotkey}
          placeholder="Shift+Tab"
          spellCheck={false}
          onChange={(event) => patch({ overlay_hotkey: event.target.value })}
        />
        <span className="field-hint">
          Format: <span className="mono">Ctrl+Shift+O</span>, <span className="mono">Alt+F1</span>,{" "}
          <span className="mono">Shift+Tab</span>. Saved with the form; the combination is claimed
          system-wide, so while the overlay is enabled the game no longer receives it.
        </span>
      </div>

      <div className="row" style={{ gap: 8, marginTop: 4 }}>
        <button type="button" className="btn" disabled={busy} onClick={() => void preview()}>
          {busy ? <span className="spinner" /> : <Icon name="monitor" size={15} />}
          {status?.visible ? "Hide preview" : "Preview overlay"}
        </button>
        <span className="field-hint" style={{ alignSelf: "center" }}>
          {status === null
            ? "Checking the window…"
            : !status.window
              ? "The overlay window is missing from this build."
              : !settings.overlay_enabled
                ? "Preview works while disabled; the shortcut stays unregistered until you save with the box ticked."
                : status.registered
                  ? `Registered: ${status.hotkey}${status.visible ? " · visible now" : ""}`
                  : status.error ?? "The shortcut is not registered."}
        </span>
      </div>
    </div>
  );
}
