// App-wide feedback for work the user started: a count of running actions (drives the thin
// activity bar under the title bar) and short toasts for results that have no place on screen.
import { create } from "zustand";

export type ToastKind = "ok" | "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

interface ActivityState {
  running: number;
  toasts: Toast[];
  begin: () => void;
  end: () => void;
  push: (kind: ToastKind, text: string) => void;
  dismiss: (id: number) => void;
}

/** At most this many toasts at once; older ones go first. */
export const MAX_TOASTS = 4;
const TOAST_MS = { ok: 3500, info: 4500, error: 7000 } as const;
let nextId = 1;

export const useActivity = create<ActivityState>((set, get) => ({
  running: 0,
  toasts: [],
  begin: () => set((s) => ({ running: s.running + 1 })),
  end: () => set((s) => ({ running: Math.max(0, s.running - 1) })),
  push: (kind, text) => {
    const id = nextId++;
    set((s) => ({ toasts: [...s.toasts, { id, kind, text }].slice(-MAX_TOASTS) }));
    setTimeout(() => get().dismiss(id), TOAST_MS[kind]);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

export const toast = {
  ok: (text: string) => useActivity.getState().push("ok", text),
  info: (text: string) => useActivity.getState().push("info", text),
  error: (text: string) => useActivity.getState().push("error", text),
};
