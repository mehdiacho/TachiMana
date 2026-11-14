// Cases from desktop/src/main/kotlin/tachimana/library/CategorySelfTest.kt.
import { describe, expect, it } from "vitest";
import type { LibraryEntryDto } from "../gen/LibraryEntryDto";
import { applyLibraryView, categoryCounts, DEFAULT_VIEW, librarySections, nextSort, noMatchesLine, UNCATEGORIZED } from "./libraryView";

const READING = 1;
const ON_HOLD = 2;

function entry(
  id: number,
  title: string,
  unread: number,
  categoryIds: number[],
  extra: { author?: string; dateAdded?: number; downloaded?: boolean } = {},
): LibraryEntryDto {
  return {
    manga: {
      id,
      source: "0",
      url: title,
      title,
      favorite: true,
      author: extra.author ?? null,
      artist: null,
      description: null,
      genres: [],
      status: 0,
      viewer: 0,
      dateAdded: extra.dateAdded ?? 0,
      lastUpdate: 0,
      isLocal: true,
      coverVersion: 0,
    },
    unread,
    categoryIds,
    downloaded: extra.downloaded ?? false,
    latestChapter: null,
  };
}

const entries = [
  entry(1, "Alpha", 0, [READING, ON_HOLD], { dateAdded: 100 }),
  entry(2, "Beta", 5, [READING], { dateAdded: 300, downloaded: true }),
  entry(3, "Gamma", 2, [], { author: "Author of Gamma", dateAdded: 200 }),
];
const titles = (v: Partial<typeof DEFAULT_VIEW>) =>
  applyLibraryView(entries, { ...DEFAULT_VIEW, ...v }).map((e) => e.manga.title);

describe("applyLibraryView", () => {
  it("filters by category", () => {
    expect(titles({ categoryId: ON_HOLD })).toEqual(["Alpha"]);
    expect(titles({ categoryId: UNCATEGORIZED })).toEqual(["Gamma"]);
  });
  it("unread only", () => expect(titles({ unreadOnly: true })).toEqual(["Beta", "Gamma"]));
  it("searches title and author", () => {
    expect(titles({ query: "bet" })).toEqual(["Beta"]);
    expect(titles({ query: "author of gamma" })).toEqual(["Gamma"]);
  });
  it("a search looks through every category, whichever is picked", () => {
    expect(titles({ categoryId: ON_HOLD, query: "bet" })).toEqual(["Beta"]);
    expect(titles({ categoryId: ON_HOLD, query: "  " })).toEqual(["Alpha"]);
    expect(applyLibraryView(entries, { ...DEFAULT_VIEW, categoryId: ON_HOLD, query: "bet" }, true)).toEqual([]);
  });
  it("sorts by unread descending", () =>
    expect(titles({ sort: "UNREAD", descending: true })).toEqual(["Beta", "Gamma", "Alpha"]));
  it("sorts by date added descending", () =>
    expect(titles({ sort: "DATE_ADDED", descending: true })).toEqual(["Beta", "Gamma", "Alpha"]));
  it("defaults to title ascending", () => expect(titles({})).toEqual(["Alpha", "Beta", "Gamma"]));
  it("downloaded only", () => expect(titles({ downloadedOnly: true })).toEqual(["Beta"]));
  it("combines category and unread", () =>
    expect(titles({ categoryId: READING, unreadOnly: true })).toEqual(["Beta"]));
  it("ties fall back to title, and descending reverses the tie order too", () => {
    const tied = [entry(1, "b", 1, []), entry(2, "a", 1, [])];
    const v = { ...DEFAULT_VIEW, sort: "UNREAD" as const };
    expect(applyLibraryView(tied, v).map((e) => e.manga.title)).toEqual(["a", "b"]);
    expect(applyLibraryView(tied, { ...v, descending: true }).map((e) => e.manga.title)).toEqual(["b", "a"]);
  });
});

describe("categoryCounts and nextSort", () => {
  it("counts every chip", () => {
    const c = categoryCounts(entries);
    expect(c.get(null)).toBe(3);
    expect(c.get(READING)).toBe(2);
    expect(c.get(ON_HOLD)).toBe(1);
    expect(c.get(UNCATEGORIZED)).toBe(1);
  });
  it("clicking again flips, a new sort uses its default", () => {
    expect(nextSort(DEFAULT_VIEW, "TITLE")).toEqual({ sort: "TITLE", descending: true });
    expect(nextSort(DEFAULT_VIEW, "UNREAD")).toEqual({ sort: "UNREAD", descending: true });
  });
});

describe("noMatchesLine", () => {
  const v = (p: Partial<typeof DEFAULT_VIEW>) => noMatchesLine({ ...DEFAULT_VIEW, ...p });
  it("says why the view is empty, search first", () => {
    expect(v({ query: " frieren ", unreadOnly: true })).toBe('Nothing matches "frieren".');
    expect(v({ unreadOnly: true, downloadedOnly: true })).toBe("Nothing here is both unread and downloaded.");
    expect(v({ unreadOnly: true })).toBe("Everything here is read.");
    expect(v({ downloadedOnly: true })).toBe("Nothing here is downloaded yet.");
    expect(v({ categoryId: UNCATEGORIZED })).toBe("Every manga is in a category.");
    expect(v({ categoryId: 3 })).toBe("This category is empty.");
  });
});

describe("librarySections", () => {
  const cats = [
    { id: READING, name: "Reading" },
    { id: ON_HOLD, name: "On hold" },
    { id: 9, name: "Empty" },
  ];
  const names = (v: Partial<typeof DEFAULT_VIEW>) =>
    librarySections(entries, cats, { ...DEFAULT_VIEW, ...v }).map((x) => `${x.name}:${x.entries.map((e) => e.manga.title).join(",")}`);
  it("lists each category in order, Uncategorised last, without the empty ones", () =>
    expect(names({})).toEqual(["Reading:Alpha,Beta", "On hold:Alpha", "Uncategorised:Gamma"]));
  it("filters every section and ignores the picked category", () =>
    expect(names({ categoryId: ON_HOLD, unreadOnly: true })).toEqual(["Reading:Beta", "Uncategorised:Gamma"]));
  it("searches inside each section", () => expect(names({ query: "alp" })).toEqual(["Reading:Alpha", "On hold:Alpha"]));
  it("is one section called All when there are no categories", () =>
    expect(librarySections(entries, [], DEFAULT_VIEW).map((x) => x.name)).toEqual(["All"]));
});
