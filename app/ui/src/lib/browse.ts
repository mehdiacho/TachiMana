// Pure helpers for Browse (behaviour from ui/browse/BrowseScreen.kt).
import type { CatalogEntryDto } from "../gen/CatalogEntryDto";
import type { OnlineMangaDto } from "../gen/OnlineMangaDto";
import type { SourceDto } from "../gen/SourceDto";

/** The repo users add most often. Offered as a one-tap chip, never added on its own. */
export const KEIYOUSHI_REPO = "https://raw.githubusercontent.com/keiyoushi/extensions/repo";

const LANG_NAMES: Record<string, string> = {
  all: "Multi",
  en: "English",
  ja: "Japanese",
  ko: "Korean",
  zh: "Chinese",
  "zh-hans": "Chinese (Simplified)",
  "zh-hant": "Chinese (Traditional)",
  es: "Spanish",
  "es-419": "Spanish (Latin America)",
  fr: "French",
  de: "German",
  it: "Italian",
  pt: "Portuguese",
  "pt-br": "Portuguese (Brazil)",
  ru: "Russian",
  id: "Indonesian",
  vi: "Vietnamese",
  th: "Thai",
  tr: "Turkish",
  ar: "Arabic",
  pl: "Polish",
  uk: "Ukrainian",
};

export function langName(code: string): string {
  return LANG_NAMES[code.toLowerCase()] ?? code.toUpperCase();
}

export interface Group<T> {
  key: string;
  title: string;
  items: T[];
}

/** Language order: English first, then Multi, then the rest by name. */
function langRank(code: string): [number, string] {
  const c = code.toLowerCase();
  if (c === "en") return [0, ""];
  if (c === "all") return [1, ""];
  return [2, langName(c)];
}

function byLang(a: string, b: string): number {
  const [ra, na] = langRank(a);
  const [rb, nb] = langRank(b);
  return ra - rb || na.localeCompare(nb);
}

/** Installed sources filtered by `query` (name or language) and grouped by language. */
export function groupSources(sources: SourceDto[], query: string): Group<SourceDto>[] {
  const q = query.trim().toLowerCase();
  const hit = sources.filter(
    (s) => !q || s.name.toLowerCase().includes(q) || langName(s.lang).toLowerCase().includes(q) || s.lang.toLowerCase() === q,
  );
  const by = new Map<string, SourceDto[]>();
  for (const s of hit) {
    const k = s.lang.toLowerCase();
    by.set(k, [...(by.get(k) ?? []), s]);
  }
  return [...by.keys()]
    .sort(byLang)
    .map((k) => ({ key: k, title: langName(k), items: (by.get(k) ?? []).sort((a, b) => a.name.localeCompare(b.name)) }));
}

/** Sources to list: all of them, or without the NSFW ones, and how many that leaves out. */
export function visibleSources(sources: SourceDto[], showNsfw: boolean): { shown: SourceDto[]; hidden: number } {
  if (showNsfw) return { shown: sources, hidden: 0 };
  const shown = sources.filter((s) => !s.nsfw);
  return { shown, hidden: sources.length - shown.length };
}

/**
 * The NSFW item of a source's menu. `set` is the person's own mark to store; null clears it, which
 * is what going back to the extension's flag means.
 */
export function nsfwToggle(src: Pick<SourceDto, "nsfw" | "nsfwByExtension">): { label: string; set: boolean | null } {
  const next = !src.nsfw;
  return { label: src.nsfw ? "Mark as not NSFW" : "Mark as NSFW", set: next === src.nsfwByExtension ? null : next };
}

export interface CatalogFilter {
  query: string;
  showNsfw: boolean;
  lang: string | null;
}

/** Catalog rows grouped as UPDATE PENDING / INSTALLED / AVAILABLE (spec §2.23). */
export function groupCatalog(entries: CatalogEntryDto[], f: CatalogFilter): Group<CatalogEntryDto>[] {
  const q = f.query.trim().toLowerCase();
  const seen = new Set<string>();
  const hit: CatalogEntryDto[] = [];
  for (const e of entries) {
    // The same package can come from two repos; the first one listed wins.
    if (seen.has(e.pkgName)) continue;
    if (!f.showNsfw && e.nsfw && !e.installedVersion) continue;
    if (f.lang && e.lang.toLowerCase() !== f.lang && !e.installedVersion) continue;
    if (q && !e.name.toLowerCase().includes(q) && !e.pkgName.toLowerCase().includes(q)) continue;
    seen.add(e.pkgName);
    hit.push(e);
  }
  const updates = hit.filter((e) => e.updateAvailable);
  const installed = hit.filter((e) => e.installedVersion && !e.updateAvailable);
  const available = hit.filter((e) => !e.installedVersion);
  return [
    { key: "updates", title: "Update pending", items: updates },
    { key: "installed", title: "Installed", items: installed },
    { key: "available", title: "Available", items: available },
  ].filter((g) => g.items.length > 0);
}

/** Languages offered by a catalog, for the language filter. */
export function catalogLangs(entries: CatalogEntryDto[]): string[] {
  return [...new Set(entries.map((e) => e.lang.toLowerCase()))].sort(byLang);
}

/** Appends a page of results, dropping titles already listed (sources repeat across pages). */
export function appendPage(have: OnlineMangaDto[], next: OnlineMangaDto[]): OnlineMangaDto[] {
  const urls = new Set(have.map((m) => m.url));
  const out = [...have];
  for (const m of next) {
    if (urls.has(m.url)) continue;
    urls.add(m.url);
    out.push(m);
  }
  return out;
}

/** Two-letter mark for a source without an icon (spec §2.17 "2-letter mono"). */
export function initials(name: string): string {
  const words = name.trim().split(/[\s._-]+/).filter(Boolean);
  const first = words[0] ?? "";
  const second = words[1] ?? "";
  const two = second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2);
  return two.toUpperCase() || "?";
}
