// The download queue, fed by the `downloads` event from Rust (port of DownloadManager.jobs).
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { api } from "../api/commands";
import { keys, queryClient } from "../api/queries";
import type { DownloadJobDto } from "../gen/DownloadJobDto";
import type { SyncStatusDto } from "../gen/SyncStatusDto";
import type { AniListAccountDto } from "../gen/AniListAccountDto";
import type { UpdateStatusDto } from "../gen/UpdateStatusDto";
import type { UploadJobDto } from "../gen/UploadJobDto";
import { newlyDone } from "../lib/downloads";

interface DownloadsState {
  jobs: DownloadJobDto[];
  /** The queue has been read at least once (so an empty list means empty, not "not loaded"). */
  ready: boolean;
  /** NAS uploads (`uploads` event). */
  uploads: UploadJobDto[];
  /** Subscribes once and loads the current queue. Safe to call from every screen. */
  init: () => void;
}

let started = false;

export const useDownloads = create<DownloadsState>((set, get) => {
  const apply = (next: DownloadJobDto[]) => {
    const finished = newlyDone(get().jobs, next);
    set({ jobs: next, ready: true });
    if (finished) {
      // Downloaded flags live on chapters and library entries.
      void queryClient.invalidateQueries({ queryKey: ["detail"] });
      void queryClient.invalidateQueries({ queryKey: keys.library });
      void queryClient.invalidateQueries({ queryKey: keys.onDisk });
    }
  };
  return {
    jobs: [],
    ready: false,
    uploads: [],
    init: () => {
      if (started) return;
      started = true;
      void listen<UploadJobDto[]>("uploads", (e) => set({ uploads: e.payload })).catch(() => undefined);
      // Sync status (peers, progress) and libraries changed by a pull or restore.
      void listen<SyncStatusDto>("sync", (e) => queryClient.setQueryData(keys.sync, e.payload)).catch(() => undefined);
      void listen<AniListAccountDto>("anilist", (e) => queryClient.setQueryData(keys.anilist, e.payload)).catch(() => undefined);
      void listen<UpdateStatusDto>("update", (e) => queryClient.setQueryData(keys.update, e.payload)).catch(() => undefined);
      void listen<number>("track-changed", (e) => void queryClient.invalidateQueries({ queryKey: keys.track(e.payload) })).catch(() => undefined);
      void listen("library-changed", () => {
        for (const k of [keys.library, keys.categories, keys.recents, ["detail"]]) void queryClient.invalidateQueries({ queryKey: k });
      }).catch(() => undefined);
      // An archived chapter left this computer after its upload.
      void listen("downloads-changed", () => {
        void queryClient.invalidateQueries({ queryKey: ["detail"] });
        void queryClient.invalidateQueries({ queryKey: keys.library });
        void queryClient.invalidateQueries({ queryKey: keys.onDisk });
      }).catch(() => undefined);
      void api
        .nasUploads()
        .then((uploads) => set({ uploads }))
        .catch(() => undefined);
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
