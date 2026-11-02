// Library update progress, fed by the `library-update` event from Rust (port of LibraryUpdater).
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api } from "../api/commands";
import { queryClient } from "../api/queries";
import type { CmdError } from "../gen/CmdError";
import type { LibraryUpdateDto } from "../gen/LibraryUpdateDto";

interface UpdateState {
  progress: LibraryUpdateDto | null;
  /** The last finished run, until the UI has shown it. */
  finished: LibraryUpdateDto | null;
  start: () => Promise<string | null>;
  clearFinished: () => void;
}

let listening = false;

function ensureListening(set: (p: Partial<UpdateState>) => void) {
  if (listening) return;
  listening = true;
  void listen<LibraryUpdateDto>("library-update", (e) => {
    const p = e.payload;
    if (p.running) set({ progress: p });
    else {
      set({ progress: null, finished: p });
      void queryClient.invalidateQueries({ queryKey: ["library"] });
      void queryClient.invalidateQueries({ queryKey: ["detail"] });
    }
    // Each finished title can have new chapters; keep the list current while it runs.
    if (p.done > 0) void queryClient.invalidateQueries({ queryKey: ["library"] });
  }).catch(() => {
    listening = false;
  });
}

export const useLibraryUpdate = create<UpdateState>((set) => ({
  progress: null,
  finished: null,
  start: async () => {
    ensureListening(set);
    try {
      const total = await api.libraryUpdate();
      if (total === 0) return "No online titles in the library to update.";
      set({ progress: { running: true, done: 0, total, current: null, chaptersAdded: 0, failures: [] } });
      return null;
    } catch (e) {
      return (e as CmdError).message;
    }
  },
  clearFinished: () => set({ finished: null }),
}));

/** One line for a finished run. */
export function summary(p: LibraryUpdateDto): string {
  const added = p.chaptersAdded === 0 ? "no new chapters" : `${p.chaptersAdded} new chapter${p.chaptersAdded === 1 ? "" : "s"}`;
  const failed = p.failures.length === 0 ? "" : ` · ${p.failures.length} could not be reached`;
  return `Library updated: ${added}${failed}.`;
}
