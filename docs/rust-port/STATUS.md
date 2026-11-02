# Rust port status
Updated: 2026-11-02 by Claude (implementing; the planned agent was unavailable)
Current phase: P6 done on feat/rust-p6-downloads (PR to main); next P7 (backup/restore, LAN sync)

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
- CI: `.github/workflows/rust-port.yml` runs UI typecheck/tests/build, cargo fmt, clippy, tests, and
  checks the generated types are committed.

Next step: P6 PR review and CI, then P7 (backup/restore, LAN sync).

P6 not done yet:
- Pinned manga (never evicted by "Move to NAS") do not exist in the app yet, so nothing is pinned.
- A downloaded chapter that was moved to the NAS shows as not downloaded; it reads from the NAS. A
  "on the NAS" mark in the chapter list is not built.
- Library download badge shows a filter only (the badge wants a per-manga count).
- NAS folder names need the source's name, which the app learns when Browse lists sources
  (`source_names.json`); until then uploads for that source are skipped (logged).
- The fake WebDAV server is the only server tested; no real NAS here.

P5 not done yet:
- Downloading the host zip on first use (PLAN §P5.1) waits on open question 1 (where it is hosted).
  Today the app looks for `TM_EXTHOST_DIR`, then `<resources>/ext-host`, then (debug builds) the
  Gradle output folder; without it Browse says so.
- "Restart once, then surface the error": the app restarts a dead host on every call instead.

Blockers / questions:
- Gradle in the cloud container: Maven Central works through a mirror init script, but jitpack (401)
  and github.com/Suwayomi raw (403, android-jar) are blocked, so Kotlin builds only run in CI.
- Open question 1 in PLAN.md (where the ext-host zip downloads from, the repo is private) still stands.

Deviations from PLAN.md and why:
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

Older test results (P5): `cargo test --workspace`: tm-core 10, tm-db 13, tm-archive 4, tm-local 4, tm-cache 4,
tachimana 16, all pass; clippy -D warnings and fmt clean. `npm test` in app/ui: 26 pass.
