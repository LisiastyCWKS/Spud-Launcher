import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { bytes, classNames, loaderLabel } from "../lib/format";
import type { DetectedInstance, DetectedLauncher, ImportResult } from "../lib/types";
import { Icon } from "./Icon";

const LAUNCHER_LABEL: Record<string, string> = {
  prism: "Prism",
  multimc: "MultiMC",
  polymc: "PolyMC",
  modrinth: "Modrinth",
  curseforge: "CurseForge",
  atlauncher: "ATLauncher",
  gdlauncher: "GDLauncher",
  technic: "Technic",
  lunar: "Lunar",
  vanilla: "Minecraft Launcher",
};

/**
 * Two doors into the library: pull a profile out of another launcher, or open a
 * Spud export / mrpack archive. Both land as a normal instance — the game files
 * are fetched by the usual Install button afterwards.
 */
export function ImportPanel({
  onImported,
}: {
  onImported: (result: ImportResult) => void;
}) {
  const [launchers, setLaunchers] = useState<DetectedLauncher[]>([]);
  const [instances, setInstances] = useState<DetectedInstance[]>([]);
  const [scanning, setScanning] = useState(true);
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [versions, setVersions] = useState<Record<string, string>>({});
  const [archivePath, setArchivePath] = useState("");
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const scan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const result = await api.launchers.scan();
      setLaunchers(result.launchers);
      setInstances(result.instances);
      setOpen((current) => current ?? result.launchers[0]?.key ?? null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    void scan();
  }, [scan]);

  const importArchive = useCallback(
    async (path: string) => {
      if (!path.toLowerCase().endsWith(".zip") && !path.toLowerCase().endsWith(".mrpack")) {
        setError("drop a .zip or .mrpack archive");
        return;
      }
      setBusy("archive");
      setError(null);
      setNote(null);
      try {
        const result = await api.instances.import(path);
        setNote(`Imported "${result.instance.name}" (${result.files} files).`);
        onImported(result);
      } catch (cause) {
        setError(String(cause));
      } finally {
        setBusy(null);
      }
    },
    [onImported],
  );

  useEffect(() => {
    const disposers: Array<() => void> = [];
    let disposed = false;

    const attach = async () => {
      const offDrop = await listen<{ paths?: string[] }>("tauri://drag-drop", (event) => {
        setDragging(false);
        const path = event.payload?.paths?.[0];
        if (path) void importArchive(path);
      });
      const offEnter = await listen("tauri://drag-enter", () => setDragging(true));
      const offLeave = await listen("tauri://drag-leave", () => setDragging(false));
      if (disposed) {
        offDrop();
        offEnter();
        offLeave();
        return;
      }
      disposers.push(offDrop, offEnter, offLeave);
    };

    void attach();
    return () => {
      disposed = true;
      for (const dispose of disposers) dispose();
    };
  }, [importArchive]);

  const browseArchive = async () => {
    try {
      const picked = await openDialog({
        multiple: false,
        title: "Pick a Spud export or Modrinth pack",
        filters: [{ name: "Minecraft packs", extensions: ["zip", "mrpack"] }],
      });
      if (typeof picked === "string") {
        setArchivePath(picked);
        void importArchive(picked);
      }
    } catch (cause) {
      setError(String(cause));
    }
  };

  const importProfile = async (entry: DetectedInstance) => {
    const version = entry.mc_version ?? versions[entry.path] ?? "";
    if (!version.trim()) {
      setError(`"${entry.name}" does not record a Minecraft version — type it in first.`);
      return;
    }
    setBusy(entry.path);
    setError(null);
    setNote(null);
    try {
      const result = await api.launchers.import(entry, version);
      setNote(
        `Imported "${result.instance.name}" — ${result.copied} files copied${
          result.skipped > 0 ? `, ${result.skipped} skipped` : ""
        }. Install it to download the game.`,
      );
      onImported({ instance: result.instance, files: result.copied });
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  const groups = launchers.map((launcher) => ({
    launcher,
    items: instances.filter((entry) => entry.launcher_key === launcher.key),
  }));

  return (
    <div className="stack">
      <div className="panel">
        <div className="row-between">
          <div>
            <div className="panel-title">Import an archive</div>
            <div className="section-sub">
              A Spud export (.zip) or a Modrinth pack (.mrpack). Drop it anywhere in this window.
            </div>
          </div>
          <button type="button" className="btn btn-sm" onClick={() => void scan()}>
            <Icon name="refresh" size={14} /> Rescan
          </button>
        </div>

        <div
          className={classNames("dropzone", dragging && "active")}
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            setDragging(false);
            const file = event.dataTransfer.files?.[0];
            if (!file) return;
            // The webview cannot resolve an absolute path from a browser drop;
            // the Tauri event above handles that. This branch is the fallback
            // for when the file name is all that is available.
            setArchivePath(file.name);
            setNote("Use the Tauri drag-and-drop event or paste the full path below.");
          }}
        >
          <Icon name="package" size={22} />
          <div>{dragging ? "Drop it" : "Drag a .zip or .mrpack here"}</div>
          <div className="muted">or paste a path</div>
          <button type="button" className="btn btn-sm" style={{ marginTop: 6 }} onClick={() => void browseArchive()}>
            <Icon name="folder" size={14} /> Choose file
          </button>
        </div>

        <div className="row" style={{ marginTop: 12, gap: 8 }}>
          <input
            className="input mono"
            placeholder="C:\Users\you\Downloads\my-pack.zip"
            value={archivePath}
            onChange={(event) => setArchivePath(event.target.value)}
          />
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy !== null || !archivePath.trim()}
            onClick={() => void importArchive(archivePath.trim())}
          >
            {busy === "archive" ? <span className="spinner" /> : <Icon name="download" size={15} />} Import
          </button>
        </div>
      </div>

      <div className="panel">
        <div className="panel-title">Other launchers</div>
        <div className="section-sub">
          Detected from the usual install locations. Importing copies mods, configs, resource packs and
          worlds into a new isolated instance.
        </div>

        {scanning ? (
          <div className="empty small" style={{ marginTop: 14 }}>
            <span className="spinner" /> scanning…
          </div>
        ) : groups.length === 0 ? (
          <div className="empty small" style={{ marginTop: 14 }}>
            No other launcher found. Lunar, Prism, Modrinth App, CurseForge, ATLauncher, GDLauncher and
            a stock .minecraft folder are all checked.
          </div>
        ) : (
          <div className="import-groups" style={{ marginTop: 14 }}>
            {groups.map(({ launcher, items }) => (
              <div key={`${launcher.key}:${launcher.root}`} className="import-group">
                <button
                  type="button"
                  className={classNames("import-head", open === launcher.key && "active")}
                  onClick={() => setOpen(open === launcher.key ? null : launcher.key)}
                >
                  <Icon name={open === launcher.key ? "collapse" : "expand"} size={14} />
                  <span className="import-name">{LAUNCHER_LABEL[launcher.key] ?? launcher.name}</span>
                  <span className="badge">{items.length}</span>
                  <span className="muted mono import-root" title={launcher.root}>
                    {launcher.root}
                  </span>
                </button>

                {open === launcher.key ? (
                  <div className="import-items">
                    {items.map((entry) => (
                      <div key={entry.path} className="import-row">
                        <div style={{ minWidth: 0, flex: 1 }}>
                          <div className="import-item-name">{entry.name}</div>
                          <div className="muted small mono" title={entry.path}>
                            {entry.path}
                          </div>
                          <div className="instance-meta" style={{ marginTop: 6 }}>
                            {entry.mc_version ? (
                              <span className="badge">{entry.mc_version}</span>
                            ) : (
                              <input
                                className="input input-tiny"
                                placeholder="MC version"
                                value={versions[entry.path] ?? ""}
                                onChange={(event) =>
                                  setVersions((current) => ({
                                    ...current,
                                    [entry.path]: event.target.value,
                                  }))
                                }
                              />
                            )}
                            <span className="badge">{loaderLabel(entry.loader, entry.loader_version)}</span>
                            <span className="badge">{entry.mods} mods</span>
                            <span className="badge">{bytes(entry.bytes)}</span>
                          </div>
                        </div>
                        <button
                          type="button"
                          className="btn btn-sm btn-primary"
                          disabled={busy !== null}
                          onClick={() => void importProfile(entry)}
                        >
                          {busy === entry.path ? <span className="spinner" /> : <Icon name="download" size={14} />}
                          Import
                        </button>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>

      {note ? <div className="notice">{note}</div> : null}
      {error ? <div className="error">{error}</div> : null}
    </div>
  );
}
