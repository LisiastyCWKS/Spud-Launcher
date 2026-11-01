import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { classNames, copyText, relativeTime } from "../lib/format";
import type {
  FriendView,
  FriendsConfig,
  InstanceSummary,
  SharedArchive,
} from "../lib/types";
import { Icon } from "./Icon";

/**
 * Friends are a shared folder, not a service: each launcher writes
 * `<nick>.json` into a directory both sides sync, and reads everyone else's.
 * Instances travel as the normal export zip in the same folder.
 */
export function FriendsPanel({
  instances,
  onImported,
}: {
  instances: InstanceSummary[];
  onImported: (id: string) => void;
}) {
  const [config, setConfig] = useState<FriendsConfig | null>(null);
  const [nick, setNick] = useState("");
  const [folder, setFolder] = useState("");
  const [friendNick, setFriendNick] = useState("");
  const [friends, setFriends] = useState<FriendView[]>([]);
  const [archives, setArchives] = useState<SharedArchive[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [shareId, setShareId] = useState("");

  const adopt = useCallback((next: FriendsConfig) => {
    setConfig(next);
    setNick(next.nick);
    setFolder(next.sync_folder ?? "");
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [list, files] = await Promise.all([api.friends.list(), api.friends.archives()]);
      setFriends(list);
      setArchives(files);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        adopt(await api.friends.get());
      } catch (cause) {
        setError(String(cause));
      }
    })();
  }, [adopt]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 15_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const save = async () => {
    if (!config) return;
    setBusy("save");
    setError(null);
    setNote(null);
    try {
      const next = await api.friends.save({ ...config, nick, sync_folder: folder || null });
      adopt(next);
      await api.friends.publish().catch(() => null);
      await refresh();
      setNote("Saved. Presence is published every 45 seconds while the launcher runs.");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const browseFolder = async () => {
    try {
      const picked = await open({ directory: true, multiple: false, title: "Pick the folder both launchers can see" });
      if (typeof picked === "string") setFolder(picked);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const add = async () => {    if (!friendNick.trim()) return;
    setBusy("add");
    setError(null);
    try {
      adopt(await api.friends.add(friendNick));
      setFriendNick("");
      await refresh();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (value: string) => {
    setError(null);
    try {
      adopt(await api.friends.remove(value));
      await refresh();
    } catch (cause) {
      setError(String(cause));
    }
  };

  const share = async () => {
    if (!shareId) {
      setNote("Pick one of your instances first.");
      return;
    }
    setBusy("share");
    setError(null);
    setNote(null);
    try {
      const archive = await api.friends.share(shareId);
      setNote(`Shared as ${archive.file_name} — ${(archive.bytes / 1024 / 1024).toFixed(1)} MB in the shared folder.`);
      await refresh();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const take = async (archive: SharedArchive) => {
    setBusy(archive.path);
    setError(null);
    setNote(null);
    try {
      const result = await api.friends.import(archive.path);
      setNote(`Imported "${result.instance.name}" (${result.files} files). Install it to fetch the game files.`);
      onImported(result.instance.id);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="stack">
      <div className="panel">
        <div className="panel-title">Your presence</div>
        <div className="section-sub">
          Pick a folder that syncs between you and your friends — OneDrive, Dropbox, Syncthing, a
          network share. Each launcher writes <span className="mono">nick.json</span> there and reads
          the others.
        </div>

        <div className="form-grid" style={{ marginTop: 14 }}>
          <div className="field">
            <label htmlFor="friend-nick">Your nick</label>
            <input
              id="friend-nick"
              className="input"
              value={nick}
              placeholder="olivia"
              onChange={(event) => setNick(event.target.value)}
            />
            <div className="field-hint">Shown to everyone you add. Files are named after it.</div>
          </div>
          <div className="field">
            <label htmlFor="friend-folder">Shared folder</label>
            <div className="row" style={{ gap: 8 }}>
              <input
                id="friend-folder"
                className="input mono"
                value={folder}
                placeholder="C:\Users\you\OneDrive\spud-friends"
                onChange={(event) => setFolder(event.target.value)}
              />
              <button
                type="button"
                className="btn"
                onClick={() => void browseFolder()}
                title="Pick a folder"
              >
                <Icon name="folder" size={15} /> Browse
              </button>
            </div>
            <div className="field-hint">The same path on every launcher that shares it.</div>
          </div>
        </div>

        <div className="form-actions">
          <button type="button" className="btn btn-primary" onClick={() => void save()} disabled={busy !== null}>
            {busy === "save" ? <span className="spinner" /> : <Icon name="check" size={15} />} Save
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => void api.friends.publish().catch((cause) => setError(String(cause)))}
            disabled={busy !== null || !config?.nick}
          >
            <Icon name="refresh" size={15} /> Publish now
          </button>
          {config?.nick ? (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => void copyText(config.nick)}
              title="Copy your nick so a friend can add you"
            >
              <Icon name="copy" size={14} /> Copy nick
            </button>
          ) : null}
        </div>
      </div>

      <div className="panel">
        <div className="panel-title">Friends</div>
        <div className="section-sub">Add a nick of someone else running Spud Launcher.</div>

        <div className="row" style={{ marginTop: 12, gap: 8 }}>
          <input
            className="input"
            value={friendNick}
            placeholder="their nick"
            onChange={(event) => setFriendNick(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void add();
            }}
          />
          <button type="button" className="btn btn-primary" onClick={() => void add()} disabled={busy !== null}>
            <Icon name="plus" size={15} /> Add
          </button>
        </div>

        {friends.length === 0 ? (
          <div className="empty small" style={{ marginTop: 14 }}>
            No friends yet. Add a nick above — once they publish, their server and instances show up
            here.
          </div>
        ) : (
          <div className="friend-list">
            {friends.map((friend) => (
              <div key={friend.nick} className="friend-card">
                <div className="friend-head">
                  <div className="friend-avatar">{friend.nick.slice(0, 1).toUpperCase()}</div>
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div className="friend-name">
                      <span className={classNames("dot", friend.online && "ok")} />
                      {friend.nick}
                      <span className="badge">{friend.status}</span>
                    </div>
                    <div className="friend-sub">
                      {friend.online ? (
                        friend.server ? (
                          <>
                            on <span className="mono">{friend.server}</span>
                          </>
                        ) : (
                          "in the menus"
                        )
                      ) : (
                        `offline${friend.updated_at ? ` · seen ${relativeTime(friend.updated_at)}` : ""}`
                      )}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="icon-btn"
                    title="Remove this friend"
                    onClick={() => void remove(friend.nick)}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                </div>

                {friend.instance ? (
                  <div className="friend-playing">
                    <Icon name="play" size={13} /> {friend.instance}
                    {friend.mc_version ? <span className="muted"> · {friend.mc_version}</span> : null}
                    {friend.loader && friend.loader !== "vanilla" ? (
                      <span className="badge">{friend.loader}</span>
                    ) : null}
                  </div>
                ) : null}

                {friend.instances.length > 0 ? (
                  <div className="friend-instances">
                    {friend.instances.slice(0, 8).map((instance) => (
                      <span key={instance.id || instance.name} className="chip">
                        {instance.name}
                        <span className="muted"> {instance.mc_version}</span>
                      </span>
                    ))}
                  </div>
                ) : null}

                {friend.archives.length > 0 ? (
                  <div className="friend-archives">
                    {friend.archives.map((archive) => (
                      <button
                        key={archive.path}
                        type="button"
                        className="btn btn-sm"
                        disabled={busy !== null}
                        onClick={() => void take(archive)}
                        title={archive.path}
                      >
                        {busy === archive.path ? <span className="spinner" /> : <Icon name="download" size={14} />}
                        Copy {archive.instance}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="panel">
        <div className="panel-title">Share one of your instances</div>
        <div className="section-sub">
          Exports the instance as a zip into the shared folder. The friend clicks “Copy” and gets the
          whole pack — mods, configs and worlds.
        </div>
        <div className="row" style={{ marginTop: 12, gap: 8 }}>
          <select className="select" value={shareId} onChange={(event) => setShareId(event.target.value)}>
            <option value="">Pick an instance…</option>
            {instances.map((instance) => (
              <option key={instance.id} value={instance.id}>
                {instance.name} · {instance.mc_version}
              </option>
            ))}
          </select>
          <button type="button" className="btn btn-primary" onClick={() => void share()} disabled={busy !== null}>
            {busy === "share" ? <span className="spinner" /> : <Icon name="package" size={15} />} Share
          </button>
        </div>

        {archives.length > 0 ? (
          <div className="log-list" style={{ marginTop: 14 }}>
            {archives.map((archive) => (
              <div key={archive.path} className="log-row">
                <span className="log-name mono">{archive.file_name}</span>
                <span className="muted">{(archive.bytes / 1024 / 1024).toFixed(1)} MB</span>
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={busy !== null}
                  onClick={() => void take(archive)}
                >
                  Import
                </button>
              </div>
            ))}
          </div>
        ) : null}
      </div>

      {note ? <div className="notice">{note}</div> : null}
      {error ? <div className="error">{error}</div> : null}
    </div>
  );
}
