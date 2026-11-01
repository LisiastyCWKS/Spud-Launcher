import { classNames } from "../lib/format";

/**
 * The spud mark: a gradient tile with the letter optically centred and a leaf
 * tucked into the top-right corner. The tile is styled from the accent tokens,
 * so the logo, the buttons and the progress bar always agree on one hue.
 */
export function BrandMark({ size = 32, className }: { size?: number; className?: string }) {
  return (
    <span
      className={classNames("brand-mark", className)}
      style={{ width: size, height: size, borderRadius: Math.round(size * 0.31) }}
      aria-hidden="true"
    >
      <span className="brand-mark-sheen" />
      <span className="brand-mark-letter" style={{ fontSize: Math.round(size * 0.58) }}>
        S
      </span>
      <svg className="brand-mark-sprout" viewBox="0 0 14 14" fill="currentColor">
        <path d="M7.6 10.4c-.5-2.9.6-4.7 3.4-5.4.2 3-1.2 4.9-3.4 5.4z" />
        <path d="M6.6 10.6C6.2 7.7 4.6 6 2 5.6c.4 2.9 2 4.6 4.6 5z" opacity=".62" />
      </svg>
    </span>
  );
}
