import { describe, expect, it } from "vitest";
import type { CatalogEntryDto } from "../gen/CatalogEntryDto";
import type { SourceDto } from "../gen/SourceDto";
import { appendPage, catalogLangs, groupCatalog, groupSources, initials, langName } from "./browse";

const src = (name: string, lang: string): SourceDto => ({
  id: `${name}-${lang}`,
  name,
  lang,
  extensionPkg: "p",
  extensionName: name,
  iconUrl: null,
  supportsLatest: true,
  isConfigurable: false,
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
