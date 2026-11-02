# Rust port status
Updated: 2026-11-02 by Claude (implementing; the planned agent was unavailable)
Current phase: P4 Tauri shell and UI foundation, branch feat/rust-p4-ui (stacked on feat/rust-p3-local, which is stacked on feat/rust-p2-db)

Done:
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
- CI: `.github/workflows/rust-port.yml` runs UI typecheck/tests/build, cargo fmt, clippy, tests, and
  checks the generated types are committed.

Next step: P5 (ext-host client, Browse, online reading) needs P1 (the Kotlin ext-host). Gradle here gets
HTTP 429 from Maven Central, so P1 may have to be built and tested through CI.

Blockers / questions:
- Gradle in the cloud container is rate limited by Maven Central (P1, the Kotlin fixture DB for P2.4).
- Open question 1 in PLAN.md (where the ext-host zip downloads from, the repo is private) still stands.

Deviations from PLAN.md and why:
- Schema version is 30, not 29 (SQLDelight's version is migrations + 1). Plan corrected.
- SQL is embedded from the Kotlin sources rather than copied (one source of truth). Plan corrected.
- P1 and P2 done out of order because P1 needs Gradle.
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

Test results: `cargo test --workspace`: tm-core 10, tm-db 13, tm-archive 4, tm-local 4, tm-cache 4,
tachimana 16, all pass; clippy -D warnings and fmt clean. `npm test` in app/ui: 26 pass.
