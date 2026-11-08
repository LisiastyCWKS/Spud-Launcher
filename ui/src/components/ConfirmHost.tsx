import { useEffect, useRef, useSyncExternalStore } from "react";
import { confirmSnapshot, settle, subscribeConfirm } from "../lib/confirm";
import { Icon } from "./Icon";
import { classNames } from "../lib/format";

/**
 * Renders the pending confirmation, if any.
 *
 * Focus lands on Cancel for a destructive action and on Confirm otherwise, the
 * dialog traps Tab inside itself, Escape cancels and the backdrop cancels —
 * the same three escapes a native `confirm()` gives, with a real look.
 */
export function ConfirmHost() {
  const request = useSyncExternalStore(subscribeConfirm, confirmSnapshot, () => null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const safeRef = useRef<HTMLButtonElement | null>(null);
  const goRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!request) return;
    const target = request.tone === "danger" ? safeRef.current : goRef.current ?? safeRef.current;
    target?.focus();

    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        settle(false);
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [request]);

  if (!request) return null;

  const danger = request.tone === "danger";

  return (
    <div
      className="modal-overlay confirm-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) settle(false);
      }}
    >
      <div
        ref={dialogRef}
        className="confirm"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby={request.description ? "confirm-text" : undefined}
        key={request.id}
      >
        <div className={classNames("confirm-mark", danger && "danger")}>
          <Icon name={danger ? "warning" : "info"} size={20} />
        </div>

        <div className="confirm-copy">
          <h2 id="confirm-title" className="confirm-title">
            {request.title}
          </h2>
          {request.description ? (
            <p id="confirm-text" className="confirm-text">
              {request.description}
            </p>
          ) : null}
        </div>

        <div className="confirm-actions">
          <button
            ref={safeRef}
            type="button"
            className="btn"
            onClick={() => settle(false)}
          >
            {request.cancelLabel ?? "Cancel"}
          </button>
          <button
            ref={goRef}
            type="button"
            className={classNames("btn", danger ? "btn-danger" : "btn-primary")}
            onClick={() => settle(true)}
          >
            {request.confirmLabel ?? "Confirm"}
          </button>
        </div>
      </div>
    </div>
  );
}
