import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { bytes, relativeTime } from "../lib/format";
import type { LauncherSettings, SyncRemoteView, SyncStatus } from "../lib/types";
import { Icon } from "./Icon";

/** Windows is case-insensitive and both separators are legal, so compare the
 * draft path the same way the filesystem would. */
function samePath(left: string | null, right: string | null): boolean {
  const normalise = (value: string | null) =>
    (value ?? "").trim().replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  return normalise(left) === normalise(right);
}

function stamp(iso: string | null): string {
  if (!iso) return "never";
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "never" : date.toLocaleString();
}

/**
 * Instance backups against a folder — a mounted drive, a network share or a
 * folder another tool already syncs.
 *
 * Two deliberate shapes here. The settings half edits the *draft* form like
 * every other settings card, but the buttons below call commands that read the
 * *saved* settings, so the panel says plainly when the two disagree instead of
 * failing with "cloud sync is not configured" on a folder the user just typed.
 * The restore half never overwrites: a pull imports the archive as a new
 * instance, so "replace my copy" is an explicit delete-then-restore.
 */
export function SyncPanel({
  settings,
  patch,
  onError,
}: {
  settings: LauncherSettings;
  patch: (next: Partial<LauncherSettings>) => void;
  onError: (message: string | null) => void;
}) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [remotes, setRemotes] = useState<SyncRemoteView[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await api.sync.status();
      setStatus(next);
      setRemotes(next.writable ? await api.sync.remotes() : []);
    } catch (cause) {
      onError(String(cause));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  /** The panel edits the draft; commands read what is on disk. */
  const unsaved =
    status !== null && (!samePath(settings.sync_folder, status.folder) || settings.sync_keep !== status.keep);

  const pickFolder = async () => {
    onError(null);
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Pick a folder for instance backups",
      });
      if (typeof picked === "string") {
        patch({ sync_folder: picked });
      }
    } catch (cause) {
      onError(String(cause));
    }
  };

  const openFolder = async () => {
    setBusy("open");
    onError(null);
    try {
      await api.sync.openFolder();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const push = async (instanceId: string, name: string) => {
    setBusy(`push:${instanceId}`);
    onError(null);
    setNote(null);
    try {
      const result = await api.sync.push(instanceId);
      setNote(
        result.deduplicated
          ? `${name} is already on the remote — the newest snapshot is byte-identical, nothing was uploaded.`
          : `Backed up ${name}: ${bytes(result.archive_bytes)} from ${result.files} files, ${relativeTime(
              result.snapshot.created_at,
            )}.${result.removed.length > 0 ? ` Pruned ${result.removed.length} older snapshot(s).` : ""}`,
      );
      await refresh();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const pull = async (instanceId: string, file: string) => {
    setBusy(`pull:${file}`);
    onError(null);
    setNote(null);
    try {
      const result = await api.sync.pull(instanceId, file);
      setNote(`Restored as a new instance: “${result.instance.name}” (${result.files} files).`);
      await refresh();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const forget = async (instanceId: string, file: string) => {
    if (!window.confirm("Delete this snapshot from the remote? This cannot be undone.")) return;
    setBusy(`forget:${file}`);
    onError(null);
    setNote(null);
    try {
      await api.sync.forget(instanceId, file);
      setNote("Snapshot deleted from the remote.");
      await refresh();
    } catch (cause) {
      onError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <div className="card settings-card">
        <div className="field">
          <label htmlFor="set-sync-folder">Backup folder</label>
          <div className="row" style={{ gap: 8 }}>
            <input
              id="set-sync-folder"
              className="input mono"
              value={settings.sync_folder ?? ""}
              placeholder="D:\\SpudBackups or \\\\nas\\minecraft"
              spellCheck={false}
              onChange={(event) => patch({ sync_folder: event.target.value || null })}
            />
            <button
              type="button"
              className="btn"
              disabled={busy !== null}
              onClick={() => void pickFolder()}
            >
              <Icon name="folder" size={15} />
              Browse
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy !== null || !settings.sync_folder}
              onClick={() => void openFolder()}
            >
              {busy === "open" ? <span className="spinner" /> : <Icon name="external" size={15} />}
              Open
            </button>
          </div>
          <span className="field-hint">
            A local folder, a mounted drive or a network share. Anything another tool already syncs
            (OneDrive, Syncthing, a NAS) works too — this launcher does not run a background uploader.
          </span>
        </div>

        <div className="field" style={{ marginBottom: 0 }}>
          <label htmlFor="set-sync-keep">Snapshots kept per instance</label>
          <input
            id="set-sync-keep"
            className="input mono"
            type="number"
            min={1}
            max={50}
            style={{ maxWidth: 120 }}
            value={settings.sync_keep}
            onChange={(event) => patch({ sync_keep: Number(event.target.value) || 5 })}
          />
          <span className="field-hint">
            Older snapshots are deleted when a new one is pushed. The newest is never pruned:
            identical bytes are not uploaded twice, so a repeated backup costs nothing.
          </span>
        </div>

        {unsaved ? (
          <div className="notice warn" style={{ marginTop: 12 }}>
            These settings are not saved yet. Back up and restore below use the saved values — save
            the form first.
          </div>
        ) : null}

        {status && status.configured && !status.reachable ? (
          <div className="notice warn" style={{ marginTop: 10 }}>
            The folder is not reachable right now. Connect the drive and press Refresh.
          </div>
        ) : null}

        {status && status.reachable && !status.writable ? (
          <div className="notice warn" style={{ marginTop: 10 }}>
            The folder is read-only — restoring works, backing up does not.
          </div>
        ) : null}

        <div className="row" style={{ gap: 8, marginTop: 12 }}>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy !== null}
            onClick={() => {
              void refresh();
            }}
          >
            <Icon name="refresh" size={15} />
            Refresh
          </button>
          {note ? <span className="field-hint">{note}</span> : null}
        </div>
      </div>

      <div className="card settings-card">
        <div className="section-sub" style={{ marginTop: 0 }}>
          This device
        </div>
        {status === null ? (
          <span className="field-hint">Reading the sync state…</span>
        ) : status.local.length === 0 ? (
          <span className="field-hint">No instances yet.</span>
        ) : (
          <dl className="kv">
            {status.local.map((entry) => (
              <div key={entry.instance_id} style={{ display: "contents" }}>
                <dt>
                  {entry.name}{" "}
                  {entry.remote_newer ? (
                    <span className="badge" title="Another device pushed since this one last looked">
                      newer on remote
                    </span>
                  ) : null}
                </dt>
                <dd>
                  <div className="row" style={{ gap: 8, justifyContent: "flex-end" }}>
                    <span className="muted" style={{ fontSize: 12 }}>
                      backed up {relativeTime(entry.last_push_at)}
                      {entry.last_pull_at ? ` · restored ${relativeTime(entry.last_pull_at)}` : ""}
                    </span>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      disabled={busy !== null || !status.writable}
                      onClick={() => void push(entry.instance_id, entry.name)}
                    >
                      {busy === `push:${entry.instance_id}` ? (
                        <span className="spinner" />
                      ) : (
                        <Icon name="download" size={15} />
                      )}
                      Back up
                    </button>
                  </div>
                </dd>
              </div>
            ))}
          </dl>
        )}
      </div>

      <div className="card settings-card">
        <div className="section-sub" style={{ marginTop: 0 }}>
          On the remote
        </div>
        {remotes.length === 0 ? (
          <span className="field-hint">
            No backups yet. Every snapshot is a full instance — worlds, configs and hand-installed
            jars included.
          </span>
        ) : (
          remotes.map((remote) => (
            <div key={remote.instance_id} className="sync-remote">
              <div className="row" style={{ justifyContent: "space-between", alignItems: "baseline" }}>
                <strong>{remote.name || remote.instance_id}</strong>
                <span className="muted" style={{ fontSize: 12 }}>
                  updated {stamp(remote.updated_at)} · {remote.snapshots.length} snapshot
                  {remote.snapshots.length === 1 ? "" : "s"}
                </span>
              </div>
              {remote.snapshots.map((snapshot) => (
                <div
                  key={snapshot.file}
                  className="row"
                  style={{ justifyContent: "space-between", gap: 8, marginTop: 8 }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div className="mono" style={{ fontSize: 12 }}>
                      {stamp(snapshot.created_at)} · {bytes(snapshot.bytes)}
                    </div>
                    <div className="muted" style={{ fontSize: 11.5 }}>
                      {snapshot.mc_version}
                      {snapshot.loader !== "vanilla" ? ` · ${snapshot.loader}` : ""}
                      {snapshot.origin ? ` · from ${snapshot.origin}` : ""}
                      {snapshot.note ? ` · ${snapshot.note}` : ""}
                    </div>
                  </div>
                  <div className="row" style={{ gap: 8 }}>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      disabled={busy !== null}
                      onClick={() => void pull(remote.instance_id, snapshot.file)}
                    >
                      {busy === `pull:${snapshot.file}` ? (
                        <span className="spinner" />
                      ) : (
                        <Icon name="refresh" size={15} />
                      )}
                      Restore
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      disabled={busy !== null}
                      onClick={() => void forget(remote.instance_id, snapshot.file)}
                    >
                      {busy === `forget:${snapshot.file}` ? (
                        <span className="spinner" />
                      ) : (
                        <Icon name="trash" size={15} />
                      )}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ))
        )}
        <span className="field-hint">
          Restoring never overwrites an instance: the archive is verified against its SHA-256 and
          imported as a new one. To replace a local copy, delete it and restore.
        </span>
      </div>
    </>
  );
}
