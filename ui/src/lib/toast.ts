/**
 * Toast store.
 *
 * A four-line external store rather than a context provider: an event handler,
 * a promise rejection or a plugin bridge can raise a toast without being inside
 * the React tree, and `useSyncExternalStore` gives the renderer a stable
 * snapshot without subscribing the whole app to the notification queue.
 *
 * Rules the store enforces:
 *   - at most `MAX_VISIBLE` toasts; the oldest is retired first
 *   - `duration: 0` pins a toast until it is dismissed (used for hard errors)
 *   - `key` collapses repeats: a second "Sync failed" replaces the first
 *   - timers pause while the pointer is over the stack, so a long message is
 *     never yanked out from under someone reading it
 */

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastInput {
  title: string;
  description?: string;
  kind?: ToastKind;
  /** Milliseconds on screen. `0` keeps it up until dismissed. */
  duration?: number;
  action?: ToastAction;
  /** Repeated keys replace the toast already on screen. */
  key?: string;
}

export interface ToastEntry {
  id: number;
  title: string;
  description?: string;
  kind: ToastKind;
  duration: number;
  action?: ToastAction;
  key?: string;
  /** True for the exit frame; the node is removed ~200 ms later. */
  leaving: boolean;
}

const MAX_VISIBLE = 4;
const DEFAULT_MS = 5200;
const EXIT_MS = 200;

let nextId = 1;
let entries: ToastEntry[] = [];
const listeners = new Set<() => void>();
const timers = new Map<number, number>();
/** Milliseconds left and the epoch they started counting from. */
const clocks = new Map<number, { left: number; since: number }>();

function commit(next: ToastEntry[]) {
  entries = next;
  for (const listener of listeners) listener();
}

function clearTimer(id: number) {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
}

function arm(id: number, duration: number) {
  clearTimer(id);
  if (duration <= 0) return;
  clocks.set(id, { left: duration, since: Date.now() });
  timers.set(
    id,
    window.setTimeout(() => dismiss(id), duration),
  );
}

function drop(id: number) {
  clearTimer(id);
  clocks.delete(id);
  commit(entries.filter((entry) => entry.id !== id));
}

export function dismiss(id: number) {
  const entry = entries.find((candidate) => candidate.id === id);
  if (!entry || entry.leaving) return;
  clearTimer(id);
  clocks.delete(id);
  commit(entries.map((candidate) =>
    candidate.id === id ? { ...candidate, leaving: true } : candidate,
  ));
  // The exit frame is `toast-out` in app.css. Removing the node later keeps
  // the animation from being cut off by an unmount.
  window.setTimeout(() => drop(id), EXIT_MS);
}

export function dismissAll() {
  for (const entry of entries) dismiss(entry.id);
}

/** Freeze a toast's countdown while the pointer sits on the stack. */
export function pause(id: number) {
  const clock = clocks.get(id);
  if (!clock) return;
  clearTimer(id);
  const left = Math.max(400, clock.left - (Date.now() - clock.since));
  clocks.set(id, { left, since: Date.now() });
}

/** Resume a paused toast with whatever time was left. */
export function resume(id: number) {
  const clock = clocks.get(id);
  if (!clock) return;
  if (!timers.has(id)) arm(id, clock.left);
}

export function push(input: ToastInput): number {
  const id = nextId++;
  const entry: ToastEntry = {
    id,
    title: input.title,
    description: input.description,
    kind: input.kind ?? "info",
    duration: input.duration ?? (input.kind === "error" ? 8000 : DEFAULT_MS),
    action: input.action,
    key: input.key,
    leaving: false,
  };

  // Identical messages collapse into one: a boot where three calls fail for the
  // same reason should read as one problem, not three. The surviving toast
  // restarts its countdown, which is the useful signal anyway.
  const signature = (candidate: ToastEntry) =>
    `${candidate.kind}\u0000${candidate.title}\u0000${candidate.description ?? ""}`;
  const duplicate = entries.find(
    (candidate) => !candidate.leaving && signature(candidate) === signature(entry),
  );
  if (duplicate) {
    arm(duplicate.id, entry.duration);
    // `leaving` is reset in case the duplicate was mid-exit.
    commit(
      entries.map((candidate) =>
        candidate.id === duplicate.id ? { ...candidate, leaving: false } : candidate,
      ),
    );
    return duplicate.id;
  }

  const kept = input.key
    ? entries.filter((candidate) => candidate.key !== input.key)
    : entries;
  const capped = kept.length >= MAX_VISIBLE ? kept.slice(kept.length - MAX_VISIBLE + 1) : kept;

  commit([...capped, entry].map((candidate) => ({ ...candidate })));
  arm(id, entry.duration);
  return id;
}

interface Api {
  (input: ToastInput): number;
  push: (input: ToastInput) => number;
  success: (title: string, description?: string) => number;
  info: (title: string, description?: string) => number;
  warning: (title: string, description?: string) => number;
  error: (title: string, description?: string) => number;
  dismiss: (id: number) => void;
  dismissAll: () => void;
}

/**
 * `toast("Saved")` and `toast.error("…")` both work: the callable form is what
 * most call sites want, the name-spaced methods are what reads best in a catch.
 */
export const toast: Api = Object.assign(
  (input: ToastInput) => push(input),
  {
    push,
    success: (title: string, description?: string) =>
      push({ kind: "success", title, description }),
    info: (title: string, description?: string) => push({ kind: "info", title, description }),
    warning: (title: string, description?: string) =>
      push({ kind: "warning", title, description }),
    error: (title: string, description?: string) =>
      push({ kind: "error", title, description }),
    dismiss,
    dismissAll,
  },
);

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function snapshot(): ToastEntry[] {
  return entries;
}

/** A single-line summary for a thrown value; `String(err)` keeps the command. */
export function reason(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  return String(cause);
}
