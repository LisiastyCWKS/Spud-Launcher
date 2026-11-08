/**
 * Promise-based confirmation.
 *
 * Same idea as the toast store: a module-level queue so any component — or a
 * plain async function — can ask for a decision without threading a provider
 * through the tree. One request is live at a time; a second call resolves the
 * first with `false` rather than stacking dialogs on top of each other.
 */

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** `danger` paints the confirm button red and focuses Cancel first. */
  tone?: "default" | "danger";
  /** Optional third option, e.g. "Open folder". Resolves `true` like confirm. */
  extraLabel?: string;
}

export interface ConfirmRequest extends ConfirmOptions {
  id: number;
  resolve: (value: boolean) => void;
}

let nextId = 1;
let request: ConfirmRequest | null = null;
const listeners = new Set<() => void>();

function commit(next: ConfirmRequest | null) {
  request = next;
  for (const listener of listeners) listener();
}

export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  if (request) settle(false);
  return new Promise<boolean>((resolve) => {
    commit({ id: nextId++, tone: "default", ...options, resolve });
  });
}

/** Resolve the live request and unmount the dialog. */
export function settle(value: boolean) {
  if (!request) return;
  const current = request;
  commit(null);
  current.resolve(value);
}

export function subscribeConfirm(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function confirmSnapshot(): ConfirmRequest | null {
  return request;
}
