// Port of desktop/src/main/kotlin/tachimana/library/LibraryView.kt `applyLibraryView`.
import type { LibraryEntryDto } from "../gen/LibraryEntryDto";

export type LibrarySort = "TITLE" | "UNREAD" | "DATE_ADDED" | "LAST_UPDATED";

export const SORTS: { id: LibrarySort; label: string; defaultDescending: boolean }[] = [
  { id: "TITLE", label: "Title", defaultDescending: false },
  { id: "UNREAD", label: "Unread", defaultDescending: true },
  { id: "DATE_ADDED", label: "Recently added", defaultDescending: true },
  { id: "LAST_UPDATED", label: "Last updated", defaultDescending: true },
];

/** Pseudo-category for manga in no category. Real ids are positive rowids. */
export const UNCATEGORIZED = -1;

export interface LibraryView {
  /** null = All. */
  categoryId: number | null;
  sort: LibrarySort;
  descending: boolean;
  unreadOnly: boolean;
  downloadedOnly: boolean;
  query: string;
}

export const DEFAULT_VIEW: LibraryView = {
  categoryId: null,
  sort: "TITLE",
  descending: false,
  unreadOnly: false,
  downloadedOnly: false,
  query: "",
};

export function isSort(s: string): s is LibrarySort {
  return SORTS.some((x) => x.id === s);
}

function inCategory(e: LibraryEntryDto, id: number | null): boolean {
  if (id === null) return true;
  if (id === UNCATEGORIZED) return e.categoryIds.length === 0;
  return e.categoryIds.includes(id);
}

function sortKey(e: LibraryEntryDto, sort: LibrarySort): number {
  switch (sort) {
    case "UNREAD":
      return e.unread;
    case "DATE_ADDED":
      return e.manga.dateAdded;
    case "LAST_UPDATED":
      return e.manga.lastUpdate;
    case "TITLE":
      return 0;
  }
}

// Kotlin compares `title.lowercase()` with String.compareTo (UTF-16 code units).
function cmpTitle(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  return x < y ? -1 : x > y ? 1 : 0;
}

/**
 * Category, search, filters, then sort. `descending` reverses the whole order, ties included.
 * A search looks through the whole library whatever category is picked (unlike the Kotlin app),
 * so finding a manga never means trying each category in turn; `withinCategory` keeps the
 * category, for the grouped view where each section is searched on its own.
 */
export function applyLibraryView(entries: LibraryEntryDto[], view: LibraryView, withinCategory = false): LibraryEntryDto[] {
  const q = view.query.trim().toLowerCase();
  const category = q === "" || withinCategory ? view.categoryId : null;
  const out = entries.filter(
    (e) =>
      inCategory(e, category) &&
      (q === "" ||
        e.manga.title.toLowerCase().includes(q) ||
        (e.manga.author?.toLowerCase().includes(q) ?? false)) &&
      (!view.unreadOnly || e.unread > 0) &&
      (!view.downloadedOnly || e.downloaded),
  );
  const cmp = (a: LibraryEntryDto, b: LibraryEntryDto) =>
    sortKey(a, view.sort) - sortKey(b, view.sort) || cmpTitle(a.manga.title, b.manga.title);
  out.sort(view.descending ? (a, b) => cmp(b, a) : cmp);
  return out;
}

export interface LibrarySection {
  /** A category id, or [`UNCATEGORIZED`]. */
  id: number;
  name: string;
  entries: LibraryEntryDto[];
}

/**
 * The library as one page of sections, one per category in the user's order and Uncategorised
 * last, each filtered and sorted by `view` (its `categoryId` is ignored). Sections with nothing
 * to show are left out. A manga in two categories is in both sections.
 */
export function librarySections(entries: LibraryEntryDto[], categories: { id: number; name: string }[], view: LibraryView): LibrarySection[] {
  const all = [...categories.map((c) => ({ id: c.id, name: c.name })), { id: UNCATEGORIZED, name: categories.length ? "Uncategorised" : "All" }];
  return all
    .map((c) => ({ ...c, entries: applyLibraryView(entries, { ...view, categoryId: c.id }, true) }))
    .filter((x) => x.entries.length > 0);
}

/** Live chip counts: All, each category, Uncategorised (only when any manga is uncategorised). */
export function categoryCounts(entries: LibraryEntryDto[]): Map<number | null, number> {
  const m = new Map<number | null, number>();
  m.set(null, entries.length);
  for (const e of entries) {
    if (e.categoryIds.length === 0) m.set(UNCATEGORIZED, (m.get(UNCATEGORIZED) ?? 0) + 1);
    for (const c of e.categoryIds) m.set(c, (m.get(c) ?? 0) + 1);
  }
  return m;
}

/** Clicking the active sort flips direction; a new sort starts at its own default direction. */
export function nextSort(view: LibraryView, sort: LibrarySort): Pick<LibraryView, "sort" | "descending"> {
  if (view.sort === sort) return { sort, descending: !view.descending };
  const def = SORTS.find((s) => s.id === sort)?.defaultDescending ?? false;
  return { sort, descending: def };
}

/** Why a non-empty library shows nothing with this view (`NoMatchesHint` in the Kotlin app). */
export function noMatchesLine(view: LibraryView): string {
  const q = view.query.trim();
  if (q) return `Nothing matches "${q}".`;
  if (view.unreadOnly && view.downloadedOnly) return "Nothing here is both unread and downloaded.";
  if (view.unreadOnly) return "Everything here is read.";
  if (view.downloadedOnly) return "Nothing here is downloaded yet.";
  if (view.categoryId === UNCATEGORIZED) return "Every manga is in a category.";
  return "This category is empty.";
}
