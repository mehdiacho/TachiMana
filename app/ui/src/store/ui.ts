// Local UI state (Zustand). Server state lives in TanStack Query.
import { create } from "zustand";
import { isBoolean, readPref, writePref } from "../lib/prefs";

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
  /** The library's genre filter; the manga page's genre pills set it too. */
  libraryGenres: string[];
  setLibraryGenres: (g: string[]) => void;
  /** Open chapters in the library's manga pane, with the list still beside them (off by default). */
  besideLibrary: boolean;
  setBesideLibrary: (on: boolean) => void;
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
  libraryGenres: [],
  setLibraryGenres: (libraryGenres) => set({ libraryGenres }),
  besideLibrary: readPref("reader.besideLibrary", false, isBoolean),
  setBesideLibrary: (besideLibrary) => {
    writePref("reader.besideLibrary", besideLibrary);
    set({ besideLibrary });
  },
}));

/** Whether the reader is shown inside the library's manga pane rather than over everything. */
export function readsInPane(u: Pick<UiState, "besideLibrary" | "screen">, compact: boolean): boolean {
  return u.besideLibrary && u.screen === "library" && !compact;
}
