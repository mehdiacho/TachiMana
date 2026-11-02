// The download queue, fed by the `downloads` event from Rust (port of DownloadManager.jobs).
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api } from "../api/commands";
import { keys, queryClient } from "../api/queries";
import type { DownloadJobDto } from "../gen/DownloadJobDto";
import { newlyDone } from "../lib/downloads";

interface DownloadsState {
  jobs: DownloadJobDto[];
  /** Subscribes once and loads the current queue. Safe to call from every screen. */
  init: () => void;
}

let started = false;

export const useDownloads = create<DownloadsState>((set, get) => {
  const apply = (next: DownloadJobDto[]) => {
    const finished = newlyDone(get().jobs, next);
    set({ jobs: next });
    if (finished) {
      // Downloaded flags live on chapters and library entries.
      void queryClient.invalidateQueries({ queryKey: ["detail"] });
      void queryClient.invalidateQueries({ queryKey: keys.library });
      void queryClient.invalidateQueries({ queryKey: keys.onDisk });
    }
  };
  return {
    jobs: [],
    init: () => {
      if (started) return;
      started = true;
      void listen<DownloadJobDto[]>("downloads", (e) => apply(e.payload)).catch(() => {
        started = false;
      });
      void api
        .downloadsList()
        .then(apply)
        .catch(() => undefined);
    },
  };
});
