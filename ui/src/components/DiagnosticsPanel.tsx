import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { bytes, classNames, copyText, relativeTime } from "../lib/format";
import type { InstanceSummary, LogFileInfo } from "../lib/types";
import { ConsolePanel } from "./ConsolePanel";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

/**
 * Everything needed to answer "why did it crash": a live console, the log and
 * crash-report files the last runs left behind, and one-click copying for
 * pasting into a bug report.
 */
export function DiagnosticsPanel({
  instances,
  initialId,
}: {
  instances: InstanceSummary[];
  initialId: string | null;
}) {
  const [selected, setSelected] = useState<string | null>(initialId ?? instances[0]?.id ?? null);
  const [files, setFiles] = useState<LogFileInfo[]>([]);
  const [viewing, setViewing] = useState<LogFileInfo | null>(null);
  const [content, setContent] = useState("");
  const [loadingFile, setLoadingFile] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!selected && instances.length > 0) setSelected(instances[0].id);
    if (selected && !instances.some((entry) => entry.id === selected)) {
      setSelected(instances[0]?.id ?? null);
    }
  }, [instances, selected]);

  const refreshFiles = useCallback(async () => {
    if (!selected) return;
    try {
      setFiles(await api.logs.files(selected));
    } catch (cause) {
      setError(String(cause));
    }
  }, [selected]);

  useEffect(() => {
    setFiles([]);
    void refreshFiles();
  }, [refreshFiles, reload]);

  const open = async (entry: LogFileInfo) => {
    if (!selected) return;
    setViewing(entry);
    setLoadingFile(true);
    setContent("");
    try {
      setContent(await api.logs.read(selected, entry.name));
    } catch (cause) {
      setContent(String(cause));
    } finally {
      setLoadingFile(false);
    }
  };

  const copyFile = async (entry: LogFileInfo) => {
    if (!selected) return;
    try {
      const text = await api.logs.read(selected, entry.name);
      await copyText(text);
      setCopied(entry.name);
      window.setTimeout(() => setCopied(null), 1600);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const crashes = files.filter((entry) => entry.name.startsWith("crash-reports/"));
  const logs = files.filter((entry) => entry.name.startsWith("logs/"));

  return (
    <div className="stack">
      <div className="panel">
        <div className="row-between" style={{ marginBottom: 12 }}>
          <div>
            <div className="panel-title">Console</div>
            <div className="section-sub">
              Live output from the running instance. Copy it straight into a report when a mod
              misbehaves.
            </div>
          </div>
          <button type="button" className="btn btn-sm" onClick={() => setReload((value) => value + 1)}>
            <Icon name="refresh" size={14} /> Refresh files
          </button>
        </div>

        {instances.length === 0 ? (
          <div className="empty">Create an instance first — there is nothing to watch yet.</div>
        ) : (
          <>
            <div className="chips" style={{ marginBottom: 12 }}>
              {instances.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  className={classNames("chip", selected === entry.id && "active")}
                  onClick={() => setSelected(entry.id)}
                >
                  {entry.name}
                </button>
              ))}
            </div>
            {selected ? (
              <ConsolePanel instanceId={selected} onError={setError} height={380} />
            ) : null}
          </>
        )}
      </div>

      {error ? <div className="error">{error}</div> : null}

      {selected ? (
        <div className="panel-grid">
          <div className="panel">
            <div className="panel-title">Crash reports</div>
            <div className="section-sub">Written to crash-reports/ by Minecraft itself.</div>
            {crashes.length === 0 ? (
              <div className="empty small">No crash reports — the last run either worked or died before it could write one.</div>
            ) : (
              <div className="log-list">
                {crashes.map((entry) => (
                  <div key={entry.path} className="log-row">
                    <button type="button" className="log-name" onClick={() => void open(entry)}>
                      <Icon name="warning" size={14} /> {entry.name.replace("crash-reports/", "")}
                    </button>
                    <span className="muted mono">{bytes(entry.bytes)}</span>
                    <span className="muted">{relativeTime(entry.modified)}</span>
                    <button
                      type="button"
                      className="icon-btn"
                      title="Copy the whole report"
                      onClick={() => void copyFile(entry)}
                    >
                      <Icon name={copied === entry.name ? "check" : "copy"} size={14} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="panel">
            <div className="panel-title">Logs</div>
            <div className="section-sub">Newest first. game.log is the console you are watching.</div>
            {logs.length === 0 ? (
              <div className="empty small">The logs folder is empty.</div>
            ) : (
              <div className="log-list">
                {logs.slice(0, 12).map((entry) => (
                  <div key={entry.path} className="log-row">
                    <button type="button" className="log-name" onClick={() => void open(entry)}>
                      <Icon name="folder" size={14} /> {entry.name.replace("logs/", "")}
                    </button>
                    <span className="muted mono">{bytes(entry.bytes)}</span>
                    <span className="muted">{relativeTime(entry.modified)}</span>
                    <button
                      type="button"
                      className="icon-btn"
                      title="Copy this log"
                      onClick={() => void copyFile(entry)}
                    >
                      <Icon name={copied === entry.name ? "check" : "copy"} size={14} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      ) : null}

      {viewing ? (
        <Modal title={viewing.name} subtitle={viewing.path} width={880} onClose={() => setViewing(null)}>
          <div className="modal-actions">
            <button
              type="button"
              className="btn"
              onClick={() => void copyText(content).then(() => setCopied(viewing.name))}
            >
              <Icon name="copy" size={14} /> Copy everything
            </button>
          </div>
          <pre className="code-box mono" style={{ maxHeight: 460, overflow: "auto" }}>
            {loadingFile ? "reading…" : content}
          </pre>
        </Modal>
      ) : null}
    </div>
  );
}
