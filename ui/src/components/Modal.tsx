import { useEffect, useRef, useState } from "react";
import { Icon } from "./Icon";

const EXIT_MS = 180;

export function Modal({
  title,
  subtitle,
  onClose,
  children,
  width,
}: {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: React.ReactNode;
  width?: number;
}) {
  const [closing, setClosing] = useState(false);
  const timer = useRef<number | null>(null);

  // One path out of the dialog: play the exit frame, then let the parent drop
  // it. Every caller keeps calling `onClose` and gets the smooth version free.
  const requestClose = () => {
    if (closing) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      onClose();
      return;
    }
    setClosing(true);
    timer.current = window.setTimeout(onClose, EXIT_MS);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") requestClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
    // `requestClose` closes over the current `closing` flag by design.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onClose, closing]);

  return (
    <div
      className={closing ? "modal-overlay is-closing" : "modal-overlay"}
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) requestClose();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        style={width ? { width: `min(${width}px, 100%)` } : undefined}
      >
        <div className="modal-head">
          <div>
            <h2 className="modal-title">{title}</h2>
            {subtitle ? <div className="modal-sub">{subtitle}</div> : null}
          </div>
          <button type="button" className="icon-btn" onClick={requestClose} aria-label="Close">
            <Icon name="close" />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
