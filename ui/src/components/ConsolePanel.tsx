import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, onGameLog } from "../lib/api";
import { classNames, copyText } from "../lib/format";
import { Icon } from "./Icon";

/** Kept in memory per console. A modded launch can print 10k lines; the last
 *  few thousand are the ones that matter when something breaks. */
const MAX_LINES = 3000;

type Level = "all" | "warn" | "error";

function levelOf(line: string): Exclude<Level, "all"> | null {
  if (/\b(ERROR|FATAL|SEVERE)\b/.test(line)) return "error";
  if (/\b(WARN|WARNING)\b/.test(line)) return "warn";
  return null;
}

export function ConsolePanel({
  instanceId,
  onError,
  height = 420,
}: {
  instanceId: string;
  onError?: (message: string) => void;
  height?: number;
}) {
  const [lines, setLines] = useState<string[]>([]);
  const [path, setPath] = useState<string>("");
  const [exists, setExists] = useState(true);
  const [filter, setFilter] = useState<Level>("all");
  const [follow, setFollow] = useState(true);
  const [copied, setCopied] = useState(false);
  const [loading, setLoading] = useState(true);
  const body = useRef<HTMLDivElement | null>(null);

  const push = useCallback((incoming: string[]) => {
    if (incoming.length === 0) return;
    setLines((current) => {
      const next = current.concat(incoming);
      return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
    });
  }, []);

  useEffect(() => {
    let alive = true;
    setLines([]);
    setLoading(true);
    setExists(true);

    (async () => {
      try {
        const snapshot = await api.logs.snapshot(instanceId);
        if (!alive) return;
        setPath(snapshot.path);
        setExists(snapshot.exists);
        setLines(snapshot.lines.slice(-MAX_LINES));
        // The launch path already started a tailer; this only claims a slot
        // when none is running, and a `false` answer is not an error.
        await api.logs.stream(instanceId).catch(() => false);
      } catch (cause) {
        if (alive) onError?.(String(cause));
      } finally {
        if (alive) setLoading(false);
      }
    })();

    return () => {
      alive = false;
    };
  }, [instanceId, onError]);

  useEffect(() => {
    let disposed = false;
    let off: (() => void) | null = null;
    void onGameLog((chunk) => {
      if (chunk.instance_id === instanceId) push(chunk.lines);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else off = unlisten;
    });
    return () => {
      disposed = true;
      off?.();
    };
  }, [instanceId, push]);

  const visible = useMemo(
    () => (filter === "all" ? lines : lines.filter((line) => levelOf(line) === filter)),
    [lines, filter],
  );

  useEffect(() => {
    if (!follow) return;
    const node = body.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [visible, follow]);

  const copyAll = async () => {
    try {
      await copyText(visible.join("\n"));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch (cause) {
      onError?.(String(cause));
    }
  };

  return (
    <div className="console">
      <div className="console-bar">
        <div className="console-path mono" title={path}>
          {path || "logs/game.log"}
          {!exists && !loading ? <span className="badge badge-warn">no log yet</span> : null}
        </div>
        <div className="console-actions">
          <div className="chips console-filter">
            {(
              [
                ["all", "All"],
                ["warn", "Warnings"],
                ["error", "Errors"],
              ] as Array<[Level, string]>
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                className={classNames("chip", filter === id && "active")}
                onClick={() => setFilter(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className={classNames("btn btn-sm", follow && "btn-ghost")}
            onClick={() => setFollow((value) => !value)}
            title="Scroll to the newest line automatically"
          >
            {follow ? "Following" : "Follow"}
          </button>
          <button type="button" className="btn btn-sm" onClick={() => void copyAll()}>
            <Icon name={copied ? "check" : "copy"} size={14} /> {copied ? "Copied" : "Copy"}
          </button>
          <button
            type="button"
            className="btn btn-sm btn-ghost"
            onClick={() => setLines([])}
            title="Clear the view (the file on disk is untouched)"
          >
            Clear
          </button>
        </div>
      </div>

      <div className="console-body mono" ref={body} style={{ height }}>
        {loading ? (
          <div className="console-empty">
            <span className="spinner" /> reading the log…
          </div>
        ) : visible.length === 0 ? (
          <div className="console-empty">
            {lines.length === 0
              ? "Nothing here yet — launch the instance and the console fills itself."
              : `No ${filter === "error" ? "errors" : "warnings"} in the last ${lines.length} lines.`}
          </div>
        ) : (
          visible.map((line, index) => {
            const level = levelOf(line);
            return (
              <div key={`${index}-${line.slice(0, 12)}`} className={classNames("console-line", level)}>
                {line}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
