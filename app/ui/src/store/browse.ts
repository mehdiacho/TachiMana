// Browse navigation, kept outside the screen so switching to Library and back keeps the place.
import { create } from "zustand";
import type { BrowseMode } from "../api/commands";

export type BrowseTab = "sources" | "extensions";

export interface SourceView {
  sourceId: string;
  name: string;
  supportsLatest: boolean;
  mode: BrowseMode;
  query: string;
}

interface BrowseState {
  tab: BrowseTab;
  source: SourceView | null;
  /** Library row of the manga open in the detail pane. */
  mangaId: number | null;
  setTab: (t: BrowseTab) => void;
  openSource: (s: SourceView) => void;
  setMode: (mode: BrowseMode, query?: string) => void;
  closeSource: () => void;
  openManga: (id: number | null) => void;
}

export const useBrowse = create<BrowseState>((set) => ({
  tab: "sources",
  source: null,
  mangaId: null,
  setTab: (tab) => set({ tab }),
  openSource: (source) => set({ source, mangaId: null }),
  setMode: (mode, query) =>
    set((st) => (st.source ? { source: { ...st.source, mode, query: query ?? st.source.query } } : {})),
  closeSource: () => set({ source: null, mangaId: null }),
  openManga: (mangaId) => set({ mangaId }),
}));
