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
};

export const useAppInfo = () => useQuery({ queryKey: keys.info, queryFn: api.appInfo });
export const useLibrary = () => useQuery({ queryKey: keys.library, queryFn: api.library });
export const useCategories = () => useQuery({ queryKey: keys.categories, queryFn: api.categories });
export const useMangaDetail = (id: number | null) =>
  useQuery({ queryKey: keys.detail(id ?? -1), queryFn: () => api.mangaDetail(id ?? -1), enabled: id !== null });
export const useSettings = () =>
  useQuery({ queryKey: keys.settings, queryFn: api.settingsGet, staleTime: Infinity });
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
