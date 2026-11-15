// Browse navigation, kept outside the screen so switching to Library and back keeps the place.
import { create } from "zustand";
import type { BrowseMode } from "../api/commands";
import type { FilterState } from "../lib/filters";

export type BrowseTab = "sources" | "extensions";

export interface SourceView {
  sourceId: string;
  name: string;
  supportsLatest: boolean;
  mode: BrowseMode;
  query: string;
  /** The person's filter choices for the search; null uses the source's own. */
  filters: FilterState[] | null;
}

interface BrowseState {
  tab: BrowseTab;
  source: SourceView | null;
  /** Library row of the manga open in the detail pane. */
  mangaId: number | null;
  setTab: (t: BrowseTab) => void;
  openSource: (s: SourceView) => void;
  /** Popular and Latest drop the filter choices; a search keeps them unless given new ones. */
  setMode: (mode: BrowseMode, query?: string, filters?: FilterState[] | null) => void;
  closeSource: () => void;
  openManga: (id: number | null) => void;
}

export const useBrowse = create<BrowseState>((set) => ({
  tab: "sources",
  source: null,
  mangaId: null,
  setTab: (tab) => set({ tab }),
  openSource: (source) => set({ source, mangaId: null }),
  setMode: (mode, query, filters) =>
    set((st) =>
      st.source
        ? {
            source: {
              ...st.source,
              mode,
              query: query ?? st.source.query,
              filters: mode === "search" ? (filters === undefined ? st.source.filters : filters) : null,
            },
          }
        : {},
    ),
  closeSource: () => set({ source: null, mangaId: null }),
  openManga: (mangaId) => set({ mangaId }),
}));
