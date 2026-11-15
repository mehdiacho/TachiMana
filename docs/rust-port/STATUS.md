# Rust port status
Updated: 2026-11-14 by Claude (settings pages with search, NSFW sources switch and per-source mark)
Current phase: P0–P9 merged; 0.1.0 released (mehdiacho/tachimana-releases). 0.1.2 released (restore progress). Now: UI states everywhere and an in-place updater (feat/ui-states), then 0.1.3. The Kotlin app stays until Mehdi has tried the Windows installer

Done:
- P0 baseline (PR #3): `docs/rust-port/baseline.md` (Linux numbers, self-test run) and scripts in
  `docs/rust-port/baseline/`. The Windows column and `browseSelfTest`/`extensionSpike` on an open
  network are still Mehdi's.
- P2 `app/` workspace (no unsafe, no unwrap/expect/panic outside tests). `tm-core`: AppPaths, Settings
  (Kotlin keys and defaults, unknown keys kept, atomic save, corrupt file kept aside), AppLock.
  `tm-db`: schema and named queries read straight from the Kotlin `.sq`/`.sqm` files; create, migrate
  (N.sqm upgrades N to N+1), newer schema opens read-only; library, chapters, sync, progress,
  history/recents, categories, extension repos, tracks.
- P3 `tm-archive` (folder/CBZ pages in natural order, capped reads, path guard, atomic CBZ packing,
  page pixel sizes from headers), `tm-local` (LocalImporter and ChapterRecognition ports, 57 Kotlin
  cases), `tm-cache` (size-capped LRU disk cache with pinning).
- P4 `src-tauri`: Tauri 2 app, borderless window with our own title bar, 18 thin commands, `tm://`
  image scheme (covers, pages; both URL forms), ts-rs types generated into `ui/src/gen`.
  `ui`: React + TS + Vite. Tokens for Yokai Dark/Light, Midnight, Ink & Vermilion. Shell with rail,
  whole-app rotation (Ctrl+Alt+R, remembered, one root so the screen is never reset). Library
  (list/grid, category chips with counts, category editor, sort that flips on second click, unread and
  downloaded filters, search, auto-scan on first open, rescan, size classes). Detail (info, categories,
  continue/start, chapters with read toggles, mark all read). Reader (strip with chapters loading at
  both ends without a jump, L→R, R→L, 2-up with cover-solo, four fits, zoom keys and Ctrl+wheel, slider,
  progress saved with a 500 ms debounce and on close). Settings (theme cards, rotation, data folder,
  page cache size and clear). Recents/Browse/Downloads/Sync show "arrives later" states.
- P1 `ext-host/` (code written, verified only in CI because Gradle cannot resolve android-jar here):
  Ktor CIO server on 127.0.0.1, bearer token, `TM_EXTHOST_READY port=N` on stdout then stdout goes to
  stderr, exits on stdin EOF or `/v1/shutdown`. Every endpoint of PLAN §P1 (docs/rust-port/api.md).
  Install downloads to `*.part`, checks the signature, then replaces the old artifact, so a bad
  download never breaks a working install. Uninstall, per-package artifact tracking, icons kept in
  `extensions/icons.json`. `:ext-host:selfTest` covers auth, repos, catalog, install, sources,
  popular/latest/search, cover, manga+chapters, pages, page bytes, preferences, reload, uninstall,
  shutdown. `.github/workflows/ext-host.yml` builds it, runs the self-test and checks the start-up
  contract with `java -jar` on Ubuntu and Windows.
- P5 `tm-exthost`: spawns the host (bundled `runtime/bin/java`, then `JAVA_HOME`, then PATH), fresh
  32-byte token, waits up to 60 s for the ready line, forwards its stderr to the log, keeps stdin open
  and closes it on drop. Typed async client for every endpoint, 64 MB body cap, host errors kept as
  `{status, kind, message}`. 6 scripted-server tests; `tests/real_host.rs` installs MangaDex from
  Keiyoushi and reads a page through the real host (run by ext-host CI on Linux and Windows).
  App: host started lazily, restarted if it died, stopped after 10 idle minutes (not while the library
  updates). Commands for repos, catalog (per-repo failures, update detection), install (only from the
  user's own repos), uninstall, sources, popular/latest/search, open, refresh, heart, source settings,
  Cloudflare toggle, library update with `library-update` progress events. Browsed manga are stored as
  non-favourite rows so detail and reader use real ids; the heart only flips `favorite`. Online pages
  and covers go through `tm://` with the page cache. Source ids travel as strings (above 2^53).
  UI: Browse (Sources / Extensions tabs, repositories dialog with a one-tap Keiyoushi chip that is never
  added on its own, catalog grouped Update pending / Installed / Available with NSFW and language
  filters, source grid with infinite scroll and de-dupe, source settings dialog), detail heart and
  refresh with the source's real error, library update button with a progress bar, Cloudflare toggle
  in Settings, reader learns online page sizes when each image loads.
- P6 `tm-download`: port of DownloadManager (one worker, QUEUED/DOWNLOADING/DONE/FAILED, the Kotlin
  layout and hash so existing downloads keep working, folder wiped before a retry, `.done` written
  last, de-dupe, dismiss a queued job). New: completed list rebuilt from disk on start, "download
  unread", delete. Port of DownloadSelfTest's 16 checks plus restart/dismiss/incomplete tests.
  App: reader order is local file → finished download → NAS → source; `downloads` events; detail shows
  per-chapter download/progress/delete and "download unread"; Downloads screen (queue, retry state,
  "on this computer" with read and delete); Recents screen (history, resume); library "Downloaded"
  filter now real.
  `tm-remote`: Mihon names (DiskFilenames port, incl. hashed and legacy names), WebDAV client
  (PROPFIND 0/1, HEAD, GET with progress and size cap, PUT, MKCOL, DELETE, MOVE; 404 = absent;
  redirects not followed; password never in Debug), remote index saved to `remote_index.json`, read
  mirror (`cache/remote_mirror/<sha256>.cbz`, size checked, trimmed to the cache budget), and the new
  upload queue (`uploads.json`): pack CBZ, PUT, compare remote and local size, then mark done; the
  "Move to NAS" role deletes the local copy only after that check; interrupted uploads resume; a saved
  job pointing outside the downloads folder is refused. Port of RemoteSelfTest's 9 checks against an
  in-memory WebDAV server (axum, feature `fake-server`) plus upload, size-mismatch, restart, escaping
  and auth tests. App: Storage settings (address, port, HTTPS, folder, user, password never sent to
  the UI, test connection without saving, keep both / move to NAS, download from the NAS first, read
  cache size and clear), uploads after each finished download, upload list on the Downloads screen.
- P7 `tm-backup`: the `.tachibk` protobuf tag for tag with `BackupModels.kt` (prost). Fields kotlinx
  treats as required (`source`, `url`, `name`, `syncId`, ...) are always written, even when zero:
  without that a local manga (source 0) made the Kotlin app throw `MissingFieldException` (seen, then
  fixed). `favorite` and `memo` read as Kotlin's defaults when absent; `categories` is written
  unpacked like kotlinx. Export and import ported from `BackupRepo.kt` in one transaction: upsert by
  (source, url) and (url, manga), furthest progress wins, Mihon fields kept by `lastModifiedAt`,
  categories by name and additive, tracking only moves forward, extension stores only reported as
  pending, preference sections parked and handed back byte for byte. New: history goes both ways (the
  Kotlin export leaves it out); a gzip bomb or broken file is an error. Settings (and so the NAS
  password) are never in a backup. 12 tests, including restoring `tests/fixtures/kotlin-app.tachibk`,
  a backup written by the Kotlin app.
  `tm-sync`: the Kotlin LAN protocol (same routes, `X-Pair-Code`, JSON keys, port 27193 and up, mDNS
  `_tachimana._tcp` with `id`/`name` TXT), `device.json` shared with the Kotlin app. New hardening:
  manifest paths must be `<folder>/<file>` on both the serving and the pulling side (a hostile peer
  cannot write outside the library; tested), symlinks out of the library are not served, 10 wrong
  codes a minute and the server answers 429, files stream to `.part` and are length-checked. Port of
  SyncSelfTest's LAN checks plus those. 11 tests.
  App: serves and advertises at start-up (errors shown, not fatal), Sync screen (this device's code,
  port and addresses; devices found on the network; add by address; pull with progress; last result
  or error; restore and export through the system file dialog; pending extension repos with Add or
  Not now). New `tauri-plugin-dialog`.
- CI: `.github/workflows/rust-port.yml` runs UI typecheck/tests/build, cargo fmt, clippy, tests, and
  checks the generated types are committed.

- P8 `tm-track`: AniList GraphQL client (search, viewer, list entry read/save; scores sent as
  `scoreRaw`), implicit-grant sign-in (client 46514, browser capture on 127.0.0.1:43815
  `/auth/anilist`, pasted-token fallback), `anilist.json` in the Kotlin shape written 0600,
  tracker on `manga_sync` sync_id 2 that never lowers remote progress. 9 tests against a fake
  GraphQL server plus an ignored live search (passed against real AniList).
  `tm-update`: GitHub releases tagged `app-v<version>`, version compare, Windows installer download
  checked against its `.sha256`, other platforms open the release page. 5 tests.
  `tm-exthost::install`: helper zip unpacked into `<data>/ext-host` (staged, swapped, old helper
  restored on failure; path, symlink, size and count guards). 2 tests.
  App: Tracking dialog on manga detail (connect, find, linked card with status/chapters/score),
  finished chapters pushed to AniList, Settings gains AniList, extension helper (download or from
  file), Sync and update rows; quiet update check 10 s after start.
  Packaging: `tauri.conf.json` bundles NSIS (per-user) and MSI on Windows, `.deb` and AppImage on
  Linux. `app/scripts/package-ext-host.sh` builds the helper zip (jdeps + jlink runtime) with its
  `.sha256`. `.github/workflows/app-release.yml` builds all of them on `app-v*` tags (and on PRs that
  touch packaging) and publishes a release on tags.
  Checked by hand in the running app: screenshots in `docs/rust-port/shots/p8/`. Linux installers
  built here; sizes, start-up and memory in `baseline.md` ("Rust app").

Full backup restore (2026-11-03, feat/backup-restore-all). Asked by Mehdi: a restore should bring
everything back, including repos, extensions, tracking and downloads.
- A stock Mihon `.tachibk` holds library, chapters (read, bookmark, page), history, categories,
  tracking rows, sources (id + name), extension repos (tag 106) and preferences. It does not hold
  installed extensions or download state. New optional tags 900 add them; the schema is in
  `docs/rust-port/backup-schema.md`, shared with Mehdi's Mihon fork (its own thread does that side).
- Restore now adds the backup's repos (http/https only), reports them, and offers on the Sync screen:
  "Find the extensions it used" then "Install all" (tag 900 list, or for stock Mihon backups the
  catalog extensions providing the library's sources), "Download all / Unread only" for chapters
  marked downloaded, and "Sign in to AniList" when AniList rows came back without a session.
  The same applies to a LAN pull.
- Export writes tag 900 from the host's last-seen extension list (refreshed before saving, and
  whenever Browse lists extensions) and marks chapters complete on disk as downloaded.
- Both apps share `%LOCALAPPDATA%\TachiMana` on Windows, so a Rust install opens the Kotlin
  library. Mehdi's PC was wiped clean 2026-11-03 for a fresh install.

Restore progress (feat/restore-progress, 2026-11-03), from Mehdi trying 0.1.1 on his real backup:
- The restore held the one database lock for minutes on the UI thread, so the window hung and then
  crashed while he moved around. It now runs on a background thread in batches of 25 manga (each
  in a savepoint, so one bad manga is rolled back and listed as an error), and no longer re-reads
  every chapter after each insert. His backup (1,688 manga, 172,371 chapters): 202 s to 6.6 s in a
  release build, 30 s in a debug build. After restoring it under Xvfb, about 3 minutes of fast
  scrolling, opening titles and switching screens: no crash and no panic in the log. Not yet tried
  on Windows/WebView2.
- Live progress (`restore-progress` events: "Library 215 of 1,688: <title>") and a summary with the
  time and the error count, even 0; errors can be expanded.
- Dropping a file on the Restore row asks "Restore <file>?" first (`backup_restore_path`). Drag and
  drop cannot be driven under Xvfb, so this is untested in the app.
- "Find the extensions it used" downloads the extension helper first when it is missing (with
  progress). Under Xvfb the dev helper was found, so the download path ran only in unit tests. When
  a repo cannot be read, unmatched extensions are "not checked yet", not "not found".
- Logs: `<data>/logs/tachimana.log` (rotated at 5 MB) and `crash-<time>.txt` with a backtrace for
  any panic (newest 10 kept). On Windows: `%LOCALAPPDATA%\TachiMana\logs`.

UI states and updater (feat/ui-states, 2026-11-03), from Mehdi: "everything should have its states":
- Shared pieces: `busy` on every button (spinner, disabled, aria-busy), `Skeleton`/`SkeletonRows`/
  `SkeletonGrid`, `Loading` with a line, `useAction` (pending from the click, no double runs,
  error kept or toasted), toasts, and a thin activity bar under the title bar for running actions,
  mutations and first loads. Screens fade in. Reader pages show a spinner while loading and
  "Couldn't load page N · Retry" on failure. Read marks and categories change at once
  (optimistic) and roll back on failure. Every screen and dialog was gone through.
- Updater: one "Update now" downloads (bar from 0% at once) and runs the NSIS installer with
  `/P /UPDATE /R` (passive window, no pages, no uninstall, reopens the app); the banner says
  "Installing… will close and reopen". Checked under Xvfb with a fake release server
  (`TM_UPDATE_API`, plus debug-only `TM_UPDATE_PLATFORM=windows`): progress, then the installer was
  started with exactly those arguments and the app quit. The real NSIS run is untested here.
- Big freeze found while testing with Mehdi's library: `latest_names` (library list) used a
  correlated subquery, 7 s per library refresh in release (45 s debug) on 172,000 chapters, on the
  UI thread. Now one grouped pass (0.1 s, same results) and `library` runs off the UI thread. A
  test checks the query plan has no correlated subquery.

0.1.3 released 2026-11-03 18:27 UTC with the above.

AniList sign-in from a Mihon backup (feat/backup-tracker-login, 2026-11-03), from Mihon's
"Include sensitive settings" on Create backup:
- `tm_backup::prefs` reads the private preferences in tag 104: `__PRIVATE_track_token_2` (the
  JSON of Mihon's `ALOAuth`, `access_token` + `expires` ms), else
  `__PRIVATE_pref_mangasync_password_2` (the bare token). Key names from Mihon's
  `TrackPreferences.kt` and `Preference.kt` (Apache-2.0).
- After a restore, `track::adopt_from_backup` checks the token with AniList and saves the session
  with the backup's expiry, unless AniList is already connected here. The outcome is a line in
  the restore summary (`RestoreDto.tracking`); a failure never fails the restore and the "Sign in
  to AniList" card stays.
- Private preferences are no longer parked for re-export, so a later desktop backup never
  carries a token the user didn't choose to include. `TrackerLogin`'s Debug hides the token.
- Not verified: a real Mihon token (needs Mihon's sensitive backup). The test uses a synthetic one
  against a fake AniList. Other trackers (MAL, Kitsu…) aren't supported on desktop, so their
  tokens are dropped.

0.1.4 released 2026-11-03 19:20 UTC with the above.

Profiles, phase 1 (feat/profiles, 2026-11-05), from Mehdi: "accounts and profiles… when I switch
profiles, my library and all that stuff should switch". The plan for all five phases (profiles,
linking a phone by QR, downloading the phone's queue on the PC to here/NAS/phone, telling the
phone, badges) is `docs/rust-port/profiles-and-remote-downloads.md`. The phone side is filed as
mehdiacho/mihon issues #1 (linking), #2 (queue), #3 (destinations), #4 (badges).
- `tm_core::profiles`: `profiles.json` at the data root. "Main" is the root itself, so upgrading
  moves nothing. Others are in `profiles/<uuid>/`. Hostile or damaged entries are dropped, and
  folders can only be `""` or `profiles/<uuid>`.
- `AppPaths` splits root (lock, logs, device id, helper) from the profile folder (everything else).
- Commands `profiles_list/create/edit/switch/delete`. A switch saves and calls `app.restart()`;
  the new process retries the machine lock for 8 s while the old one exits.
- UI: a title-bar switcher (avatar, menu, inline "New profile"), and Settings → Profiles (switch,
  edit name and colour, delete with a typed name, moved to `profiles/.deleted/`).
- Checked under Xvfb on a synthetic library: create, switch (the app restarts into an empty
  library), switch back, delete (the folder is moved aside).

Linking, phase 2 desktop half (feat/linking, stacked on feat/profiles, 2026-11-05):
- `tm_sync::links`: `LinkStore` in the profile's `links.json` (at most 16, atomic save, the key
  hidden from Debug), 32-byte base64url keys from the OS (no fallback).
- Protocol: `X-Link-Key`; `/ping` adds `profileId`/`profileName`; `POST /link` (`LinkRequest` with
  an optional `port`, `LinkResponse`; 409 with no key for another profile); `LinkInvite` builds
  and parses `tachimana://link?h=&d=&p=&n=&c=`. mDNS TXT adds `p`.
- Server: `SyncServer::start_with(.., Some(Linking))`; every route but `/ping` takes the code or a
  known key, and wrong keys count towards the lockout. Client: a 43-character "code" is sent as
  the link key; a 401 then is `SyncError::Unlinked`.
- App: `links_info` (QR as SVG, addresses, code, list), `link_device` (address + code, or a pasted
  link code), `unlink_device`. UI: Sync → Linked devices.
- Checked: `tests/lan.rs` linking test (wrong code, other profile, key replaces code, unlink gives
  401; it fails with key auth switched off). In the running app under Xvfb: a curl "phone" linked
  (200) and another profile got 409, the row showed, Unlink emptied `links.json`.
- Not done: the phone side (fork #1), adopting the other side's profile id when they differ (the
  PC now says to link from the other device or switch profile), pushing `seen_at` updates.

Linking across profiles (feat/profile-join, 2026-11-05), from Mehdi: link even when the profiles
differ, choosing whose library to keep.
- `Profiles::adopt_id` gives a profile another device's id and keeps its folder (folders are now
  any `profiles/<uuid>`, unique, not tied to the id).
- `link_device(.., keep: "theirs")`: on 409 the PC asks again with the other side's id, then
  `take_profile` (in place if the library is empty, else a new profile), saves the link and a
  `join.json` note in that folder, and restarts. On start-up `resume_join` pulls the peer's
  library with the link key (`sync::pull_from`).
- The server remembers the last 409 (`Linking.refused`); Sync shows "<phone> tried to link from
  its own profile" with Keep <phone>'s (needs the phone's code) or Keep this PC's.
- QR copy from TachiMana #23 (camera or in-app scan, pick the install).
- Checked with two app copies under Xvfb, one standing in for the phone: an empty PC profile took
  on the phone's id in place and copied its 6 manga and 25 files; a non-empty one made a new
  profile "Mihon Temp" and copied there, with Main untouched.
- Keeping the PC's library is the phone's job: mehdiacho/mihon#9.

Merge instead of a new profile (fix/sync-merge, 2026-11-05):
- From Mehdi: he restored the phone's backup into a profile, scanned to link, and the PC looked
  wiped (no NAS, extensions offered again). Cause: a non-empty profile made a new one.
- "Keep the phone's" is now **Merge into this profile** (default) or **New profile**
  (`link_device`/`link_refused` `keep: "merge" | "new"`; `take_profile` re-ids a profile that
  already had the phone's id, or reuses it for "new").
- NAS settings travel as the fork's `remote_storage_*` preferences (`tm_backup::prefs::NasPrefs`,
  `nas::adopt_backup_nas`, `nas::backup_nas`), never the password; installed extensions are
  dropped from the restore offer after a pull or restore (`RestoreOffer::drop_installed`).
- Checked: unit tests (each seen failing with its fix reverted), and two app copies under Xvfb:
  a PC profile with 3 manga (2 shared, 1 its own) merged a 6-manga "phone" with NAS settings, in
  place, ending with 7 manga, the same folder, and the NAS set up.
- Asked the fork (mihon#11): how the NAS password travels, whether the phone changed anything at
  Mehdi's scan, #9's state, and the outbox and release replies.

Phase 3, the phone's queue on the PC (feat/phone-queue, 2026-11-06):
- tm-sync: `queue` module (wire types, `Outbox` in `outbox.json`), client `queue`, `claim`,
  `release`, `done`, `put_file`, `outbox`; server `GET /outbox?since=` (link key only).
- App: `phone_queue.rs` (jobs in `phone-jobs.json`, destination per phone). Downloads has a
  "Linked phones" section: the phone's queue, This PC / NAS / Back to the phone, "Download all
  here". Finished downloads are routed there (`downloads.rs`), NAS uploads report back
  (`nas::kick`), failures are released to the phone.
- Checked: unit tests; `tests/phone_queue.rs` against a stand-in phone; the outbox route in
  `tests/lan.rs`; and the app under Xvfb against a stand-in phone (Python, wire as in mihon#2/#3).
  Two chapters already on the PC went to the phone as CBZs named
  `downloads/Fake Source (EN)/Lantern Online/Chapter 1_86c8db.cbz`, with `sent` done events
  acked. One that failed to download (no extension) was released. Not yet seen against the real
  phone.
- Not done: the NAS outbox file (`.tachimana/outbox/<id>.jsonl`), the destination picker on
  ordinary downloads, and cancelling a phone job by hand (the phone's 24-hour claim timeout
  covers it).

Fixes from Mehdi's try of 0.1.7 on Windows (2026-11-13, branch `fix/challenge-window`):
- A terminal opened with the app: `java.exe` (the ext-host) is a console program and was started
  without `CREATE_NO_WINDOW`. `tm-exthost` now sets it. Not seen on Windows from here.
- "Solve the <site> challenge" windows opened on cover images and stayed open: the browser was
  let in without a challenge, so no `cf_clearance` ever came and the window waited out its three
  minutes, once per image. The solver now notices a loaded page with no challenge on it, adopts
  the browser's identity and cookies, retries once, and leaves that host alone for ten minutes
  after any attempt that didn't end in a clearance. The window carries a line saying what to do.
  Compiled in CI only; whether the retry then gets the image is not known.

0.1.8 (2026-11-13) carried those fixes, but Mehdi still saw the stuck challenge windows: the
fix is in the ext-host jar, and the installed helper never updated with the app. Branch
`fix/helper-updates`: a downloaded helper records the app version it was built for
(`<data>/ext-host/helper-version`) and is replaced at start-up when it is from another version
(`helper::update_if_stale`); one installed from a file is left alone. Seen in the container: a
helper with no version file was replaced by the release's zip at launch.

Usability pass asked by Mehdi 2026-11-14 (mouse and keyboard on a PC), branch `feat/usability`.
View choices that are only about this computer's layout live in the webview's storage
(`ui/src/lib/prefs.ts`), not in settings.json.
- Library categories (`CategoryBar.tsx`): the chip row scrolls with the wheel, by dragging and with
  arrows at its ends; a menu lists every category with its count; "Sections on one page" shows
  the library grouped by category with headings that fold (`LibraryRows.tsx`,
  `librarySections`). A search now looks through the whole library whatever category is picked.
- Dividers (`components/SplitHandle.tsx`): list | manga in Library and Browse, and details |
  chapters on the manga page. Drag, arrow keys, double-click to reset. The cover grids fit as
  many columns as the width allows.
- Keys and mouse: arrow keys, Home and End through the library; `/` or Ctrl+F to search, Esc
  clears it; Ctrl+1 to Ctrl+5 for the screens, Ctrl+, for Settings; Alt+Left and the mouse's
  back button go back. Reader: `[` `]` (and buttons) for the previous and next chapter, F or F11
  for full screen (no title bar or rail), Esc leaves full screen, then the reader.
- Right-click a chapter: read, mark read or unread, mark this and earlier read, download or delete.
- Checked under Xvfb on a synthetic library of 260 manga in 14 categories. Not seen on Windows.
- Round 2 (branch `feat/usability-2`): right-click a library entry (`EntryMenu.tsx`: open,
  categories, mark all read, and for online titles download unread, refresh, remove); pick several
  chapters with Ctrl+click, Shift+click, Ctrl+A or "Select" in the menu, then mark, download or
  delete them together (a plain click picks too while any are picked; Esc clears); the reader's
  fit and reading mode are menus with the choices spelled out; in full screen the reader's bars
  tuck past the edges and return when the mouse reaches the top or bottom edge. Checked under
  Xvfb the same way; the online-only menu items were not exercised (no source in the test library).
- Round 3 (branch `feat/usability-3`): Recents grouped by day with a right-click menu (continue,
  go to the manga); Settings lists its sections beside the page on a wide window and the data
  and local folders have "Open folder" (`open_folder` command); the name above a manga's
  downloaded chapters leads to the manga; the webview's own right-click menu (Back, Reload,
  Inspect) no longer shows outside text fields in a release build (kept in dev, so that part was
  not seen running). The Cloudflare setting now says a window opens, which is what happens.
- Not done yet: Browse and Sync in detail (needs a working source in the test rig), retry on a
  failed download from the Downloads screen (the job does not carry the ids).

PLANNING (Mehdi 2026-11-14): he asked to plan the whole system before building more: sync as
a companion (who leads, one-way or two-way, carriers: direct, shared storage, account), one
shared download queue, a protocol standard for work handed to the phone app, and a usability
list. Draft: `docs/plan/system-plan.md` and `docs/protocol/README.md`. It has a checklist for him
(§3), decisions D1 to D4 (§9) and questions to the phone side (mihon#13). Do not build steps C
to H of the plan before he has answered; step B (select several manga, manga page that stacks
when narrow, reader beside the library, library sorts) is his own ask and can go ahead.

STEP B (branch `feat/step-b`, Mehdi's own asks of 2026-11-14): pick several manga in the library
(Ctrl+click, Shift+click, Ctrl+A, Esc) with a bar for categories, mark read, download unread,
refresh and remove; a Genres filter (every genre in the library with counts, searchable, several
at once) and clickable genre pills on the manga page; sorts "Last read" and "Latest chapter",
"Recently added" renamed "Date added" (glossary); the manga page puts its chapters under the
details when its pane is under 760 px; "Read beside the library" (Settings > Reading and a button
in the reader's top bar, off by default; full screen still takes the window); a reload button
(F5, Ctrl+R) in the reader; linked devices show when they were last seen (`lastSeen` in
links.json, written at most once a minute, set by mDNS and by calls in with the link key).
Checked under Xvfb on the synthetic library with genres added; online-only bulk actions and
last-seen were not exercised (no source or linked phone in the rig). The chapter fix (PR 34)
shipped as 0.1.13.

SYNC STEP C (branch `feat/changes-roles`, after the phone side's answers in TachiMana#35 and
mihon#15): `changes/1` and `roles/1` are specified in `docs/protocol/capabilities/` with test
files. The computer side is built:
- `tm-sync`: `journal.rs` (a SQLite journal per profile, `journal.db`: entries, the value held
  per target, cursors per linked device; the who-wins rules run the test files), `roles.rs`, the
  routes `GET/POST /changes` and `GET/PUT /roles`, and the client calls.
- `src-tauri/src/changes.rs`: every change made here is journalled where it happens (library
  heart, categories and their order, rename, delete, read marks and page, history, reading mode);
  entries from linked devices are applied to the library; a background loop pushes about 2 s
  after a change, pulls when a linked device appears on the network and every 5 minutes (a pull
  at most once a minute otherwise), and "Sync now" runs a round at once. A cursor that is too old
  falls back to a full pull. A leader ignores its follower's entries; a follower journals but
  does not send.
- Sync screen: per linked device "Keep in step" (both ways / this PC leads / the other leads),
  sent with `PUT /roles` and again on every round until both agree; a Changes row with Sync now
  and the last round.
- Restore reads the fork's NAS connection section (tag 901) and, when it carries the password
  sealed with a passphrase (`TMP1`, Argon2id + AES-256-GCM, `tm-backup/src/seal.rs`), the Sync
  screen asks for the passphrase and saves the password for this profile's NAS. The plain
  `__PRIVATE_remote_storage_password` in tag 104 is never read.
Checked with two copies of the app under Xvfb: a phone-style entry pushed to one reached the
other by itself, a library removal on the leader reached the follower within seconds, changing
who leads on one changed it on the other, and a synthetic phone backup's NAS password unlocked
with the right passphrase and was refused with a wrong one. Shipped as 0.1.15 (PR 38).

COPY ONCE (branch `feat/take-once`, 0.1.16): the rest of roles/1 on the computer.
- `POST /roles/take` (`tm-sync` server and `SyncClient::ask_take`): a repeated request id is
  answered but not run again; the result goes into the outbox as a `take` done event.
- `src-tauri/src/take.rs`: "Copy once…" on a linked device offers merge or replace, either way.
  Taking copies their library here (`pull/1`); replace first saves this library to
  `backups/before-<device>-<time>.tachibk` in the profile folder, then takes out of the library
  what the other device doesn't have (journalled). Sending asks the other device, which shows a
  card (Merge or Replace / Decline, and "Don't ask again" for merges, stored as `autoMerge` on the
  link); the asking side reads the answer from the other's outbox every 5 s for 15 minutes and
  shows it. This side's push cursor jumps to its head before a take so older entries don't undo
  it; the pull cursor is left as it is (the spec says both).
- A notice when the other device changes who leads (`PUT /roles` hook).
- Bookmarks: "Bookmark" / "Remove the bookmark" on a chapter's menu, a mark in the chapter list,
  journalled as `chapter.bookmark`.
- Fixed on the way: a restore never put a manga back into the library when it was known here but
  taken out; now a manga in the backup's library comes back in (nothing leaves the library).
Checked with two copies of the app under Xvfb: replace sent from C and accepted on D (backup file
written, D's extra manga left the library, C's came back in), merge taken from D, a decline and an
accept shown on C, "Don't ask again" letting the next merge run without a card, a bookmark on C
reaching D by itself, and "Both ways" on C showing the notice on D. Not done: the questions after
scanning (roles/1), restores writing journal entries.

SOURCE FILTERS AND SEARCH ACROSS SOURCES (branch `feat/source-filters`, 0.1.19, review items 1-2):
- Helper: `GET /v1/sources/{id}/filters` and `POST /v1/sources/{id}/search` (`ext-host`
  `Filters.kt`, `docs/rust-port/api.md` "Filters"). States mirror the filter tree by position; a
  missing or wrong one leaves that filter as the source made it. The helper's self-test asks
  MangaDex for its filters and searches with empty and damaged states (CI, green).
- App: `browse_filters` and `browse_list(..., filters)`; `FiltersDialog.tsx`, `lib/filters.ts`.
  A helper older than 0.1.19 answers 404, which reads as "no filters", so the chip is not shown.
- Search across sources: `GlobalSearch.tsx`, `searchAll` in `store/browse.ts`, `limiter`,
  `searchOrder` and `searchSummary` in `lib/browse.ts`. Four sources at a time, first page each.
- Seen: both, in a browser against the dev UI with made-up sources and answers standing in for
  the app's commands (no real source can be reached from the container). The states the dialog
  sent were the ones chosen. NOT seen: either one against a real source in the built app.
- Found on the way and fixed: the chips under a source's search box took half the pane's height
  since 0.1.10 (the library bar they borrowed their rule from began to stretch), pushing results down.
- Not done: pinning sources (so the search is of every listed source), saved searches, "search
  this title elsewhere" from a result.

HELLO AND THE STAND-IN PEER (branch `feat/hello`, 0.1.18):
- `GET /hello` (`hello/1`, `docs/protocol/capabilities/hello-1.md`): the server lists what it
  implements; `SyncClient::hello` asks and falls back to `/ping` for older apps
  (`Hello::from_answers` is the pure rule, run against `docs/protocol/vectors/hello-1/`).
- The Sync screen asks each linked device each minute (`link_hello`) and says under it what its
  app can't do yet (`helloLine` in `ui/src/lib/sync.ts`). Not seen in the running app with a real
  older peer; the line has unit tests and the route was tried with the stand-in.
- The calls themselves do not consult `/hello` yet; they still try their route.
- Stand-in peer: `cargo run -p tm-sync --example stand_in -- serve|check` (protocol README §6).
  Tried: `check` against `serve` passes all checks, with and without `--no-changes`.

SETTINGS PAGES AND NSFW (branch `feat/settings-pages-nsfw`, 0.1.17, asked by Mehdi 2026-11-14):
- Settings is a list of pages (Profiles, Appearance, Reader, Browse, Tracking, Data and storage,
  Sync, About) with a search box. `ui/src/lib/settingsIndex.ts` lists every setting with its page
  and search words; a new setting gets an entry there and an `<Item id>` on its page. A wide window
  shows the list beside the page; a narrow one shows the list, then the page with a back arrow.
- "Show NSFW sources" (Settings, Browse; `showNsfwSources`, on by default as in Mihon). A source is
  NSFW when its extension is flagged; right-click a source in Browse to mark it yourself
  (`sourceNsfw` in settings.json, `source_set_nsfw`), which wins over the extension's flag, so a
  source such as MangaDex can stay listed with the switch off. Hidden sources are counted under the
  list with a Show link. The Extensions tab's NSFW chip starts from the switch.
Checked under Xvfb: pages, search (Enter opens the first result and outlines it), the narrow
layout. The NSFW hiding and the per-source mark have unit tests but were NOT tried with a real
source: the test rig cannot reach an extension repository.

Next step: the phone builds its half (mihon#16, #17); then the questions after scanning. Have Mehdi try merge + "Download all here" with the real phone.
Still open from Mehdi 2026-11-03: back up the NAS
connection (credentials encrypted) and NAS state (dated; the user chooses to use it or recheck),
and "sync from the original device" over the existing pairing when it is reachable, with Download
all as the fallback (needs his Mihon fork too; that work is owned by his local session, brief in
his worklog `projects/mihon-nas.md`). After he tries the Windows installer, a PR removes `desktop/`
and `release.yml` (ext-host and the Kotlin modules it needs stay).

P8 not done yet:
- Open question 1 answered 2026-11-03 (Mehdi: public releases repo); changed 2026-11-14 when
  this repository went public. Releases now go to `mehdiacho/TachiMana` itself; the updater and
  the helper download read from it from 0.1.10 on (`extHostBaseUrl` still overrides the
  helper's, and "From file…" always works). App releases here are never marked "Latest", so the
  Kotlin app's `/releases/latest` check keeps seeing its own `v*` release.
- Builds up to 0.1.9 read from `mehdiacho/tachimana-releases`, so CI still mirrors every
  release there (needs the `RELEASES_TOKEN` secret, Contents read/write on that repository
  only; without it the mirror is skipped with a warning). Once every install is on 0.1.10 or
  later: remove the mirror from `app-release.yml`, delete the secret, and archive (never
  delete) `tachimana-releases`, since 0.1.9 and older still download their helper zips from it.
- The Windows installers are built by CI only; nobody has installed one yet.
- The Tauri updater plugin was not used: it needs signed update manifests and public URLs, and the
  repo is private. The updater ports `UpdateChecker.kt` instead, as PLAN.md allows.
- The Track dialog has AniList only, like the Kotlin app.

P7 not done yet:
- The Kotlin export has no history and no `excludedScanlators`; the Rust export writes history but
  not excluded scanlators either (the app has no per-manga scanlator filter yet).
- Pulls are one-way per press, as in the Kotlin app: to sync both ways, pull on each device.
- Unknown protobuf fields from future Mihon versions are dropped on restore, as in the Kotlin app.

P6 not done yet:
- Pinned manga (never evicted by "Move to NAS") do not exist in the app yet, so nothing is pinned.
- A downloaded chapter that was moved to the NAS shows as not downloaded; it reads from the NAS. A
  "on the NAS" mark in the chapter list is not built.
- Library download badge shows a filter only (the badge wants a per-manga count).
- NAS folder names need the source's name, which the app learns when Browse lists sources
  (`source_names.json`); until then uploads for that source are skipped (logged).
- The fake WebDAV server is the only server tested; no real NAS here.

P5 not done yet:
- Downloading the host zip (PLAN §P5.1) is in Settings since P8, but where it is hosted is still
  open question 1. The app looks for `TM_EXTHOST_DIR`, then `<data>/ext-host` (installed from
  Settings), then `<resources>/ext-host`, then (debug builds) the Gradle output folder.
- "Restart once, then surface the error": the app restarts a dead host on every call instead.

## Parity table (P9)

Every Kotlin self-test and every Kotlin desktop feature, and what covers it in the Rust app.
"Test" means an automated Rust or UI test; "app" means checked by hand in the running app
(screenshots under `shots/`). Swept 2026-11-03 against `desktop/src` at the head of this branch.

### Kotlin self-tests

| Kotlin self-test (checks) | Rust coverage |
| --- | --- |
| `syncSelfTest` (32) | `tm-sync` tests (LAN pull between two instances, code check and throttling, path guard, short files discarded, port moves on); `tm-backup` tests (Mihon round trip of every stored field, passthrough sections byte-identical, Kotlin backup restored, cover relink); interop script against the Kotlin app both ways (above) |
| `libraryUpdateSelfTest` (12) | `tm-db` `sync_chapters_adds_new_and_never_touches_read_state` (new chapters added, read and in-progress state kept, source order realigned, re-run adds nothing, local untouched); app: library update button with progress (shots/p5) |
| `categorySelfTest` (29) | `tm-db` `categories_crud_order_and_assignment`; `tm-backup` `categories_match_by_name_case_insensitively`, `progress_only_moves_forward_and_categories_are_additive`; UI `libraryView.test.ts` (the self-test's view cases) |
| `downloadSelfTest` (16) | `tm-download` `download_self_test` (port of the same steps), `a_dismissed_queued_job_is_not_downloaded`, `incomplete_chapters_have_no_pages` |
| `cloudflareSelfTest` (7) | runs inside the Kotlin ext-host (its `SelfTest.kt`); the Rust app only forwards the toggle (`PUT /v1/config`), tested in `tm-exthost` |
| `trackSelfTest` (8) | `tm-track` tests (fake GraphQL server: search, sign-in session shape, bind/unbind, push never lowers, edits) plus a live AniList search (ignored by default, passed 2026-11-02); `tm-db` `track_rows_put_update_delete` |
| `browseSelfTest` (18) | `tm-exthost` `install_browse_read` against the real host (install from Keiyoushi, popular, chapters, pages, images; ignored by default) and the ext-host's `:ext-host:selfTest`. Both need an open network: run 2026-11-03 with the packed helper, it stopped at the repo fetch (the container proxy answers 403 for github.com raw), as the Kotlin test does here |
| `remoteSelfTest` (9) | `tm-remote` tests (fake WebDAV: names, index, mirror, upload queue, roles); app test `an_online_chapter_on_the_nas_is_read_from_it` |
| `migrationSelfTest` (5) | `tm-db` `migration_29_to_30_preserves_rows`, `fresh_database_has_kotlin_schema_version`, `newer_database_opens_read_only` |
| `updateSelfTest` (9) | `tm-update` `versions_compare_numerically` (the same cases plus the `app-v` prefix and overflow) and release picking |
| `jsEngineSelfTest` (7) | stays in Kotlin: the JS engine is part of the ext-host |

### Kotlin features

| Feature (Kotlin file) | Rust app |
| --- | --- |
| Borderless window, title bar, nav rail, rotation (`AppShell.kt`) | Shell (shots/p4) |
| Themes (`Theme.kt`, `Tokens.kt`) | four themes as CSS tokens, Settings (shots/p4) |
| Library list and grid, search, sort, unread and downloaded filters, categories (`LibraryScreen.kt`, `LibraryView.kt`) | Library screen; filter empty states ported in P9 (shots/p9/2) |
| Category dialog: create, rename, delete, reorder, assign (`CategoryDialog.kt`) | Categories menu on detail and `CategoryEditor.tsx`; `tm-db` and `libraryView.test.ts` |
| Manga detail, continue reading, chapter read state (`LibraryScreen.kt`) | Manga detail (shots/p6/manga-detail-downloads, p8/1-detail); `chapters.test.ts` |
| "Available on remote server" mark on chapters (`LibraryScreen.kt`, `BrowseScreen.kt`) | ported in P9: cloud mark from the saved NAS listing; the folder is listed once when a manga is opened. Tested in `nas.rs`; not seen on screen (no NAS with an online manga here) |
| Local import, rescan (`LocalImporter.kt`, `ArchiveReader.kt`) | `tm-local`, `tm-archive` |
| Reader: paged and strip, Fit W, Fit H, fit screen, original, zoom keys, cover solo, chapter to chapter (`ReaderScreen.kt`) | Reader (shots/p4 reader-*); `reader.test.ts` |
| Recents (`RecentsScreen.kt`) | Recents screen (shots/p6/recents), `tm-db` `history_drives_recents` |
| Browse: repos, catalog, install, sources, popular, search, add to library, source settings (`BrowseScreen.kt`, `SourceSettingsDialog.kt`, `ExtensionService.kt`) | Browse screens and `SourceSettingsDialog.tsx`; `tm-exthost` `real_host` tests. On screen only the offline states (shots/p5): the container cannot reach extension repos. The packed helper does start from Settings and serves Browse (log, 2026-11-02) |
| Cloudflare bypass toggle | Settings (shots/p5/settings-cloudflare) |
| Downloads queue, retry, clear finished, delete (`DownloadsScreen.kt`, `DownloadManager.kt`) | Downloads screen (shots/p6/downloads-failed-and-on-disk) |
| NAS storage: settings, test, upload queue, reading from the NAS, cache (`remote/*`) | Settings NAS section, uploads (shots/p6) |
| LAN sync, backup restore and export (`SyncPage.kt`, `sync/*`) | Sync screen (shots/p7) |
| AniList sign-in and Track dialog (`track/*`, `TrackDialog.kt`) | Tracking dialog, Settings row (shots/p8) |
| Update banner and installer (`UpdateBanner.kt`, `UpdateChecker.kt`) | banner ported in P9 (shots/p9/1) and Settings row; Windows installer run, other platforms open the release page |
| Settings page (`SettingsPage.kt`) | Settings screen (shots/p8/8, 9) |
| Packaged installer (`release.yml`, jpackage) | `app-release.yml`: NSIS, MSI, deb, AppImage, helper zips |

### Still missing or different (P9)
- Retire the `tachimana-releases` mirror once every install is on 0.1.10+ (see P8 not done yet).
- Windows: installers built by CI only; Windows measurements not taken (both apps).
- Pinned manga do not exist in either app (the Kotlin app only round-trips the backup memo).
- `browseSelfTest`-style online checks need an open network; not run here.
- Removing `desktop/` (P9.3): Mehdi chose 2026-11-03 to remove it after he has tried the Rust app's Windows installer.

Blockers / questions:
- Gradle in the cloud container works with an init script that points Maven Central at Google's
  mirror (Central rate-limits with 429) and `github.com/<repo>/raw/` at raw.githubusercontent.com
  (the proxy answers 403). The Kotlin desktop app builds and its self-tests run here now.
- Open question 1 answered (public releases repository); waits on the repository and secret above.

Deviations from PLAN.md and why:
- P8 release tags are `app-v<version>`, not `v*`: the Kotlin app's `release.yml` still owns `v*`
  until P9, and the updater must not offer a Kotlin release to the Rust app.
- P8 CI: the Rust checks already run in `rust-port.yml` (P4); the installer builds are a separate
  `app-release.yml` so a PR only pays for them when it touches packaging.
- Schema version is 30, not 29 (SQLDelight's version is migrations + 1). Plan corrected.
- SQL is embedded from the Kotlin sources rather than copied (one source of truth). Plan corrected.
- P1 and P2 done out of order because P1 needs Gradle.
- `:ext-host:fatJar` produces `ext-host.jar` plus `lib/` (manifest Class-Path) instead of one merged
  jar: merging would drop duplicate `META-INF/services` files that GraalVM, Ktor and coroutines need.
- Chapter order: online sources store `source_order` newest first (0 = newest), but the Kotlin
  LocalImporter numbers files oldest first (0 = oldest). The Rust UI orders each kind correctly. The
  Kotlin app shows `sortedByDescending(source_order)` for both, which looks right for local manga and
  backwards for online ones; worth checking in the Kotlin app.
- Tooltips use the native `title` attribute for now, not Radix Tooltip.
- Rotation is a Rust-only setting (`rotated`); the Kotlin app does not remember it. Kotlin drops
  unknown settings keys on save (found in P0), so Rust-only keys reset if the Kotlin app saves settings.

Manual checklist 6.6 (P4 part), run on Linux under Xvfb with a synthetic six-manga library:
- Library list and grid, sort menu and flip, unread filter, search, category chips, rescan: done.
- Category editor and per-manga categories: built, exercised by code path only (not screenshotted).
- Reader strip, 2-up, mode switch keeps the page, next chapter loads inline, previous chapter prepends
  without moving the view, progress saved and shown as "Reading · Page 7 of 12": done.
- Rotation (Ctrl+Alt+R) keeps the current screen: done.
- Themes: library, grid and reader in Yokai Dark, Midnight, Ink & Vermilion (shots/p4/).
- Not done: 60 fps measurement and 1920×1080 (needs a real GPU display), Windows `http://tm.localhost`
  form on a real Windows webview.

Manual check (P5), Linux under Xvfb with the host built by CI: Browse without a host, Sources and
Extensions empty states, repositories dialog, Settings toggle (shots/p5/). This container's proxy
blocks GitHub raw (403), so adding Keiyoushi shows the host's real error here. Install, listing,
chapters and page bytes are covered by `tests/real_host.rs` in ext-host CI; the online screens
themselves (grid, detail, reader) have not been seen with real data yet.

Manual check (P6), Linux under Xvfb, a seeded online manga and the fake WebDAV server
(`cargo run -p tm-remote --features fake-server --example fake_dav`): detail download marks, reading a
downloaded chapter with no host, Downloads queue with a failed job and retry, delete, Recents, NAS
settings and test connection, reading a chapter that only the NAS has, downloading it from the NAS with
the source not installed, and an interrupted upload finishing after a restart (shots/p6/).

Interop check (P7), `docs/rust-port/interop/sync-interop.sh`, Kotlin app vs Rust on two data dirs:
Rust pulls Kotlin (local manga, a Mihon-style online manga with progress, bookmark, category,
tracking; the extension store pending, not added), Kotlin pulls Rust, all five transferred files
byte-identical (`cmp`), Kotlin's `.tachibk` restored by Rust and Rust's by Kotlin with the preference
bytes intact, and both libraries print the same apart from the opt-in store. Kotlin `syncSelfTest`:
32/32 pass in this container. Manual check in the app under Xvfb (shots/p7/): the Rust app found the
Kotlin app over mDNS (JmDNS and mdns-sd talk), pulled it, showed the pending repo and added it, saved
and restored backups through the file dialog, and showed the wrong-code error; the Kotlin app then
pulled the running Rust app (10 manga, 30 files, 12 MB).

Older test results (P5): `cargo test --workspace`: tm-core 10, tm-db 13, tm-archive 4, tm-local 4, tm-cache 4,
tachimana 16, all pass; clippy -D warnings and fmt clean. `npm test` in app/ui: 26 pass.
