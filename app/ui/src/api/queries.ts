// TanStack Query hooks over the commands. Mutations invalidate what they change.
import { QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { UiSettings } from "../gen/UiSettings";
import type { UiSettingsPatch } from "../gen/UiSettingsPatch";
import { api } from "./commands";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, retry: 1, refetchOnWindowFocus: false } },
});

export const keys = {
  info: ["info"] as const,
  library: ["library"] as const,
  categories: ["categories"] as const,
  detail: (id: number) => ["detail", id] as const,
  pages: (chapterId: number) => ["pages", chapterId] as const,
  settings: ["settings"] as const,
  cache: ["cache"] as const,
  recents: ["recents"] as const,
  onDisk: ["downloads", "disk"] as const,
  nas: ["nas", "settings"] as const,
  nasCache: ["nas", "cache"] as const,
  sync: ["sync"] as const,
  anilist: ["anilist"] as const,
  update: ["update"] as const,
  track: (mangaId: number) => ["track", mangaId] as const,
  browseStatus: ["browse", "status"] as const,
  repos: ["browse", "repos"] as const,
  sources: ["browse", "sources"] as const,
  catalog: ["browse", "catalog"] as const,
  prefs: (sourceId: string) => ["browse", "prefs", sourceId] as const,
};

export const useUpdate = () => useQuery({ queryKey: keys.update, queryFn: api.updateStatus, staleTime: Infinity });
export const useAniList = () => useQuery({ queryKey: keys.anilist, queryFn: api.anilistAccount, staleTime: Infinity });
export const useTrack = (mangaId: number) => useQuery({ queryKey: keys.track(mangaId), queryFn: () => api.trackGet(mangaId) });
export const useSyncStatus = () => useQuery({ queryKey: keys.sync, queryFn: api.syncStatus, staleTime: Infinity });
export const useAppInfo = () => useQuery({ queryKey: keys.info, queryFn: api.appInfo });
export const useLibrary = () => useQuery({ queryKey: keys.library, queryFn: api.library });
export const useCategories = () => useQuery({ queryKey: keys.categories, queryFn: api.categories });
export const useMangaDetail = (id: number | null) =>
  useQuery({ queryKey: keys.detail(id ?? -1), queryFn: () => api.mangaDetail(id ?? -1), enabled: id !== null });
export const useSettings = () =>
  useQuery({ queryKey: keys.settings, queryFn: api.settingsGet, staleTime: Infinity });
export const useBrowseStatus = () => useQuery({ queryKey: keys.browseStatus, queryFn: api.browseStatus, staleTime: Infinity });
export const useRepos = () => useQuery({ queryKey: keys.repos, queryFn: api.extRepos });
/** Starting the extension host can take a few seconds the first time; no automatic retry. */
export const useSources = (enabled = true) =>
  useQuery({ queryKey: keys.sources, queryFn: api.browseSources, enabled, retry: false, staleTime: 5 * 60_000 });
export const useCatalog = (enabled = true) =>
  useQuery({ queryKey: keys.catalog, queryFn: api.extCatalog, enabled, retry: false, staleTime: 10 * 60_000 });
export const useSourcePrefs = (sourceId: string | null) =>
  useQuery({ queryKey: keys.prefs(sourceId ?? ""), queryFn: () => api.sourcePreferences(sourceId ?? ""), enabled: sourceId !== null, retry: false });

/** Invalidate what an install, uninstall or repo change affects. */
export function useBrowseInvalidate() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: keys.repos });
    void qc.invalidateQueries({ queryKey: keys.sources });
    void qc.invalidateQueries({ queryKey: keys.catalog });
  };
}

export const useRecents = () => useQuery({ queryKey: keys.recents, queryFn: api.recents, staleTime: 0 });
export const useOnDisk = () => useQuery({ queryKey: keys.onDisk, queryFn: api.downloadsOnDisk, staleTime: 0 });

export const useNasSettings = () => useQuery({ queryKey: keys.nas, queryFn: api.nasSettingsGet, staleTime: Infinity });
export const useNasCache = () => useQuery({ queryKey: keys.nasCache, queryFn: api.nasCacheUsage });

export const useCacheUsage = () => useQuery({ queryKey: keys.cache, queryFn: api.cacheUsage });

/** Writes a settings patch, updating the cached settings optimistically. */
export function useSetSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: UiSettingsPatch) => api.settingsSet(patch),
    onMutate: (patch) => {
      const prev = qc.getQueryData<UiSettings>(keys.settings);
      if (prev) {
        const next = { ...prev } as Record<string, unknown>;
        for (const [k, v] of Object.entries(patch)) if (v !== undefined) next[k] = v;
        qc.setQueryData(keys.settings, next as UiSettings);
      }
      return { prev };
    },
    onError: (_e, _p, ctx) => {
      if (ctx?.prev) qc.setQueryData(keys.settings, ctx.prev);
    },
    onSuccess: (s) => qc.setQueryData(keys.settings, s),
  });
}

/** Invalidate everything library-shaped after a write. */
export function useLibraryInvalidate() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: keys.library });
    void qc.invalidateQueries({ queryKey: keys.categories });
    void qc.invalidateQueries({ queryKey: ["detail"] });
  };
}
