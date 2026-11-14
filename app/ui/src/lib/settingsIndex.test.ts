import { describe, expect, it } from "vitest";
import { SETTINGS_ITEMS, SETTINGS_PAGES, searchSettings } from "./settingsIndex";

describe("settings index", () => {
  it("gives every item a page that exists and an id of its own", () => {
    const pages = new Set(SETTINGS_PAGES.map((p) => p.id));
    for (const i of SETTINGS_ITEMS) expect(pages.has(i.page)).toBe(true);
    expect(new Set(SETTINGS_ITEMS.map((i) => i.id)).size).toBe(SETTINGS_ITEMS.length);
    // No page is empty.
    for (const p of pages) expect(SETTINGS_ITEMS.some((i) => i.page === p)).toBe(true);
  });
});

describe("searchSettings", () => {
  const ids = (q: string) => searchSettings(q).map((i) => i.id);
  it("finds nothing for nothing", () => {
    expect(ids("")).toEqual([]);
    expect(ids("   ")).toEqual([]);
    expect(ids("zzzz")).toEqual([]);
  });
  it("finds a setting by its title, by another word for it, and by its page", () => {
    expect(ids("nsfw")[0]).toBe("nsfw");
    expect(ids("adult")).toEqual(["nsfw"]);
    expect(ids("webdav")).toEqual(["nas"]);
    expect(ids("tracking")).toEqual(["anilist"]);
  });
  it("needs every word, in any case", () => {
    expect(ids("PAGE cache")).toEqual(["cache"]);
    expect(ids("cache anilist")).toEqual([]);
  });
  it("puts a title hit before a hit in the other words", () => {
    // "sources" is in the NSFW title and only in the helper's other words.
    const r = ids("sources");
    expect(r.indexOf("nsfw")).toBeLessThan(r.indexOf("helper"));
  });
});
