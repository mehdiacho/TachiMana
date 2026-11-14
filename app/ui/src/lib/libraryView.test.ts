// Cases from desktop/src/main/kotlin/tachimana/library/CategorySelfTest.kt.
import { describe, expect, it } from "vitest";
import type { LibraryEntryDto } from "../gen/LibraryEntryDto";
import { applyLibraryView, categoryCounts, genreCounts, DEFAULT_VIEW, librarySections, nextSort, noMatchesLine, UNCATEGORIZED } from "./libraryView";

const READING = 1;
const ON_HOLD = 2;

function entry(
  id: number,
  title: string,
  unread: number,
  categoryIds: number[],
  extra: { author?: string; dateAdded?: number; downloaded?: boolean; genres?: string[]; lastRead?: number; latestUpload?: number } = {},
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
      genres: extra.genres ?? [],
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
    latestUpload: extra.latestUpload ?? 0,
    lastRead: extra.lastRead ?? 0,
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

describe("genres and the newer sorts", () => {
  const more = [
    entry(1, "Alpha", 0, [], { genres: ["Action", "Comedy"], lastRead: 50, latestUpload: 900 }),
    entry(2, "Beta", 0, [], { genres: ["action ", "Drama"], lastRead: 70, latestUpload: 100 }),
    entry(3, "Gamma", 0, [], { genres: ["Comedy", "Comedy"], latestUpload: 500 }),
  ];
  const t = (v: Partial<typeof DEFAULT_VIEW>) => applyLibraryView(more, { ...DEFAULT_VIEW, ...v }).map((e) => e.manga.title);
  it("filters by every picked genre, ignoring case and spaces", () => {
    expect(t({ genres: ["ACTION"] })).toEqual(["Alpha", "Beta"]);
    expect(t({ genres: ["action", "comedy"] })).toEqual(["Alpha"]);
    expect(t({ genres: ["Horror"] })).toEqual([]);
    expect(noMatchesLine({ ...DEFAULT_VIEW, genres: ["Horror"] })).toBe("Nothing here is Horror.");
  });
  it("counts each genre once per manga, most common first", () => {
    expect(genreCounts(more)).toEqual([
      { name: "Action", count: 2 },
      { name: "Comedy", count: 2 },
      { name: "Drama", count: 1 },
    ]);
  });
  it("sorts by last read and by latest chapter", () => {
    expect(t({ sort: "LAST_READ", descending: true })).toEqual(["Beta", "Alpha", "Gamma"]);
    expect(t({ sort: "LATEST_CHAPTER", descending: true })).toEqual(["Alpha", "Gamma", "Beta"]);
  });
});
