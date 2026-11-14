// The settings pages and what is on each, so Settings can list its pages and search them.
// A row on a page carries the same id, which is how a search result finds it.

export type SettingsPageId = "profiles" | "appearance" | "reader" | "browse" | "tracking" | "storage" | "sync" | "about";

export interface SettingsPage {
  id: SettingsPageId;
  name: string;
  /** One line under the name in the list of pages. */
  about: string;
}

export const SETTINGS_PAGES: readonly SettingsPage[] = [
  { id: "profiles", name: "Profiles", about: "Separate libraries on this computer" },
  { id: "appearance", name: "Appearance", about: "Theme, rotation" },
  { id: "reader", name: "Reader", about: "How chapters open" },
  { id: "browse", name: "Browse", about: "Sources, extensions, NSFW" },
  { id: "tracking", name: "Tracking", about: "AniList" },
  { id: "storage", name: "Data and storage", about: "Folders, page cache, NAS" },
  { id: "sync", name: "Sync", about: "Other devices and backups" },
  { id: "about", name: "About", about: "Version and updates" },
];

export interface SettingsItem {
  id: string;
  page: SettingsPageId;
  title: string;
  /** Other words someone might type to find it. */
  words: string;
}

export const SETTINGS_ITEMS: readonly SettingsItem[] = [
  { id: "profiles", page: "profiles", title: "Profiles", words: "profile library user switch add rename delete" },
  { id: "theme", page: "appearance", title: "Theme", words: "colour color dark light midnight ink vermilion yokai appearance" },
  { id: "rotation", page: "appearance", title: "Portrait rotation", words: "rotate monitor sideways vertical 90 screen orientation" },
  { id: "beside", page: "reader", title: "Read beside the library", words: "reader pane split side by side window" },
  { id: "nsfw", page: "browse", title: "Show NSFW sources", words: "adult 18+ hide hidden sources extensions mature" },
  { id: "cloudflare", page: "browse", title: "Cloudflare bypass", words: "challenge captcha browser window protection source blocked" },
  { id: "helper", page: "browse", title: "Extension helper", words: "extensions install download java host sources" },
  { id: "anilist", page: "tracking", title: "AniList", words: "tracker tracking account sign in connect progress" },
  { id: "data-folder", page: "storage", title: "Data folder", words: "location path files database open" },
  { id: "local-folder", page: "storage", title: "Local manga folder", words: "location path files cbz import open" },
  { id: "cache", page: "storage", title: "Page cache", words: "disk space size clear images storage" },
  { id: "nas", page: "storage", title: "NAS storage", words: "webdav server remote network share upload archive mirror password after a download" },
  { id: "sync", page: "sync", title: "Sync and backups", words: "link phone device pair backup restore export tachibk mihon" },
  { id: "version", page: "about", title: "Version and updates", words: "update check release about licence license" },
];

/**
 * Items matching every word typed, best first: a hit in the title beats one in the other words,
 * and a title that starts with the text beats both.
 */
export function searchSettings(query: string, items: readonly SettingsItem[] = SETTINGS_ITEMS): SettingsItem[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return [];
  const scored: { item: SettingsItem; score: number; at: number }[] = [];
  items.forEach((item, at) => {
    const title = item.title.toLowerCase();
    const page = SETTINGS_PAGES.find((p) => p.id === item.page)?.name.toLowerCase() ?? "";
    const rest = `${item.words.toLowerCase()} ${page}`;
    let score = 0;
    for (const t of terms) {
      if (title.startsWith(t)) score += 3;
      else if (title.includes(t)) score += 2;
      else if (rest.includes(t)) score += 1;
      else return;
    }
    scored.push({ item, score, at });
  });
  return scored.sort((a, b) => b.score - a.score || a.at - b.at).map((x) => x.item);
}
