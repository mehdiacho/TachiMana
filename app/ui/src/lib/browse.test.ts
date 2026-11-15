import { describe, expect, it } from "vitest";
import type { CatalogEntryDto } from "../gen/CatalogEntryDto";
import type { SourceDto } from "../gen/SourceDto";
import { appendPage, catalogLangs, groupCatalog, groupSources, initials, langName, nsfwToggle, visibleSources } from "./browse";

const src = (name: string, lang: string): SourceDto => ({
  id: `${name}-${lang}`,
  name,
  lang,
  extensionPkg: "p",
  extensionName: name,
  iconUrl: null,
  supportsLatest: true,
  isConfigurable: false,
  nsfw: false,
  nsfwByExtension: false,
});

const ext = (name: string, extra: Partial<CatalogEntryDto> = {}): CatalogEntryDto => ({
  repoUrl: "r",
  repoName: "R",
  pkgName: `pkg.${name.toLowerCase()}`,
  name,
  versionName: "1.0",
  lang: "en",
  nsfw: false,
  iconUrl: "",
  installedVersion: null,
  updateAvailable: false,
  ...extra,
});

describe("groupSources", () => {
  it("puts English first, then Multi, then the rest by name", () => {
    const g = groupSources([src("B", "ja"), src("A", "en"), src("M", "all"), src("C", "fr")], "");
    expect(g.map((x) => x.title)).toEqual(["English", "Multi", "French", "Japanese"]);
  });
  it("filters by name or language", () => {
    expect(groupSources([src("MangaDex", "en"), src("Comick", "en")], "dex").flatMap((x) => x.items.map((s) => s.name))).toEqual(["MangaDex"]);
    expect(groupSources([src("A", "ja"), src("B", "en")], "japanese").flatMap((x) => x.items.map((s) => s.name))).toEqual(["A"]);
  });
});

describe("NSFW sources", () => {
  const dex = { ...src("MangaDex", "all"), nsfw: true, nsfwByExtension: true };
  const safe = src("Safe", "en");
  it("hides NSFW sources only when the switch is off, and counts them", () => {
    expect(visibleSources([dex, safe], true)).toEqual({ shown: [dex, safe], hidden: 0 });
    expect(visibleSources([dex, safe], false)).toEqual({ shown: [safe], hidden: 1 });
    expect(visibleSources([], false)).toEqual({ shown: [], hidden: 0 });
  });
  it("offers the opposite mark, and clears the person's mark when that is what the extension says", () => {
    expect(nsfwToggle(dex)).toEqual({ label: "Mark as not NSFW", set: false });
    expect(nsfwToggle({ nsfw: false, nsfwByExtension: true })).toEqual({ label: "Mark as NSFW", set: null });
    expect(nsfwToggle(safe)).toEqual({ label: "Mark as NSFW", set: true });
    expect(nsfwToggle({ nsfw: true, nsfwByExtension: false })).toEqual({ label: "Mark as not NSFW", set: null });
  });
});

describe("groupCatalog", () => {
  const all = [
    ext("Zeta"),
    ext("Alpha", { installedVersion: "1.0" }),
    ext("Beta", { installedVersion: "0.9", updateAvailable: true }),
    ext("Lewd", { nsfw: true }),
    ext("Jap", { lang: "ja" }),
    ext("Zeta", { repoName: "Other" }),
  ];
  it("groups updates, installed, available and drops duplicate packages", () => {
    const g = groupCatalog(all, { query: "", showNsfw: false, lang: null });
    expect(g.map((x) => [x.key, x.items.map((e) => e.name)])).toEqual([
      ["updates", ["Beta"]],
      ["installed", ["Alpha"]],
      ["available", ["Zeta", "Jap"]],
    ]);
  });
  it("hides NSFW and other languages only among extensions not installed", () => {
    const g = groupCatalog([...all, ext("Mine", { nsfw: true, lang: "ja", installedVersion: "1" })], { query: "", showNsfw: false, lang: "en" });
    const names = g.flatMap((x) => x.items.map((e) => e.name));
    expect(names).toContain("Mine");
    expect(names).not.toContain("Lewd");
    expect(names).not.toContain("Jap");
  });
  it("searches names", () => {
    expect(groupCatalog(all, { query: "alp", showNsfw: true, lang: null }).flatMap((x) => x.items.map((e) => e.name))).toEqual(["Alpha"]);
  });
  it("lists languages English first", () => {
    expect(catalogLangs(all)).toEqual(["en", "ja"]);
  });
});

describe("appendPage", () => {
  it("drops titles already listed", () => {
    const m = (url: string) => ({ url, title: url, thumbnailUrl: null, libraryId: null });
    expect(appendPage([m("a"), m("b")], [m("b"), m("c"), m("c")]).map((x) => x.url)).toEqual(["a", "b", "c"]);
  });
});

describe("labels", () => {
  it("initials and language names", () => {
    expect(initials("MangaDex")).toBe("MA");
    expect(initials("Asura Scans")).toBe("AS");
    expect(initials("")).toBe("?");
    expect(langName("pt-BR")).toBe("Portuguese (Brazil)");
    expect(langName("xx")).toBe("XX");
  });
});

describe("search across all sources", () => {
  it("searches in the order the Sources tab lists", async () => {
    const { searchOrder } = await import("./browse");
    expect(searchOrder([src("Zeta", "fr"), src("Beta", "en"), src("Multi", "all"), src("Alpha", "en")]).map((x) => x.name)).toEqual([
      "Alpha",
      "Beta",
      "Multi",
      "Zeta",
    ]);
  });

  it("runs no more jobs at once than allowed, in order, and frees a place when one fails", async () => {
    const { limiter } = await import("./browse");
    const run = limiter(2);
    let active = 0;
    let peak = 0;
    const started: number[] = [];
    const job = (i: number) => async () => {
      started.push(i);
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((r) => setTimeout(r, 5));
      active -= 1;
      if (i === 1) throw new Error("boom");
      return i;
    };
    const all = await Promise.allSettled([0, 1, 2, 3, 4].map((i) => run(job(i))));
    expect(peak).toBe(2);
    expect(started).toEqual([0, 1, 2, 3, 4]);
    expect(all.map((r) => r.status)).toEqual(["fulfilled", "rejected", "fulfilled", "fulfilled", "fulfilled"]);
  });

  it("keeps to the limit when a job arrives just as another ends", async () => {
    const { limiter } = await import("./browse");
    const run = limiter(1);
    let active = 0;
    let peak = 0;
    const job = async () => {
      active += 1;
      peak = Math.max(peak, active);
      await Promise.resolve();
      active -= 1;
    };
    const first = run(job);
    const second = run(job);
    // Arrives in the same tick the first one ends, before the waiting one has woken.
    const late = first.then(() => run(job));
    await Promise.all([first, second, late]);
    expect(peak).toBe(1);
  });

  it("does not start a job that was cancelled while it waited", async () => {
    const { limiter } = await import("./browse");
    const run = limiter(1);
    const gone = new AbortController();
    let ran = false;
    const first = run(() => new Promise((r) => setTimeout(r, 5)));
    const second = run(async () => {
      ran = true;
    }, gone.signal);
    gone.abort();
    await first;
    await expect(second).rejects.toThrow();
    expect(ran).toBe(false);
    // The place it held is free again.
    await expect(run(async () => 7)).resolves.toBe(7);
  });

  it("treats a nonsense limit as one at a time", async () => {
    const { limiter } = await import("./browse");
    for (const bad of [0, -3, Number.NaN]) await expect(limiter(bad)(async () => 1)).resolves.toBe(1);
  });

  it("says how far the search is, then what it found", async () => {
    const { searchSummary } = await import("./browse");
    expect(searchSummary([])).toBe("No sources to search.");
    expect(searchSummary(["results", "loading", "empty"])).toBe("Searched 2 of 3 sources…");
    expect(searchSummary(["results"])).toBe("Found in 1 of 1 source.");
    expect(searchSummary(["results", "empty", "empty", "failed"])).toBe("Found in 1 of 4 sources, 2 found nothing, 1 could not be searched.");
  });
});
