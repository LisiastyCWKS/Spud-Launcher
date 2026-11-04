import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { classNames } from "../lib/format";
import type { InstanceConfig, TuningPreview } from "../lib/types";
import { Icon } from "./Icon";

/**
 * Hardware tuning for one instance.
 *
 * Read-only until Apply: `tuning_preview` re-scans the machine every time this
 * card mounts, so a RAM upgrade or a new GPU shows up without a launcher
 * restart. Apply writes heap + flags into `instance.json`; the Settings tab
 * still owns manual edits, and the tuner never deletes a user flag.
 */
export function PerformanceCard({
  config,
  onApplied,
  onError,
}: {
  config: InstanceConfig;
  onApplied: (next: InstanceConfig) => void;
  onError: (message: string | null) => void;
}) {
  const [preview, setPreview] = useState<TuningPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setPreview(await api.tuning.preview(config.id));
      onError(null);
    } catch (cause) {
      onError(String(cause));
    } finally {
      setLoading(false);
    }
  }, [config.id, onError]);

  useEffect(() => {
    void load();
  }, [load]);

  const apply = async () => {
    setApplying(true);
    setNote(null);
    try {
      const next = await api.tuning.apply(config.id);
      onApplied(next);
      setNote(
        `Applied ${next.java.min_memory_mb}–${next.java.max_memory_mb} MB and ${
          preview?.tuning.jvm_args.length ?? 0
        } JVM flags.`,
      );
      await load();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setApplying(false);
    }
  };

  if (loading && !preview) {
    return (
      <div className="card panel">
        <h3 className="panel-title">Performance</h3>
        <div className="row" style={{ gap: 8 }}>
          <span className="spinner" />
          <span className="muted">Scanning this machine…</span>
        </div>
      </div>
    );
  }

  if (!preview) {
    return (
      <div className="card panel">
        <h3 className="panel-title">Performance</h3>
        <p className="muted" style={{ fontSize: 12.5, margin: 0 }}>
          Hardware scan unavailable. Heap and JVM flags stay as configured in the Settings tab.
        </p>
      </div>
    );
  }

  const { profile, tuning } = preview;
  const unchanged =
    preview.current &&
    config.java.max_memory_mb === tuning.max_memory_mb &&
    config.java.min_memory_mb === tuning.min_memory_mb;

  return (
    <div className="card panel">
      <div className="row-between" style={{ marginBottom: 10 }}>
        <h3 className="panel-title" style={{ margin: 0 }}>
          Performance
        </h3>
        <span className={classNames("badge", unchanged ? "badge-accent" : "badge-warn")}>
          {unchanged ? "tuned" : preview.applied_at ? "machine changed" : "not tuned"}
        </span>
      </div>

      <dl className="kv">
        <dt>CPU</dt>
        <dd>
          {profile.cpu_brand}
          <span className="muted">
            {" "}
            · {profile.cpu_cores} cores / {profile.cpu_threads} threads
          </span>
        </dd>
        <dt>RAM</dt>
        <dd>
          {(profile.memory_total_mb / 1024).toFixed(0)} GB total
          <span className="muted"> · {(profile.memory_available_mb / 1024).toFixed(1)} GB free</span>
        </dd>
        <dt>GPU</dt>
        <dd>
          {profile.gpu ?? "not detected"}
          {profile.gpu_kind === "integrated" ? (
            <span className="muted"> · shared memory</span>
          ) : null}
        </dd>
        <dt>Java</dt>
        <dd>
          {preview.java_major}
          <span className="muted"> · {preview.modded ? "modded profile" : "vanilla profile"}</span>
        </dd>
        <dt>Recommended heap</dt>
        <dd className="mono">
          {tuning.min_memory_mb}–{tuning.max_memory_mb} MB
          <span className="muted">
            {" "}
            (now {config.java.min_memory_mb}–{config.java.max_memory_mb} MB)
          </span>
        </dd>
      </dl>

      {tuning.notes.length > 0 ? (
        <ul className="tuning-notes">
          {tuning.notes.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      ) : null}

      <details className="tuning-flags">
        <summary>{tuning.jvm_args.length} recommended JVM flags</summary>
        <pre className="mono tuning-flags-body">{tuning.jvm_args.join("\n")}</pre>
      </details>

      <div className="row" style={{ marginTop: 12, gap: 8 }}>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => void apply()}
          disabled={applying}
        >
          {applying ? <span className="spinner" /> : <Icon name="refresh" size={15} />}
          Apply recommended settings
        </button>
        <button type="button" className="btn" onClick={() => void load()} disabled={applying}>
          Re-scan
        </button>
      </div>

      <span className="field-hint">
        Heap lands in the instance's Java settings; flags are appended to JVM args and can be edited
        by hand in the Settings tab. Existing custom flags are never removed.
      </span>
      {note ? <div className="notice" style={{ marginTop: 10 }}>{note}</div> : null}
    </div>
  );
}
