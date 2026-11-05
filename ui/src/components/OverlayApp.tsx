import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import type { OverlayPayload } from "../lib/types";

/** One second is the cadence the sampler is designed for (CPU is a delta). */
const POLL_MS = 1000;
/** Mod titles shown before the list collapses into "+N more". */
const MOD_ROWS = 7;

const LOADERS: Record<string, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

function percent(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `${Math.round(value)}%`;
}

function gigabytes(megabytes: number | null | undefined): string {
  if (megabytes === null || megabytes === undefined) return "—";
  const gb = megabytes / 1024;
  return gb >= 10 ? `${gb.toFixed(1)} GB` : `${gb.toFixed(2)} GB`;
}

function uptime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
}

function clampPercent(value: number | null | undefined): number {
  if (value === null || value === undefined) return 0;
  return Math.max(0, Math.min(100, value));
}

export function OverlayApp() {
  const [payload, setPayload] = useState<OverlayPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hotkey, setHotkey] = useState("Shift+Tab");
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;

    const pull = async () => {
      try {
        const next = await api.overlay.payload();
        if (mounted.current) {
          setPayload(next);
          setError(null);
        }
      } catch (cause) {
        if (mounted.current) setError(String(cause));
      }
    };

    void pull();
    const timer = window.setInterval(() => void pull(), POLL_MS);

    // Showing the window is the moment the first reading matters; the regular
    // interval would otherwise paint a stale second before catching up.
    const unlistenShown = listen("overlay://shown", () => void pull());
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void api.overlay.close();
    };
    window.addEventListener("keydown", onKey);

    api.overlay
      .status()
      .then((status) => {
        if (mounted.current) setHotkey(status.hotkey);
      })
      .catch(() => undefined);

    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      window.removeEventListener("keydown", onKey);
      void unlistenShown.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const instance = payload?.instance ?? null;
  const metrics = payload?.metrics ?? null;
  const memoryPercent =
    metrics?.memory_total_mb && metrics.memory_used_mb !== null
      ? (metrics.memory_used_mb / metrics.memory_total_mb) * 100
      : null;

  const loader = instance
    ? LOADERS[instance.loader] ?? instance.loader
    : null;

  const hiddenMods = payload ? Math.max(0, payload.mods_total - MOD_ROWS) : 0;

  return (
    <div className="ov-shell">
      <div className="ov-panel">
        <header className="ov-head" data-tauri-drag-region>
          <span className="ov-mark" data-tauri-drag-region>
            SPUD<span className="ov-mark-dot">.</span>
          </span>
          <span className="ov-chip">{hotkey}</span>
          <button
            type="button"
            className="ov-close"
            title="Hide the overlay"
            onClick={() => void api.overlay.close()}
          >
            ✕
          </button>
        </header>

        <section className="ov-instance">
          {instance ? (
            <>
              <h1 className="ov-name" title={instance.name}>
                {instance.name}
              </h1>
              <p className="ov-meta">
                {instance.mc_version} · {loader}
                {instance.loader_version ? ` ${instance.loader_version}` : ""} ·{" "}
                {uptime(instance.uptime_secs)}
              </p>
            </>
          ) : (
            <>
              <h1 className="ov-name">No game running</h1>
              <p className="ov-meta">Machine counters only. Launch an instance to see its jar list.</p>
            </>
          )}
        </section>

        <section className="ov-metrics">
          <div className="ov-meter">
            <div className="ov-meter-top">
              <span className="ov-label">CPU</span>
              <span className="ov-value">{percent(metrics?.cpu_percent)}</span>
            </div>
            <div className="ov-bar">
              <span
                className="ov-bar-fill"
                style={{ width: `${clampPercent(metrics?.cpu_percent)}%` }}
              />
            </div>
          </div>

          <div className="ov-meter">
            <div className="ov-meter-top">
              <span className="ov-label">System RAM</span>
              <span className="ov-value">
                {gigabytes(metrics?.memory_used_mb)} / {gigabytes(metrics?.memory_total_mb)}
              </span>
            </div>
            <div className="ov-bar">
              <span className="ov-bar-fill" style={{ width: `${clampPercent(memoryPercent)}%` }} />
            </div>
          </div>

          <div className="ov-pair">
            <div className="ov-stat">
              <span className="ov-label">Game RSS</span>
              <span className="ov-value">{gigabytes(metrics?.process_rss_mb)}</span>
            </div>
            <div className="ov-stat">
              <span className="ov-label">Heap cap</span>
              <span className="ov-value">
                {instance ? gigabytes(instance.max_memory_mb) : "—"}
              </span>
            </div>
            <div className="ov-stat">
              <span className="ov-label">PID</span>
              <span className="ov-value mono">{metrics?.process_id ?? "—"}</span>
            </div>
          </div>
        </section>

        <section className="ov-mods">
          <div className="ov-mods-head">
            <span className="ov-label">Mods</span>
            <span className="ov-value">{payload ? payload.mods_total : "—"}</span>
          </div>
          {payload && payload.mods.length > 0 ? (
            <ul className="ov-mod-list">
              {payload.mods.slice(0, MOD_ROWS).map((mod) => (
                <li key={`${mod.title}-${mod.file_name}`} className="ov-mod">
                  <span className="ov-mod-title" title={mod.file_name}>
                    {mod.title}
                  </span>
                  <span className="ov-mod-version mono">{mod.version}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="ov-empty">
              {instance ? "No jars in this instance." : "Nothing to list until a game starts."}
            </p>
          )}
          {hiddenMods > 0 ? <p className="ov-more">+{hiddenMods} more</p> : null}
        </section>

        <footer className="ov-foot">
          {error ? (
            <span className="ov-error" title={error}>
              Counters unavailable
            </span>
          ) : (
            <span>{hotkey} toggles · click ✕ to hide</span>
          )}
          <span className="ov-foot-mark">ofx</span>
        </footer>
      </div>
    </div>
  );
}
