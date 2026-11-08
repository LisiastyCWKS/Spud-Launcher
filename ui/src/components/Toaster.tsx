import { useRef, useSyncExternalStore } from "react";
import {
  dismiss,
  pause,
  resume,
  snapshot,
  subscribe,
  type ToastEntry,
  type ToastKind,
} from "../lib/toast";
import { Icon, type IconName } from "./Icon";
import { classNames } from "../lib/format";

const ICONS: Record<ToastKind, IconName> = {
  success: "check-circle",
  error: "alert",
  warning: "warning",
  info: "info",
};

/**
 * The notification stack.
 *
 * Mounted once, below the app shell, and driven entirely by `lib/toast`. Each
 * entry keeps its own timer in the store, so re-rendering the list — or the
 * whole app — never restarts a countdown. Hovering the stack pauses every
 * countdown in it; leaving resumes with the remaining time.
 */
export function Toaster() {
  const entries = useSyncExternalStore(subscribe, snapshot, () => EMPTY);

  // One listener on the stack instead of one per toast: a pointer that moves
  // between two toasts should not restart either countdown.
  const onEnter = () => {
    for (const entry of entries) pause(entry.id);
  };
  const onLeave = () => {
    for (const entry of entries) resume(entry.id);
  };

  if (entries.length === 0) return null;

  return (
    <div
      className="toaster"
      role="region"
      aria-label="Notifications"
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
    >
      {entries.map((entry) => (
        <ToastCard key={entry.id} entry={entry} />
      ))}
    </div>
  );
}

const EMPTY: ToastEntry[] = [];

function ToastCard({ entry }: { entry: ToastEntry }) {
  const actionRef = useRef<HTMLButtonElement | null>(null);

  const runAction = () => {
    entry.action?.run();
    dismiss(entry.id);
  };

  return (
    <div
      className={classNames("toast", `toast-${entry.kind}`, entry.leaving && "leaving")}
      role={entry.kind === "error" ? "alert" : "status"}
      aria-live={entry.kind === "error" ? "assertive" : "polite"}
    >
      <span className="toast-icon" aria-hidden="true">
        <Icon name={ICONS[entry.kind]} size={17} />
      </span>

      <div className="toast-body">
        <div className="toast-title">{entry.title}</div>
        {entry.description ? <div className="toast-text">{entry.description}</div> : null}
        {entry.action ? (
          <button
            ref={actionRef}
            type="button"
            className="toast-action"
            onClick={runAction}
          >
            {entry.action.label}
          </button>
        ) : null}
      </div>

      <button
        type="button"
        className="toast-close"
        onClick={() => dismiss(entry.id)}
        aria-label="Dismiss notification"
      >
        <Icon name="close" size={14} />
      </button>

      {/* The bar is the countdown, not decoration: `duration` is the same number
          the store schedules, so the two never disagree. */}
      {entry.duration > 0 ? (
        <span
          className="toast-timer"
          style={{ animationDuration: `${entry.duration}ms` }}
          aria-hidden="true"
        />
      ) : null}
    </div>
  );
}
