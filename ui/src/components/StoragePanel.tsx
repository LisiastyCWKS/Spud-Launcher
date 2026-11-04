import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { bytes } from "../lib/format";
import type { DedupeReport, GcReport, StoreStats } from "../lib/types";
import { Icon } from "./Icon";

/**
 * Mod store maintenance.
 *
 * Two irreversible-ish actions and both are gated: GC always runs as a dry run
 * first and the sweep button only appears once the dry run has reported what it
 * would free. Deduplication itself is safe — it only replaces an instance copy
 * with a hard link to identical bytes already in the store.
 */
export function StoragePanel({ onError }: { onError: (message: string | null) => void }) {
  const [stats, setStats] = useState<StoreStats | null>(null);
  const [dryRun, setDryRun] = useState<GcReport | null>(null);
  const [dedupe, setDedupe] = useState<DedupeReport | null>(null);
  const [busy, setBusy] = useState<"stats" | "dedupe" | "gc" | "sweep" | null>(null);

  const loadStats = useCallback(async () => {
    try {
      setStats(await api.store.stats());
    } catch (cause) {
      onError(String(cause));
    }
  }, [onError]);

  useEffect(() => {
    void loadStats();
  }, [loadStats]);

  const runDedupe = async () => {
    setBusy("dedupe");
    setDedupe(null);
    onError(null);
    try {
      const instances = await api.instances.list();
      const totals: DedupeReport = {
        examined: 0,
        reused: 0,
        added: 0,
        linked: 0,
        copied: 0,
        bytes_saved: 0,
        problems: [],
      };
      for (const instance of instances) {
        const report = await api.store.dedupe(instance.id);
        totals.examined += report.examined;
        totals.reused += report.reused;
        totals.added += report.added;
        totals.linked += report.linked;
        totals.copied += report.copied;
        totals.bytes_saved += report.bytes_saved;
        if (report.problems.length > 0) {
          totals.problems.push(...report.problems.map((line) => `${instance.name}: ${line}`));
        }
      }
      setDedupe(totals);
      await loadStats();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const runDry = async () => {
    setBusy("gc");
    onError(null);
    try {
      setDryRun(await api.store.gc(true));
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const runSweep = async () => {
    setBusy("sweep");
    onError(null);
    try {
      const report = await api.store.gc(false);
      setDryRun(null);
      setStats((current) =>
        current ? { ...current, blobs: current.blobs - report.removed, bytes: current.bytes - report.bytes_freed } : current,
      );
      await loadStats();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="card settings-card">
      <dl className="kv">
        <dt>Stored blobs</dt>
        <dd>{stats ? stats.blobs.toLocaleString() : "…"}</dd>
        <dt>Store size</dt>
        <dd className="mono">{stats ? bytes(stats.bytes) : "…"}</dd>
        <dt>Location</dt>
        <dd className="mono" style={{ wordBreak: "break-all" }}>
          {stats?.index_path.replace(/[\\/]index\.json$/, "") ?? "…"}
        </dd>
      </dl>

      <p className="muted" style={{ fontSize: 12.5, margin: "0 0 12px" }}>
        Mods are stored once per unique SHA-256 and hard-linked into every instance. The game, pack
        exports and `.mrpack` files see ordinary jars — nothing else needs to know.
      </p>

      <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
        <button
          type="button"
          className="btn"
          onClick={() => void runDedupe()}
          disabled={busy !== null}
        >
          {busy === "dedupe" ? <span className="spinner" /> : <Icon name="package" size={15} />}
          Deduplicate all instances
        </button>
        <button type="button" className="btn" onClick={() => void runDry()} disabled={busy !== null}>
          {busy === "gc" ? <span className="spinner" /> : <Icon name="search" size={15} />}
          Check for unused blobs
        </button>
        {dryRun && dryRun.removed > 0 ? (
          <button
            type="button"
            className="btn btn-danger"
            onClick={() => void runSweep()}
            disabled={busy !== null}
          >
            {busy === "sweep" ? <span className="spinner" /> : <Icon name="trash" size={15} />}
            Free {bytes(dryRun.bytes_freed)}
          </button>
        ) : null}
      </div>

      {dedupe ? (
        <div className="notice" style={{ marginTop: 10 }}>
          Examined {dedupe.examined} files · {dedupe.reused} already stored · {dedupe.added} new ·{" "}
          {dedupe.linked} linked · {dedupe.copied} copied
          {dedupe.bytes_saved > 0 ? ` · ${bytes(dedupe.bytes_saved)} reclaimed` : ""}
          {dedupe.problems.length > 0 ? (
            <ul style={{ margin: "8px 0 0", paddingLeft: 18 }}>
              {dedupe.problems.slice(0, 5).map((line) => (
                <li key={line} className="mono" style={{ fontSize: 11.5 }}>
                  {line}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      {dryRun ? (
        <div className={dryRun.removed > 0 ? "notice warn" : "notice"} style={{ marginTop: 10 }}>
          {dryRun.kept} blobs in use
          {dryRun.removed > 0
            ? ` · ${dryRun.removed} unused (${bytes(dryRun.bytes_freed)}) can be freed`
            : " · nothing to free"}
        </div>
      ) : null}

      <span className="field-hint">
        Copies appear instead of links when the store and the instance live on different drives;
        those are never deleted behind the game's back.
      </span>
    </div>
  );
}
