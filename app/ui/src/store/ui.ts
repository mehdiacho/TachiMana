// Local UI state (Zustand). Server state lives in TanStack Query.
import { create } from "zustand";

export type Screen = "library" | "recents" | "browse" | "downloads" | "sync" | "settings";

export interface ReaderTarget {
  mangaId: number;
  chapterId: number;
}

interface UiState {
  screen: Screen;
  selectedMangaId: number | null;
  reader: ReaderTarget | null;
  rotated: boolean;
  /** The reader fills the screen: no title bar, no side rail, the window full screen. */
  immersive: boolean;
  setImmersive: (on: boolean) => void;
  setScreen: (s: Screen) => void;
  select: (id: number | null) => void;
  openReader: (t: ReaderTarget) => void;
  closeReader: () => void;
  setRotated: (r: boolean) => void;
}

export const useUi = create<UiState>((set) => ({
  screen: "library",
  selectedMangaId: null,
  reader: null,
  rotated: false,
  immersive: false,
  setImmersive: (immersive) => set({ immersive }),
  setScreen: (screen) => set({ screen }),
  select: (selectedMangaId) => set({ selectedMangaId }),
  openReader: (reader) => set({ reader }),
  closeReader: () => set({ reader: null, immersive: false }),
  setRotated: (rotated) => set({ rotated }),
}));
