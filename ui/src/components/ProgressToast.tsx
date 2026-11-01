import { useEffect, useState } from "react";
import type { DownloadEvent } from "../lib/types";
import { bytes } from "../lib/format";

/** How long the finished toast stays up before it starts leaving, and how long
 *  the exit frame lasts — the second number matches `toast-out` in app.css. */
const HOLD_MS = 2200;
const FADE_MS = 200;

export function ProgressToast({ event }: { event: DownloadEvent | null }) {
  const [visible, setVisible] = useState(false);
  const [closing, setClosing] = useState(false);

  useEffect(() => {
    if (!event) {
      setVisible(false);
      setClosing(false);
      return;
    }
    setVisible(true);
    setClosing(false);
    if (event.type === "finished") {
      const fade = window.setTimeout(() => setClosing(true), HOLD_MS);
      const hide = window.setTimeout(() => setVisible(false), HOLD_MS + FADE_MS);
      return () => {
        window.clearTimeout(fade);
        window.clearTimeout(hide);
      };
    }
    return undefined;
  }, [event]);

  if (!visible || !event) return null;

  const completed = event.completed ?? 0;
  const total = event.total ?? 0;
  const percent = event.type === "finished" ? 100 : total > 0 ? (completed / total) * 100 : 0;

  return (
    <div className={closing ? "toast is-closing" : "toast"} role="status" aria-live="polite">
      <div className="toast-title">
        <span>{event.type === "finished" ? "Download complete" : "Downloading files"}</span>
        <span className="muted">{event.type === "finished" ? "" : `${Math.round(percent)}%`}</span>
      </div>
      <div className={`progress${total === 0 && event.type === "progress" ? " indeterminate" : ""}`}>
        <span style={{ width: event.type === "finished" ? "100%" : `${percent}%` }} />
      </div>
      <div className="toast-sub" style={{ marginTop: 7 }}>
        {event.type === "finished"
          ? `${event.files ?? completed} files · ${bytes(event.bytes ?? 0)}`
          : `${completed}/${total} · ${bytes(event.bytes ?? 0)} · ${event.current ?? ""}`}
      </div>
    </div>
  );
}
