# TachiMana: Rust + Tauri port, build plan

Status: plan approved for implementation, 2026-11-01. Owner: Mehdi. Implementer: a coding agent.
Reviewer: Claude reviews each phase's pull request on GitHub before it merges.

This is the instruction set for the agent that writes the code. Read all of it before you start, then
work one phase at a time, in order. Do not skip ahead and do not start a phase before the previous
phase's pull request is merged.

---

## 0. What we are building, in one paragraph

TachiMana is a desktop manga reader that today is 100% Kotlin (Compose Multiplatform on the JVM).
We are moving it to a **hybrid**: a **Rust + Tauri 2** desktop app with a **TypeScript + React** UI,
plus a small **Kotlin "extension host"** helper process that keeps running Tachiyomi/Mihon extensions,
because extensions are Java bytecode and cannot run outside a JVM. The Rust app owns everything else:
the library database, local files, the reader, the disk cache, NAS (WebDAV) storage, downloads,
backup/restore, LAN sync, AniList tracking, settings and the UI. The extension host is downloaded
and started only when the user first uses an online source, so people who only read local or NAS
manga never need Java.

```
┌──────────────────────── TachiMana (Rust + Tauri) ────────────────────────┐
│  ui/ (React + TS, runs in the system webview)                            │
│      │ Tauri commands + events (typed)      ▲ tm:// image protocol        │
│  src-tauri/ (thin shell: commands, events, protocol, window)             │
│      │                                                                    │
│  crates/: tm-core · tm-db · tm-archive · tm-cache · tm-remote ·          │
│           tm-download · tm-backup · tm-sync · tm-track · tm-exthost       │
└──────────────┬────────────────────────────────────────────────────────────┘
               │ HTTP/JSON on 127.0.0.1:<random port>, bearer token
┌──────────────▼──────────── ext-host (Kotlin, headless JVM) ──────────────┐
│  wraps the EXISTING :extension, :android-compat, :engine:source-api      │
│  (APK/jar loader, signature checks, Cloudflare via JCEF, GraalJS)         │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Ground rules (apply to every phase)

### 1.1 Git and pull requests
- `main` is protected in spirit: never push to it. One branch and one pull request per phase:
  `feat/rust-p<N>-<slug>` (for example `feat/rust-p2-db`). Open the PR into `main` when the phase's
  gate passes, and stop. Claude reviews it; fix what the review asks on the same branch.
- Small commits, each one building and passing tests. Message format: `P<N>: <what changed>`
  (for example `P2: open existing tachimana.db read-only and list library`).
- **No AI attribution lines in commits or PRs** (no `Co-Authored-By`, no "Generated with" footers).
- Push at least every 30 minutes of work and at every milestone. Nothing important may exist only on
  the local machine.
- Every push updates `docs/rust-port/STATUS.md` (section 9 says what goes in it). A different person
  or agent must be able to continue from that file alone.

### 1.2 Do not break the Kotlin app
- The existing Kotlin app (`desktop/`, `engine/`, `sync/`, `extension/`, `android-compat/`) must keep
  building and passing its self-tests until phase 9. The only Kotlin you add is the new `ext-host/`
  module (phase 1). Do not refactor or delete existing Kotlin code. If ext-host needs a change in
  `:extension` or `:engine:source-api`, keep it additive and run that module's self-tests again.
- The Kotlin code is the **source of truth for behaviour**. When this plan says "port X", open the
  named Kotlin file, read it fully, and reproduce its behaviour, including the edge cases its comments
  mention. When the plan and the Kotlin code disagree, the Kotlin code wins. Write down the
  disagreement in STATUS.md.

### 1.3 Interop invariants (breaking any of these is a bug, never a style choice)
1. **Same database.** The Rust app opens the same `tachimana.db` (SQLite) the Kotlin app writes, at
   schema version **29**, tracked in `PRAGMA user_version`. Same tables, same columns, same meanings
   (source id `0` = local; `manga.url` = folder name for local; `chapter.url` = `<manga>/<file>` for
   local; `viewer` flag values 5/1/2/6 for strip/L→R/R→L/double; `manga_sync.sync_id` 2 = AniList).
2. **Same data folder.** Windows `%LOCALAPPDATA%\TachiMana`, Linux `$XDG_DATA_HOME/TachiMana` or
   `~/.local/share/TachiMana`; overridable with env `TACHIMANA_DATA_DIR`. Same sub-paths as
   `desktop/src/main/kotlin/tachimana/AppPaths.kt` (`local/`, `downloads/`, `extensions/apks`,
   `extensions/jars`, `android-compat/`, `cef/`, `cache/remote_mirror`, `settings.json`, `device.json`).
3. **Same `settings.json`.** Read and write the same JSON keys as `AppSettingsData` in
   `desktop/src/main/kotlin/tachimana/data/AppSettings.kt`. Unknown keys must be preserved on write
   (read into a map, update known keys, write the map back). New Rust-only settings get new keys.
4. **Backups are Mihon-compatible.** `.tachibk` = gzip of the protobuf `Backup` message. Field tags
   must match `sync/src/main/kotlin/tachimana/sync/backup/BackupModels.kt` **exactly**, including
   the passthrough of unknown preference sections (tags 104/105 kept as raw bytes) and the
   `backup_passthrough` table. A Mihon → TachiMana → Mihon round trip must lose nothing.
5. **Mihon file naming for NAS storage.** Port `remote/DiskFilenames.kt` byte for byte (FAT-invalid
   characters, UTF-8 byte truncation at code-point boundaries, md5 suffixes, legacy names), so files
   the phone uploaded are found and files we upload are found by the phone.
6. **LAN sync wire format.** Routes `/ping`, `/manifest`, `/backup`, `/file`, the pairing-code gate
   and the mDNS service type `_tachimana._tcp.local.` stay compatible with
   `sync/src/main/kotlin/tachimana/sync/lan/`. A Rust instance and a Kotlin instance must be able to
   sync with each other.
7. Only one app writes the DB at a time. The Rust app takes an exclusive lock file
   `<data>/app.lock` at start and shows a clear message if it is held.

### 1.4 Rust code rules
- Stable Rust, edition 2021, a Cargo workspace under `app/`. Use the latest stable versions of the
  crates in section 2 at the time you start, and commit `Cargo.lock`.
- **Never panic on input.** Files, network responses, the DB, settings, archives and backups are all
  untrusted. No `unwrap`/`expect`/`panic!`/indexing with input-derived positions in non-test code.
  Return `Result` with a per-crate `thiserror` error enum and propagate with `?`. Cap allocations
  sized by input (for example, refuse a zip entry over 200 MB, a backup over 500 MB uncompressed).
- `#![forbid(unsafe_code)]` in every crate.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean before every commit.
- Every crate has unit tests. Every phase gate lists the tests that must exist and pass.
- Logging through `tracing`, to `<data>/logs/tachimana.log` (rotate at 10 MB, keep 3).

### 1.5 UI code rules
- TypeScript `strict: true`, no `any` except at clearly-commented boundaries.
- Types shared with Rust are **generated** (section 2), never hand-copied.
- Every colour, size, radius and spacing comes from the design tokens (section 6.1). No literal
  colours or pixel values in components except `0` and `1px` borders.
- Vitest unit tests for pure logic (library sort/filter, reader paging maths, chapter ordering).
- Test every screen at 1360×840 and 1920×1080, and at the narrow breakpoint in the ratio spec §0.1.

---

## 2. Technology choices (fixed; do not substitute without asking)

| Concern | Choice |
| --- | --- |
| Desktop shell | Tauri 2 |
| UI | React 18 + TypeScript + Vite |
| Server state in UI | TanStack Query (wrap every Tauri command) |
| Local UI state | Zustand |
| Long lists and the reader strip | TanStack Virtual |
| Menus, dialogs, dropdowns, tooltips | Radix UI primitives (unstyled), styled with our tokens |
| Animation | `motion` (Framer Motion) |
| Icons | `lucide-react` (open licence; no Adobe/Google-proprietary assets) |
| Styling | CSS Modules + CSS custom properties for tokens. No Tailwind. |
| Rust ↔ TS types | `ts-rs` (derive `TS` on every type crossing the boundary; generated into `app/ui/src/gen/`) |
| Async runtime | `tokio` |
| SQLite | `rusqlite` with the `bundled` feature |
| HTTP client | `reqwest` with `rustls-tls` (no OpenSSL) |
| LAN server | `axum` |
| mDNS | `mdns-sd` |
| Protobuf | `prost` (hand-written message structs with `#[prost(tag = ...)]` matching BackupModels.kt) |
| Zip/CBZ | `zip` |
| Gzip | `flate2` |
| Serialization | `serde` + `serde_json` |
| Errors | `thiserror` (libraries), `anyhow` only in `src-tauri` glue |
| Paths | `directories` crate only as a fallback; the rules in invariant 2 decide |
| Kotlin ext-host server | Ktor CIO (already a dependency of `:sync`) + kotlinx.serialization |

### 2.1 How images reach the screen (important for smooth reading)
Never send image bytes through Tauri commands or as base64. Register a custom async URI scheme
`tm` in `src-tauri` (`register_asynchronous_uri_scheme_protocol`). The UI uses plain `<img src>`
URLs and the browser engine does the decoding, caching and GPU scrolling:
- `tm://cover/<mangaId>`: library cover (local file, cached online cover, or fetched through ext-host).
- `tm://page/<chapterId>/<index>`: a page of a library chapter (local archive, download, NAS mirror,
  disk cache, then ext-host, in that order; see 5.4).
- `tm://browse/page?source=<id>&url=<urlencoded>&image=<urlencoded>`: a page of a not-in-library manga.
- `tm://browse/cover?source=<id>&url=<urlencoded>`.
On Windows the webview sees these as `http://tm.localhost/...`; build URLs with one helper in
`app/ui/src/lib/tmUrl.ts` that handles both forms. Responses carry the right `Content-Type`
(sniff magic bytes like `DownloadManager.kt` does) and `Cache-Control: max-age=31536000, immutable`
for downloaded and local pages.

---

## 3. Target repository layout

```
TachiMana/
├─ app/                       NEW: Rust + Tauri app
│  ├─ Cargo.toml              workspace
│  ├─ crates/
│  │  ├─ tm-core/             paths, settings.json, shared models, error types, lock file
│  │  ├─ tm-db/               schema + migrations + typed queries
│  │  ├─ tm-archive/          CBZ/ZIP/folder pages, natural sort, CBZ packing
│  │  ├─ tm-cache/            size-capped LRU disk cache
│  │  ├─ tm-local/            local library importer
│  │  ├─ tm-exthost/          ext-host client + lifecycle (download, verify, start, stop)
│  │  ├─ tm-library/          add/remove, chapter sync, library refresh, history, progress
│  │  ├─ tm-download/         download queue and on-disk layout
│  │  ├─ tm-remote/           WebDAV client, remote index, mirror, upload queue
│  │  ├─ tm-backup/           .tachibk encode/decode, import/export
│  │  ├─ tm-sync/             LAN server/client, mDNS, pairing
│  │  └─ tm-track/            AniList OAuth + GraphQL
│  ├─ src-tauri/              Tauri app: commands, events, tm:// protocol, window, tray-free
│  └─ ui/                     React + TS (Vite)
├─ ext-host/                  NEW: Kotlin Gradle module, headless extension server
├─ docs/rust-port/            this plan, STATUS.md, baseline.md, api.md
└─ (existing Kotlin modules unchanged until phase 9)
```

---

## 4. Phases at a glance

| Phase | Delivers | Gate (must pass before the PR) |
| --- | --- | --- |
| P0 | Baseline numbers for today's Kotlin app | `docs/rust-port/baseline.md` filled in |
| P1 | Kotlin ext-host with an HTTP API | `./gradlew :ext-host:selfTest` all green |
| P2 | Rust workspace, `tm-core`, `tm-db` | DB fixture tests green; opens a Kotlin-made DB |
| P3 | Local library, archives, disk cache | Rust tests port `LocalImporter` + archive behaviour |
| P4 | Tauri shell + UI foundation, Library + Reader for local manga | Manual checklist 6.6 + Vitest |
| P5 | ext-host lifecycle, Browse, online reading, add to library, refresh | `tm-exthost` integration test against a real ext-host |
| P6 | Downloads, Recents, NAS storage incl. upload queue | Rust ports of download + remote self-tests |
| P7 | Backup/restore, LAN sync | Round-trip tests; Rust↔Kotlin LAN sync works both ways |
| P8 | AniList, Settings, updater, packaging, CI | Installer builds in CI; size and start-time recorded |
| P9 | Parity sweep, then retire the Compose app (only on Mehdi's OK) | Every Kotlin self-test has a Rust equivalent |

---

## 5. Phase details

### P0: Measure the current app (half a day)
1. Build today's app: `./gradlew :desktop:packageExe` (Windows) and `:desktop:packageDeb` (Linux if
   available).
2. Record in `docs/rust-port/baseline.md`: installer size, installed folder size, cold start to a
   visible library (stopwatch or a timestamp log, 3 runs), memory (Task Manager "Memory" column)
   idle on Library, while reading a local CBZ chapter of about 30 pages, and while browsing MangaDex.
3. Run every existing self-test once and record pass/fail:
   `syncSelfTest libraryUpdateSelfTest categorySelfTest downloadSelfTest cloudflareSelfTest
   trackSelfTest browseSelfTest remoteSelfTest migrationSelfTest updateSelfTest` (`:desktop:` tasks)
   and `:extension:extensionSpike`.
Gate: baseline.md committed with real numbers.

### P1: The Kotlin extension host
Goal: a headless JVM program that wraps the existing extension machinery and serves it over
localhost HTTP. It is the only place extensions ever run.

1. New Gradle module `ext-host` (add `include(":ext-host")` to `settings.gradle.kts`). Kotlin JVM,
   JVM 17 target, depends on `:extension`, `:engine:source-api`, `:android-compat`, Ktor server CIO,
   kotlinx.serialization. Main class `tachimana.exthost.MainKt`. Use the `application` plugin and a
   `shadowJar`-style fat jar task (`:ext-host:fatJar`) so it runs as `java -jar ext-host.jar`.
2. Start-up contract (document it in `docs/rust-port/api.md`):
   - Args/env: `--data-dir <path>` (the TachiMana data dir), `--port 0` (0 = pick a free port),
     env `TM_EXTHOST_TOKEN` (random 32-byte hex chosen by the Rust app).
   - After binding, print exactly one line to stdout: `TM_EXTHOST_READY port=<port>` and flush.
   - Every request must carry `Authorization: Bearer <token>`, otherwise 401. Bind 127.0.0.1 only.
   - Exit when stdin closes (the parent died) or on `POST /v1/shutdown`.
3. Reuse, do not rewrite: build the service on `desktop/.../browse/ExtensionService.kt` and
   `ExtensionRepoStore.kt` logic, copying the code into `ext-host` (the desktop module is an app, not
   a library). Keep paths identical to `AppPaths` (`extensions/apks`, `extensions/jars`,
   `android-compat`, `cef/bin`, `cef/cache`).
4. Endpoints (JSON; all under `/v1`; errors are `{ "error": "<message>", "kind": "<kind>" }` with a
   4xx/5xx status; never a stack trace in the body, log it instead):

   | Method + path | Does |
   | --- | --- |
   | `GET /v1/health` | `{ "version", "extensions": n, "sources": n }` |
   | `GET /v1/extensions` | installed extensions: pkgName, name, versionName, lang, nsfw, iconUrl?, sources[] |
   | `POST /v1/repos/catalog` `{repoUrl}` | available extensions from that repo (resolve index v2 / repo.json / legacy like `ExtensionRepoClient`) |
   | `POST /v1/repos/inspect` `{url}` | resolves a repo URL: name, shortName, website, signingKeyFingerprint, resolved index URL (used to add a repo) |
   | `POST /v1/extensions/install` `{repoUrl, pkgName}` | download, verify signature, load (jar preferred, APK fallback) |
   | `DELETE /v1/extensions/{pkgName}` | uninstall (delete artifact, unload) |
   | `GET /v1/sources` | id, name, lang, extensionPkg, supportsLatest, isConfigurable |
   | `GET /v1/sources/{id}/popular?page=n` | `{ mangas: SManga[], hasNextPage }` |
   | `GET /v1/sources/{id}/latest?page=n` | same |
   | `GET /v1/sources/{id}/search?q=&page=n` | same; default filter list for now (matches today's app) |
   | `POST /v1/sources/{id}/manga` `{manga: SManga, fetchDetails, fetchChapters}` | calls `getMangaUpdate` (tachiyomix 1.6 combined API, see ROADMAP) and returns `{manga, chapters: SChapter[]}` |
   | `POST /v1/sources/{id}/pages` `{chapter: SChapter}` | `Page[]` (index, url, imageUrl?) |
   | `POST /v1/sources/{id}/image` `{page}` or `{url}` | raw image bytes with the right Content-Type; resolves `imageUrl` when missing; uses the source's own client and headers |
   | `GET /v1/sources/{id}/preferences` | the source's PreferenceScreen as JSON (below) |
   | `PUT /v1/sources/{id}/preferences/{key}` `{value}` | runs `callChangeListener`, persists only if it returns true; returns the updated preference |
   | `PUT /v1/config` `{cloudflareBypassEnabled}` | toggles the JCEF Cloudflare resolver |
   | `POST /v1/shutdown` | clean exit |

   Model JSON uses the extension field names as they are (`url`, `title`, `artist`, `author`,
   `description`, `genre`, `status`, `thumbnail_url`, `update_strategy`, `initialized`; chapter
   `url`, `name`, `date_upload`, `chapter_number`, `scanlator`). Include `memo` as a JSON object.
   Preference JSON: `{ key, kind: "toggle"|"list"|"multiselect"|"text"|"plain", title, summary,
   enabled, value, entries?, entryValues? }`, mirroring `ui/browse/SourceSettingsDialog.kt`.
5. Concurrency: requests run on Dispatchers.IO; the Cloudflare per-host single-flight already exists
   in source-api. Timeouts: 60 s per source call, 120 s for installs.
6. `:ext-host:selfTest` (a JavaExec like the existing ones) that starts the server in-process on a
   random port and, over real HTTP: rejects a missing token; adds the Keiyoushi repo by URL
   (`inspect`), lists the catalog, installs MangaDex (jar path), lists sources, gets popular page 1,
   calls `manga` with fetchChapters=true and gets more than 0 chapters, gets pages for the first
   chapter, downloads page 0 bytes and checks they decode as an image, reads MangaDex's preference
   screen, flips one toggle and reads it back, and shuts down. Model it on `BrowseSelfTest.kt`.
Gate: selfTest green on Windows; `docs/rust-port/api.md` describes every endpoint with an example.

### P2: Rust workspace, settings, database
1. Create `app/` workspace and the crates `tm-core`, `tm-db` (others come in later phases).
2. `tm-core`:
   - `AppPaths` with the exact rules of invariant 2 (Windows `LOCALAPPDATA`, Linux XDG, the
     `TACHIMANA_DATA_DIR` override) and every sub-path from `AppPaths.kt`.
   - `Settings` that loads `settings.json` as `serde_json::Map`, exposes typed getters/setters for
     every `AppSettingsData` field with the same defaults, preserves unknown keys and writes
     atomically (write to `settings.json.tmp`, then rename).
   - `AppLock`: exclusive lock on `<data>/app.lock` (use the `fs4` crate or an OS-level file lock).
3. `tm-db`:
   - Copy every `CREATE` statement from `engine/data/src/commonMain/sqldelight/tachiyomi/data/*.sq`
     and `view/*.sq`, and every migration `migrations/1.sqm`..`29.sqm`, into
     `app/crates/tm-db/sql/` **verbatim**, and embed them with `include_str!`. Add a comment at the
     top of each copied file naming its Kotlin source.
   - Opening: if `user_version` = 0 and no tables, create the schema and set `user_version` = 29
     (match what `engine/data/src/jvmMain/kotlin/yokai/data/DatabaseFactory.kt` does, read it). If
     between 1 and 28, run the migrations in order inside a transaction. If above 29, open
     **read-only** and show "this library was written by a newer TachiMana".
   - WAL mode, `foreign_keys = ON`, a busy timeout of 5 s.
   - Typed query functions for everything the Kotlin repos use: port the queries named in the
     `.sq` files that `LibraryRepo.kt`, `CategoryRepo.kt`, `BackupRepo.kt`, `ExtensionRepoStore.kt`,
     `AniListTracker.kt` call. One Rust function per query, same SQL text.
   - **Bug to avoid** (from the Kotlin history): do not rely on `last_insert_rowid()` across pooled
     connections. Use one connection per write transaction and read the id from the same connection,
     or re-read by a unique key.
4. Fixtures: add a small Kotlin-side task or script that creates a fresh DB with the Kotlin app's
   `DatabaseFactory` and inserts 2 manga, 5 chapters, 1 category, 1 history row and 1 manga_sync row.
   Commit the resulting file as `app/crates/tm-db/tests/fixtures/kotlin-v29.db`. Also commit a v28
   database made by rolling the schema back (see `MigrationSelfTest.kt`).
Gate tests: fresh-create schema equals the Kotlin fixture's schema (compare
`SELECT type,name,sql FROM sqlite_master ORDER BY name`, normalised whitespace); the v29 fixture opens
and every row reads back with the right values; the v28 fixture migrates to 29 with data intact;
settings round-trip keeps an unknown key; a second `AppLock` fails.

### P3: Local library, archives, disk cache
1. `tm-archive`: port `local/ArchiveReader.kt`: supported archives (`.cbz`, `.zip`) and folders;
   image detection by extension; the **natural-sort comparator** exactly as written there (port its
   tests too); read one entry by name; cap entry size. Add `pack_cbz(pages, path)` for later phases.
2. `tm-local`: port `local/LocalImporter.kt` (scan `local/<manga>/<chapter>.{cbz,zip}` or folders;
   source 0; idempotent rescan; chapter numbers via the same recognition rules as
   `engine/domain/.../ChapterRecognition.kt`, so port that and its test file
   `ChapterRecognitionTest.kt` case for case; cover relink).
3. `tm-cache`: a size-capped LRU disk cache, `<data>/cache/pages/`, default **10 GB** (setting key
   `diskCacheMb`, default 10240), keys hashed to file names, an index file storing size and last
   access, eviction when over the cap, `clear()`, `usage()`. **Pinned** keys are never evicted.
   Safe if the process dies mid-write (write to temp, then rename).
Gate tests: ported natural-sort and chapter-recognition tests; importer on a temp folder with 2 manga
(one folder chapter, one CBZ) produces the same rows the Kotlin importer would (compare against
expectations taken from reading LocalImporter.kt); cache eviction order, cap, pinned keys, crash-safe
writes, clear.

### P4: Tauri shell and the UI foundation (Library and Reader for local manga)
Read `designs/TachiMana Desktop Ratio Spec.md` completely before writing UI. It is the design
source of truth, and the prototypes `designs/TachiMana Desktop.dc.html` (open in a browser) show it.
The spec talks in Compose `dp`; treat `1dp = 1 CSS px`. Implement to the spec, not to screenshots.

1. `src-tauri`: Tauri 2 app, window 1360×840 default, min size from the spec's smallest breakpoint,
   **custom title bar** (`decorations: false`, drag region per spec §2.1, our own min/max/close
   buttons, double-click to maximise). App state holds the DB, settings, cache. Register the `tm`
   protocol (section 2.1). Commands are thin: validate input, call a crate, map errors to a
   serialisable `{kind, message}`.
2. `app/ui`: Vite + React + TS. Folder layout: `src/gen` (ts-rs output), `src/api` (one typed
   wrapper per command + TanStack Query hooks), `src/theme` (tokens), `src/components` (shared
   components from spec §2), `src/screens/<Screen>/`, `src/lib`.
3. Design tokens (spec §1): every colour of the three themes (Yokai Dark default, Midnight,
   Ink & Vermilion; plus Yokai Light if present), the type scale, spacing scale, radius scale and
   the width-class ratios, as CSS custom properties on `:root[data-theme=...]`. Port the values
   from `desktop/src/main/kotlin/tachimana/ui/theme/Tokens.kt` (it already encodes the spec).
4. Shell: nav rail (spec §2.2) with Library, Recents, Browse, Downloads, Sync, Settings; theme switch
   persisted in settings; **whole-app rotation** (spec §2.18) by rotating the root container 90°
   with swapped width/height and correct pointer mapping, toggled with Ctrl+Alt+R, matching what
   `ui/shell/AppShell.kt` does. Rotating must not reset the current screen (a bug the Kotlin app
   had: keep one root and change only its transform).
5. Library (spec §3.1, port behaviour from `ui/library/LibraryScreen.kt`, `library/LibraryView.kt`,
   `ui/library/CategoryDialog.kt`): master–detail, list and cover-grid toggle, category chips with
   live counts (All / each / Uncategorised), category editor, sort (Title / Unread / Recently added
   / Last updated, click again to flip), filters (unread only, downloaded only), search over title
   and author, unread badges, empty states, rescan button. Port `applyLibraryView` as a pure TS
   function with Vitest tests taken from `CategorySelfTest.kt`. Use TanStack Virtual for the list.
6. Reader (spec §3.2, port from `ui/reader/ReaderScreen.kt`, 803 lines; read all of it):
   - Modes: continuous Strip, paged L→R, R→L, two-page Double with cover-solo; persisted per manga
     in the `viewer` column (5/1/2/6).
   - Fits: Fit / Fit width / Fit height / 100%, plus zoom 25–400% (toolbar −/%/+, keys `+` `-` `0`,
     Ctrl+wheel). Fit, zoom and cover-solo persist globally (`readerFit`, `readerZoom`,
     `readerCoverSolo`).
   - Strip: virtualised vertical list of `<img>` with known aspect ratios (read image dimensions in
     Rust once per page and cache them, so the list does not jump while loading); full-width wheel
     scrolling; keyboard scrolling; thin progress bar + page counter; auto-load the next chapter at
     the end and the previous chapter at the top **without a jump** (anchor the scroll position).
   - Paged: click zones, arrows/space/page keys, RTL order, spreads.
   - Progress: save `last_page_read`, `pages_left`, `read` when finished, and a history row, with a
     500 ms debounce, and immediately on close.
   - Preload the next 3 pages and keep the previous 2 (use `<link rel=preload>` or hidden images).
7. Settings stub (theme picker, data folder shown, disk cache size + clear button with usage).
Gate: Vitest green; manual checklist 6.6 passes for local manga; 60 fps strip scroll on a 1080p
display with a 100-page chapter (check with the webview dev tools performance panel); screenshots
of Library (list and grid), Reader (strip and double) in all three themes committed to
`docs/rust-port/shots/p4/`.

### P5: Extensions through the host, Browse, online reading
1. `tm-exthost`:
   - Lifecycle: find the host (dev: a path from env `TM_EXTHOST_DIR`, pointing at a folder with
     `bin/java` and `ext-host.jar`; prod: `<data>/ext-host/<version>/`). If missing, download the
     platform zip from the configured URL (setting `extHostBaseUrl`, see open question 1), verify its
     SHA-256 against the published `.sha256`, unzip to a temp folder, rename into place. Show
     download progress in the UI.
   - Start lazily on first need (Browse opened, an online library manga opened, refresh, an online
     download). Spawn with `--data-dir`, `--port 0`, a fresh token; read stdout until
     `TM_EXTHOST_READY`; 60 s timeout. Keep stdin open (closing it stops the host).
   - Stop after 10 minutes with no requests and no running downloads. Restart transparently on the
     next call. If it crashes, restart once and surface the error if it crashes again.
   - A typed async client for every endpoint in P1.
2. Browse screen (spec §3.4, port `ui/browse/BrowseScreen.kt`, 920 lines, and
   `SourceSettingsDialog.kt`): installed sources list with gear icon for configurable sources;
   extension repos view (add by URL, remove, one-tap suggested Keiyoushi chip, **never auto-added**);
   "Add source" catalog = union of the user's repos with per-repo failure notes and a repo tag;
   install/uninstall; popular/latest/search grids with infinite scroll (`hasNextPage`, de-dupe by
   url); manga detail with chapters and a clear message when chapter loading fails (show the real
   error); heart to add to library; reader for online chapters.
   Extension repos live in the `extension_repos` table (Rust owns the table; the host only resolves
   URLs and catalogs).
3. `tm-library`: port `LibraryRepo.addToLibrary`, `removeFromLibrary` (unfavourite keeps the row),
   `syncChapters` (insert new by url, realign `source_order`, never touch read state), and
   `LibraryUpdater` (refresh every favourited online manga via the host, one refresh at a time,
   progress shown in the Library header).
4. Cloudflare toggle in Browse → Extension repos (setting `cloudflareBypassEnabled`), pushed to the
   host with `PUT /v1/config` on every start. The interactive challenge window is opened by the host
   (JCEF), not by Tauri.
Gate: an integration test in `tm-exthost` (ignored by default, run with `--ignored` and
`TM_EXTHOST_DIR` set) that starts the real host and does install → popular → manga+chapters → pages
→ image bytes; Rust unit tests for `syncChapters` ported from `LibraryUpdateSelfTest.kt`; manual
checklist 6.6 Browse section.

### P6: Downloads, Recents, NAS storage
1. `tm-download`: port `download/DownloadManager.kt`: queue with one worker, states
   QUEUED/DOWNLOADING/DONE/FAILED, layout `downloads/<sourceId>/<sha(mangaUrl)>/<sha(chapterUrl)>/NNN.<ext>`
   + `.done` marker + `meta.json` (same hash function and naming as the Kotlin code, so existing
   downloads keep working), wipe the chapter folder before a retry, de-dupe concurrent enqueues.
   New: "download all unread", delete from the manga's list, rebuild the completed list from disk on
   start (these are open items in the ROADMAP).
2. Page source order for library chapters (`tm://page`): local archive → completed download → NAS
   mirror cache → disk cache → ext-host (then store in the disk cache).
3. `tm-remote`: port `remote/WebDavClient.kt` (PROPFIND depth 1, MKCOL, PUT, GET with progress,
   DELETE, MOVE; 404 means absent, other failures are errors), `RemoteIndex.kt` (lazy per-directory
   listing, persisted across restarts), `RemoteMirror.kt` (fetch into `cache/remote_mirror` with the
   `remoteStorageCacheMb` cap) and `DiskFilenames.kt` (invariant 5, port its behaviour exactly and
   test it with the names in `RemoteSelfTest.kt`).
   **New, not in the Kotlin app yet:** the upload queue. After a download completes and NAS storage
   is enabled, pack it as CBZ (`tm-archive::pack_cbz`) and upload it to
   `<folder>/<Source dir name>/<manga dir name>/<chapter file name>.cbz` using Mihon's names, verify
   the remote size equals the local size, then mark it mirrored. Roles: archive (evict the local copy
   only after a verified upload) and mirror (keep both). Pinned manga are never evicted. Persist the
   queue so it survives restarts.
   The WebDAV password must never be written into backups.
4. Recents screen (spec §3.3, port `ui/recents/RecentsScreen.kt`) and Downloads screen (spec §3.5).
5. Storage section in Settings: host, port, https, folder, username, password, test-connection
   button, mirror/archive role, cache sizes, clear buttons.
Gate: Rust ports of `DownloadSelfTest.kt` (16 checks) and `RemoteSelfTest.kt` against an in-test fake
WebDAV server (axum), plus upload-queue tests: verified upload, size mismatch keeps the local copy,
queue survives restart.

### P7: Backup and LAN sync
1. `tm-backup`: prost structs mirroring `BackupModels.kt` tag for tag (read it field by field;
   include every tag listed in the ROADMAP's "Mihon is now the upstream reference" entry). Port
   `sync/BackupRepo.kt`: export (library, chapters, categories by order, history, tracking,
   extension stores, passthrough sections), import (upsert by (source,url) and (url,mangaId),
   furthest progress wins, categories matched by name and added additively, extension stores only
   **surfaced** as pending for the user to accept, never auto-added).
2. `tm-sync`: port the LAN server and client from `sync/src/main/kotlin/tachimana/sync/lan/` and
   `desktop/.../sync/SyncManager.kt`, `DeviceIdentity.kt` (`device.json`): mDNS advertise/browse,
   manual host:port, 6-digit pairing code, pull backup + import, transfer CBZ files byte-identical.
3. Sync screen (spec §3.6, port `ui/sync/SyncPage.kt`): device card with pairing code, discovered
   peers, manual add, progress, restore from file, export backup, pending-repos accept card.
Gate: Rust ports of `SyncSelfTest.kt` (32 checks) including restoring a real Mihon/Yōkai `.tachibk`
(commit a small synthetic one made by the Kotlin app as a fixture), byte-identical re-export of an
imported backup's passthrough sections, and a manual test: a Kotlin instance and a Rust instance on
two data dirs pull from each other in both directions (write the steps and result in STATUS.md).

### P8: AniList, Settings, updater, packaging, CI
1. `tm-track`: port `track/AniListAuth.kt` (implicit grant, client id 46514, localhost capture on
   port 43815 at `/auth/anilist`, pasted-token fallback, `anilist.json`), `AniListApi.kt` (GraphQL),
   `AniListTracker.kt` (status mapping, `manga_sync` sync_id 2, push finished chapters, never regress
   remote). Track dialog per spec §2.22 (port `ui/track/TrackDialog.kt`).
2. Settings screen complete (spec §3.7, port `ui/shell/SettingsPage.kt`).
3. Updater: port `update/UpdateChecker.kt` behaviour (GitHub releases, version compare). Prefer the
   Tauri updater plugin if it fits the private-repo constraint (open question 1); otherwise keep the
   existing approach.
4. Packaging: Tauri bundler, Windows NSIS per-user installer (no UAC, like today) and MSI; Linux
   `.deb` and AppImage. The ext-host zip per platform is built separately: `:ext-host:fatJar` +
   `jlink` with only the modules the host needs (find them with `jdeps --print-module-deps`), zipped
   with a `.sha256`. Do not bundle the JRE inside the main installer.
5. CI: a GitHub Actions workflow that on PRs runs `cargo fmt --check`, `cargo clippy -D warnings`,
   `cargo test`, `pnpm --dir app/ui test`, `pnpm --dir app/ui build`, and the Kotlin `:ext-host:build`;
   and on `v*` tags builds and uploads the installers and the ext-host zips. Keep the existing
   `release.yml` for the Kotlin app until P9.
Gate: CI green; installers produced; `docs/rust-port/baseline.md` gains a "Rust app" column with the
same measurements as P0.

### P9: Parity sweep and retiring the Compose app
1. Fill the parity table in STATUS.md: every Kotlin self-test and every ROADMAP feature → the Rust
   test or screen that covers it, or "missing".
2. Fix every "missing".
3. **Stop and ask Mehdi** before deleting or archiving `desktop/`. Do not remove the Kotlin modules
   that ext-host depends on, ever.

---

## 6. UI details the agent must not improvise

### 6.1 Tokens
From spec §1 and `Tokens.kt`. Expose as CSS variables, for example `--tm-color-surface`,
`--tm-space-3`, `--tm-radius-md`, `--tm-type-body-size`. Theme switch sets `data-theme` on `<html>`.

### 6.2 Motion
Spec §2.19 and §4. Use `motion` for screen transitions and sheets; respect
`prefers-reduced-motion`.

### 6.3 Keyboard
Reader keys listed in P4. Global: Ctrl+Alt+R rotate, Ctrl+F focus search, Esc closes sheet or
reader. Every interactive element reachable by Tab with a visible focus ring from the tokens.

### 6.4 Errors and empty states
Never a blank screen. Every list has an empty state and an error state with a retry, worded plainly
("Couldn't reach MangaDex. Check your connection and try again.").

### 6.5 Performance budgets
Cold start to Library visible under 1 s on Mehdi's PC with 500 library entries. Strip scrolling at 60
fps. Memory under 250 MB while reading a local chapter (ext-host not running).

### 6.6 Manual checklist (run before every UI phase PR; paste the result into the PR)
- Library: list and grid, each sort both directions, both filters, search, category chips and editor,
  unread badges, rescan.
- Reader: each mode, each fit, zoom keys and Ctrl+wheel, next/previous chapter in strip without a
  jump, progress saved and resumed from Recents, rotation does not reset the page.
- Browse (from P5): add a repo, install MangaDex, popular/search with infinite scroll, detail,
  read online, add to library, source settings toggle persists.
- Themes: every screen in all three themes.

---

## 7. Open questions for Mehdi (do not block on them; use the default)

1. **Where the ext-host zip and updates download from.** The repo is private, so GitHub release assets
   need a token. Default for now: setting `extHostBaseUrl` (a URL prefix), plus an "Install helper from
   file" button in Settings. Mehdi to choose: make releases public in a separate public repo, host the
   zips on his NAS / mehdiacho.tech, or ship a "full" installer with the helper inside.
2. **Linux priority.** Default: Windows first, Linux must build and run, but polishing WebKitGTK
   differences can wait until P9.
3. **Passwords.** Default: keep the WebDAV password in `settings.json` like today. Optional
   improvement: OS keychain via the `keyring` crate.

---

## 8. Definition of done for the whole port
- Every invariant in 1.3 holds and has a test.
- Every Kotlin self-test has a Rust (or ext-host) equivalent that passes.
- The Rust app replaces the Compose app for Mehdi's daily use on Windows for a week without a
  regression he reports.
- The installer is under 30 MB and the app starts in under 1 s (measured as in P0).

## 9. STATUS.md format (update on every push)
```
# Rust port status
Updated: <date time UTC> by <who>
Current phase: P<N> <name>, branch <branch>, PR <link or "not opened">
Done in this phase: <bullets>
Next step: <one concrete step>
Blockers / questions: <bullets or "none">
Deviations from PLAN.md and why: <bullets or "none">
Test results: <command → pass/fail counts>
```
