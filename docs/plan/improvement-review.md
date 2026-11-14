# Improvement review: every screen of the computer app

Status: **first full pass, 2026-11-14, against 0.1.17.** Asked for by Mehdi: "look at every single
aspect of the app … how we can improve things which might turn out to be restrictive".

How it was made: by reading the app's own screens and settings in this repository, and comparing
them with what Mihon on Android offers its user (from Mihon's public behaviour, not from the fork's
code). "Not there" means it was not found in the code of 0.1.17. Nothing here is built yet; this is
the list to rank. It replaces system-plan §7.2 as the working list for step G.

Sizes: **S** under a day, **M** one to three days, **L** a week or more. "Phone too" means the
phone needs a matching change, which goes as an issue on the fork.

## 1. The twelve I would do first

Ranked by how much each one currently holds a person back, then by size.

| # | What | Why it is first | Size |
| --- | --- | --- | --- |
| 1 | **Source filters in Browse** | A source can only be browsed by Popular, Latest and a text search. MangaDex's content rating, genres, status and sort are all filters, so today they cannot be changed at all. | M |
| 2 | **Global search across sources** | Finding a manga means opening each source in turn. Mihon searches every pinned source at once. | M |
| 3 | **Chapter list sort, filter and "download next N"** | The chapter list has one order and no filter. Long series are hard to work with. | S |
| 4 | **Reader settings page, and per-manga reading mode** | Mode, fit and zoom are one global choice. A webtoon and a right-to-left manga need different modes, so the person changes it every time they switch. | M |
| 5 | **Updates screen ("new chapters")** | The library can be updated, but there is no list of what the update found. | M |
| 6 | **Download queue controls** | No pause, reorder, retry-all or "how many at once". Also no "download new chapters automatically" and no "delete after reading". | M |
| 7 | **Automatic backups** | A backup is only made when you press "Export backup" on the Sync screen. Mihon makes one on a schedule and keeps the last few. | S |
| 8 | **Remember the window** | Size, position and maximised state are not kept between runs. | S |
| 9 | **More library filters and sorts, kept per category** | Only "unread" and "downloaded" filters. Missing: started, completed, bookmarked, tracked, by source. | S |
| 10 | **Command palette (Ctrl+K) and a shortcuts sheet (?)** | A computer app is driven by the keyboard. Shortcuts exist but nothing lists them, and there is no "jump to anything". | M |
| 11 | **Migrate a manga to another source** | When a source dies the manga is stuck on it. | M |
| 12 | **Colours: accent colour, pure-black dark, cover-based tint** | Four fixed themes and nothing else to adjust. | S |

## 2. Screen by screen

Each part says what is there, what holds a person back, and what to add.

### 2.1 Window, navigation rail and app-wide behaviour

There now: a rail with Library, Recents, Browse, Downloads, Sync, plus Rotate and Settings.
Ctrl+1..5 switch screens, Ctrl+, opens Settings. An activity area shows running work. An update
banner offers new versions.

Holds you back
- The window forgets its size and place (S).
- Closing the window ends everything: downloads, the library update and sync stop. No tray icon,
  no "keep running in the background" (M).
- No desktop notifications for new chapters, finished downloads or a sync problem (S, after tray).
- No incognito mode (read without history, as on the phone) (S).
- No way to choose which screen opens first, or to hide a rail entry you never use (S).
- No shortcuts sheet and no way to change a shortcut (S for the sheet, M for the key map).
- No command palette (M).
- No back and forward through where you have been (mouse side buttons, Alt+Left) outside the
  reader (S).
- The rail has no badges: unread updates, downloads running, sync out of step (S).

### 2.2 Library

There now: grid and list views, a category bar and editor, search across the whole library, sort
with direction, "unread" and "downloaded" filters, a genre filter, multi-select with a bulk bar
(mark read or unread, download, delete downloads, categories), right-click menu, library update
with progress.

Holds you back
- Filters stop at unread and downloaded. Add started, completed, bookmarked, tracked, and by
  source; each one three-state as on the phone (include, exclude, off) (S).
- Sort and view are one setting for the whole library. Mihon can keep them per category (S,
  phone too if the order is to match).
- No choice of cover size or columns, and no choice of what a cover shows: unread count,
  downloaded count, source language, a "continue" button on hover (S).
- No drag of a manga onto a category tab (S).
- Update is all or nothing. Add "this category only", and the phone's rules: skip completed
  series, skip ones with unread chapters, skip ones never started (S).
- No automatic update on a timer (needs the app to stay running; see tray) (S after tray).
- No "open a random manga" and no item counts on the category tabs (S).
- No reading statistics (titles, chapters read, time, by source and tracker score) (M).

### 2.3 Manga page

There now: cover, details, genres that filter the library, tracking, refresh from source, chapter
list with select, mark read/unread, "mark this and earlier chapters read", download, retry, delete, "Download
unread", "Mark all read",
NAS and bookmark marks, a resizable details pane.

Holds you back
- Chapter list: no sort (by source order, chapter number, upload date), no filter (unread,
  downloaded, bookmarked), no choice between chapter title and number, no scanlator filter (S).
- Download choices stop at "unread". Add next 1, 5, 10, a custom number, and all (S).
- No notes on a manga, no change of cover, no "open in browser", no "copy link" (S each).
- No migrate to another source (M), and no "find on other sources" which is the same search.
- A manga with duplicate chapters from several scanlators shows them all with no way to prefer
  one (S, after the scanlator filter).
- No per-manga reading mode (comes with the reader work).

### 2.4 Reader

There now: Strip, left-to-right, right-to-left and two-page modes; fit choices; zoom with keys
and Ctrl+wheel; full screen (F, F11); reload (F5); previous and next chapter ([ and ]); a page
slider; the next pages loaded ahead in paged modes; a "cover alone" option for two-page mode.

Holds you back
- Every choice is global. Remember mode, fit and zoom per manga, with the global one as the
  default (M, phone already does this; the mode could sync later).
- No reader page in Settings. The choices live only in the reader's own bar (S, with the above).
- Missing display options the phone has: background colour (black, grey, white, follow theme),
  crop white borders, side padding and a gap between pages in Strip mode, split a wide page in
  two, grayscale, invert colours, a brightness and colour filter (S each, crop borders M).
- No double-click to zoom, no drag to pan when zoomed, no "fit width up to N pixels" for wide
  screens (S).
- No auto-scroll for long strips (S).
- No chapter list or page thumbnails inside the reader; changing chapter out of order means
  leaving it (S).
- No clear page between chapters ("finished chapter 12, next is 13, 2 chapters missing"), and no
  "skip chapters already read" or "skip duplicates" when moving on (S).
- Per-page actions are missing: save the page, copy it, set as cover, reload just this page (S).
- Mouse: no choice of what a click on the left or right edge does, no invert, no wheel-turns-page
  in paged modes (S).
- The key map is fixed (M, same work as the app-wide key map).
- A second window or a second monitor for the reader is not possible. D4 (read beside the
  library) is decided but not built (M).
- Keep-awake while reading is not set, so the screen can dim in auto-scroll (S).

### 2.5 Browse: sources

There now: sources grouped by language, the NSFW switch and per-source mark, 18+ badge, source
settings, a source view with Popular, Latest and search, "in library" marks on results.

Holds you back
- **No source filters** (item 1). This is the largest gap in the app.
- No global search (item 2), and no "search this title on other sources" from a result.
- No pinning of sources and no "last used" at the top; no choice of which languages to show
  (S). The NSFW mark showed the pattern: a right-click menu on a source can hold pin and hide.
- Results have no list view, no cover size, and a long press equivalent (right-click: add to
  library, pick categories, open in browser) is thin (S).
- Adding a manga that is already in the library from another source gives no warning (S).
- No "latest across my pinned sources" feed (M).
- No "open in browser" or built-in page for a source that needs a login or a captcha beyond the
  Cloudflare setting (M).

### 2.6 Browse: extensions

There now: extension list with an NSFW chip, repositories dialog, install and update.

Holds you back
- No "update all", no automatic check for extension updates with a badge on the rail (S).
- No language filter kept between runs, no search by source name inside an extension (S).
- No details page for an extension: its sources, version, which ones are switched on (S).
- No word on trust: an extension is run code. Mihon asks before trusting one from an unknown
  signer. The computer should say where an extension came from and ask once (M).

### 2.7 Downloads

There now: the queue with cancel per chapter, downloaded chapters grouped by manga with read and
delete, uploads to the NAS, the phone's queues, where chapters go.

Holds you back
- No pause and resume, no reorder, no "move to top", no cancel-all for one series, no retry-all
  after a failure (S to M).
- No setting for how many downloads run at once, or per source (S).
- No speed, size or time-left line (S).
- No "download new chapters automatically" (all, or chosen categories), no "delete after
  reading" (keep the last N), no "delete when marked read", no "never delete bookmarked" (M).
- No "download ahead while reading" (S, after the above).
- No choice of format (folder of images or a CBZ file) (S).
- No storage view: how much each manga takes, with a clean-up of chapters no longer in the
  library (S).
- The destination picker and "Download all here" from the phone are still to be tried on his
  real phone.

### 2.8 Recents

There now: a list of what was read, newest first. A click resumes the chapter; a second button
goes to the manga.

Holds you back
- No search, no removal of one entry or of one manga's history, no "clear all" (S).
- No grouping by day and no reading time (S).
- This screen and the missing Updates screen belong together, as two tabs or two rail entries
  the way the phone has them (decide with item 5).

### 2.9 Updates (not there)

The library update exists but its result is only a number. Add a screen listing new chapters by
day, with read, download, mark read and select, the time of the last update, and a badge on the
rail (M).

### 2.10 Settings

There now (0.1.17): pages for Profiles, Appearance, Reader, Browse, Tracking, Data and storage,
Sync, About, with a search box.

Holds you back
- The pages are still thin. Reader has little in it, and there is no Library, Downloads,
  Privacy or Advanced page. Each item above that is a preference lands on one of these pages.
- No "reset this page to defaults", and no export or import of settings on their own (S).
- Advanced is thin: the image cache can be cleared, but not cookies or the database of manga not in
  the library; no refresh of all covers, open the log folder or copy debug details (S each).
- Network is missing: a proxy, a custom user agent, DNS over HTTPS (S to M).
- About does not show what changed in the installed version (S).

### 2.11 Colours and customisation

There now: four themes.

Holds you back
- No accent colour, no pure-black dark, no "follow the system" for light and dark, no schedule (S).
- No interface size or density (a zoom for the whole app), which matters on a 4K screen or a TV (S).
- No choice of date format or relative dates ("2 days ago") (S).
- No tint from the cover on the manga page, which is what gives Mihon's manga page its look (S).
- The rail cannot be moved, shrunk to icons only or reordered (S).
- Language of the app itself: English only (L, not proposed now).

### 2.12 Data, backup and tracking

There now: restore a Mihon backup and export one by hand (both on the Sync screen), AniList
tracking, the NAS (WebDAV) with cache size.

Holds you back
- No automatic backups, and an exported backup leaves out settings (S). Until sync is complete
  a backup is the only safety net for the computer's library.
- Backup lives on the Sync screen, not under Settings › Data and storage where a person looks
  for it (S).
- Only AniList. The phone has MyAnimeList, Kitsu, MangaUpdates, Shikimori, Bangumi and the
  self-hosted ones (Komga, Kavita, Suwayomi). MyAnimeList and Kitsu are the two worth adding
  first (M each).
- Tracking has no "update progress after reading" switch and no "ask to track when adding" (S).
- Restores are not written to the change journal, so a restore on one device is not carried to
  the other (known, in STATUS).

### 2.13 Sync

Covered by the system plan (steps D to H). Usability gaps that are not in that plan
- One line that is always visible: "in step with <device>, 2 min ago", or what is wrong (S, phone
  too; proposed in §7.3).
- A log a person can read: what was sent, what came in, what was refused and why (S).
- A "what would change" preview before the first two-way sync (M).

### 2.14 Profiles

There now: profiles with their own libraries.

Holds you back
- No password or PIN on a profile, no "hide this profile's titles in the window title and
  notifications" (S). Worth doing with incognito.

## 3. Things the computer could do that the phone cannot

- Several windows: the library on one screen, the reader on another.
- Drag a CBZ or a folder onto the window to read it as a local manga.
- A real keyboard workflow: palette, key map, type-to-search anywhere.
- Hover previews: the manga's summary and next unread chapter on hover over a cover.
- Bulk work at scale: select hundreds, change categories, migrate many at once.
- A local server page showing download and sync state, for a NAS or a TV.

## 4. Suggested order

1. **Finding and reading**: items 1, 2, 3, 4 (filters, global search, chapter list, reader).
2. **Keeping up**: items 5, 6, 9 (Updates screen, download controls and automation, library
   filters).
3. **Trust in the app**: items 7, 8, then tray and notifications.
4. **Feel**: items 10, 12, then the rest of §2.11.
5. **Bigger pieces**: item 11 (migrate), more trackers, several windows.

Each batch is one release or two. Every item that adds a preference also adds an entry to
`ui/src/lib/settingsIndex.ts`, so the Settings search finds it.

## Revision history

- 2026-11-14: first pass, against 0.1.17.
- 2026-11-14, later: three corrections found while writing the screen catalogue. The computer can
  export a backup by hand, has "mark this and earlier chapters read", and Recents resumes a
  chapter.
