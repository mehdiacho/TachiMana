# TachiMana — Architecture & Roadmap

A desktop-first manga reader, forked in spirit from [Yōkai](https://github.com/null2264/yokai).
Same core behaviour as Yōkai, but built as a native **Compose Multiplatform (JVM desktop)**
application, with cross-device **media + library sync** as a first-class feature.

Design source of truth: [`designs/TachiMana Desktop Ratio Spec.md`](designs/TachiMana%20Desktop%20Ratio%20Spec.md)
and the prototypes in `designs/`. Implement to the ratio spec, not to screenshots.

---

## 1. Decisions locked in

| Topic | Decision |
|---|---|
| UI toolkit | Compose Multiplatform (JVM desktop; Windows + Linux) |
| Primary OS for now | Windows (this machine is the build/test target); Linux kept working |
| Content pipeline | **Local-first** (CBZ/ZIP/image folders + synced content). Data layer designed so the lineage **source/extension ecosystem can be added later** without rework. |
| Sync (headline feature) | **Direct LAN pairing first.** Self-hosted server + cloud storage are later backends behind the same sync abstraction. |
| **Sync interoperability** | **Built on Mihon-lineage standard formats** — protobuf backup (library/progress/tracking/history) + the local-source folder convention (`/local/<manga>/<chapter>.cbz`) for media. Goal: any descendant (Mihon, TachiyomiSY, Yōkai, J2K…) can sync with our desktop, not just our own apps. |
| First milestone | **Vertical slice incl. sync** — app shell + read one local manga + LAN sync proven end-to-end. |
| Repo strategy | New TachiMana project; **copy** Yōkai's multiplatform modules into `engine/` and modify freely (Yōkai clone kept as read-only reference). |
| Mobile / Android | Deferred, and **not our own standalone app**. The phone side is a thin **connector plugged into existing Mihon forks** (which the user will fork + contribute upstream). So we vendor engine modules **JVM-only** (drop `androidTarget()`/AGP → no Android SDK needed); `commonMain`/`jvmMain` split is retained so an Android target could be re-added trivially if ever wanted. |
| Theming | Dark-first; three themes (Yokai Dark default, Midnight, Ink & Vermilion) per §1 of the ratio spec. |

---

## 2. What we reuse vs. build fresh

Grounded in an audit of the Yōkai clone (`X:/Projects/yokai`).

### Reuse (near-free — just add a `jvm()` target)
- **`domain`** — models + business logic. `commonMain` has **zero** `android.*` imports.
- **`data`** — SQLDelight schema + queries live in `commonMain` (11 `.sq` files). Only the DB
  **driver** swaps: `sqldelight-android-driver` → `sqldelight-sqlite-driver` (JVM).
- **`source:api`** — the `Source` interface + models. Only **3** `commonMain` files touch
  `android.*` (`Page.kt`, `ConfigurableSource.kt`, `DelegatedHttpSource.kt`) — each gets a small
  `expect/actual` or a de-Android'd shim.
- **`i18n`** — MOKO Resources, already multiplatform (JVM supported).

### Reuse with JVM replacements (moderate — swap `androidMain` deps in a new `jvmMain`)
`core:main` concentrates the platform services. JVM substitutions:

| Yōkai `androidMain` dep | Purpose | JVM replacement |
|---|---|---|
| OkHttp | HTTP client | OkHttp (runs on JVM as-is) ✅ |
| Koin + Injekt | DI | Koin (JVM) ✅ |
| androidx.preference | settings store | `multiplatform-settings` (JVM/prefs) or java `Preferences` |
| libarchive (Android) | CBZ/CBR/archive extraction | Apache Commons Compress (zip/7z) + junrar (rar/cbr) |
| unifile | file abstraction | `java.nio` / Okio (already a dep) |
| quickjs-android | JS engine for extensions | Deferred (only needed for the source ecosystem) |

### Build fresh (not present in Yōkai in a reusable form)
- **The entire desktop UI** — Yōkai's UI is legacy Android Views (127 XML layouts, 135
  Activity/Fragment/Controller classes). None ports. We rebuild in Compose Multiplatform to the
  ratio spec.
- **The sync engine** — Yōkai has backup/restore + tracker sync, but **no peer media sync**. New.
- **Desktop shell** — custom borderless titlebar, nav rail, window management, whole-app rotation.
- **Desktop packaging** — Compose `packageDistributionForCurrentOS` (MSI/DEB).

---

## 3. Target module layout (new TachiMana project)

```
TachiMana/
├─ engine/                 # Yōkai's reusable KMP modules, each gaining a jvm() target
│  ├─ domain/              #   models + logic          (reuse)
│  ├─ data/                #   SQLDelight + repos       (reuse, JVM driver)
│  ├─ source-api/          #   Source interface         (reuse, de-Android shims)
│  ├─ core/                #   platform services        (reuse + jvmMain replacements)
│  └─ i18n/                #   MOKO resources           (reuse)
├─ sync/                   # NEW — pairing, discovery, transport, conflict resolution
│  ├─ core/                #   sync protocol + state machine (commonMain, transport-agnostic)
│  └─ lan/                 #   LAN backend: discovery + HTTP transfer
├─ reader/                 # NEW — page pipeline, three fits, rotation
├─ ui/                     # NEW — Compose Multiplatform screens (rail, library, detail, reader, sync, settings)
├─ desktop/                # NEW — JVM app entrypoint, window, titlebar, packaging
└─ designs/                # existing — ratio spec + prototypes
```

Engine modules are vendored **JVM-only** (`kotlin("multiplatform")` with a single `jvm()`
target; AGP/`androidTarget()` dropped — no Android SDK needed). The `commonMain`/`jvmMain`
split is retained, so `androidTarget()` can be re-added later at low cost if the strategy
changes. Yōkai's `androidMain` code is ported into `jvmMain` (or dropped where Android-only).

---

## 4. Sync design (LAN-first, backend-abstracted)

The sync layer is written against a `SyncTransport` abstraction so LAN / server / cloud are
interchangeable backends. LAN is first.

**Interoperability is a design constraint, not an afterthought.** The wire payloads are
**Mihon-lineage standard formats** so any descendant can participate:
- **Library / progress / tracking / history / categories** → the shared **protobuf backup
  format** (the `.tachibk`/`.proto` schema common to Mihon/Yōkai). We exchange backup
  snapshots + deltas, not a private DB dump.
- **Downloaded media** → the **local-source folder convention** (`/local/<manga title>/<chapter>.cbz`)
  so a synced folder is directly restorable/pointable by any other app.
This means the future mobile side is a small connector contributed to existing forks, and even a
user on a *different* lineage app can sync with our desktop.

**What syncs:** library + reading progress + tracking + history + reader settings (as a backup
delta), and **downloaded chapter archives** (CBZ) selectively per-manga/per-device
(matches ratio spec §3.6).

**LAN backend (first):**
- **Discovery:** mDNS/DNS-SD (JmDNS on JVM) to find peers on the same network; fallback to
  manual IP + a short **pairing code** (ratio spec shows `code 482-913`).
- **Pairing:** exchange device keys once; trust stored per device.
- **Transport:** authenticated local HTTP(S) endpoint; pull/push DB deltas + stream requested
  archives. Resumable transfers for large CBZ sets.
- **Conflict policy:** last-writer-wins on progress with per-field merge; explicit prompts only
  on true divergence (per ratio spec §2.24 propagation rules).

**Honest scoping for the first slice:** the real phone↔desktop path needs code on *both* ends,
and the Android client is a later phase. So Phase 1 proves the LAN protocol **desktop↔desktop**
(two instances / a reference peer). The same code lights up phone↔desktop when the Android
target lands.

---

## 5. Reader & whole-app rotation

- Three fits (ratio spec §3.2): **spread** (2-page RTL), **fit-height + side panels**, **vertical
  strip**. Reader strip column is a user-adjustable width dial persisted per manga (§1.6).
- **Whole-app 90° rotation** (§2.18): rotate the **root content** via
  `Modifier.graphicsLayer { rotationZ = 90f }` with swapped constraints and **remapped pointer
  input** — not the OS window. Custom titlebar path preferred so windowed rotation stays available.

---

## 6. Phased roadmap

### Phase 0 — Scaffolding (foundation)
- New Gradle/CMP project; `desktop` module launches an empty themed window (custom titlebar).
- Vendor Yōkai's `domain`, `data`, `source-api`, `core`, `i18n`; add `jvm()` targets.
- Get `data` compiling + opening a SQLite DB on JVM via `sqlite-driver`. **Exit test:** app boots,
  DB migrates, window shows the three-theme token palette.

### Phase 1 — Vertical slice (the agreed first milestone)
- **Shell:** titlebar + nav rail + master–detail Library scaffold (ratio spec §2.1/§2.2/§3.1),
  dark theme only.
- **Local content:** import a folder/CBZ; it appears in Library; detail view opens.
- **Reader:** open one manga, read it in at least the strip fit (archive extraction on JVM).
- **Sync:** LAN discovery + pairing + pull of a library delta and one chapter archive between two
  desktop instances. **Exit test:** manga added on instance A shows and opens on instance B over LAN.

### Phase 2 — Library & reader to spec
- All three reader fits + display sheet (§2.21) + whole-app rotation (§2.18).
- Full Library (list+grid, categories, badges), Detail, Recents, Downloads screens to spec.

### Phase 3 — Sync hardening + backends
- Selective per-manga/per-device download matrix, storage policies, propagation rules (§3.6).
- Add self-hosted server + cloud backends behind `SyncTransport`.

### Phase 4 — Sources ecosystem (deferred content path)
- Bring up the source/extension system on desktop (JS engine, extension loading model redesign).

### Phase 5 — Android target
- Add the phone client on the retained `androidTarget()`; real phone↔desktop sync.
- Packaging/signing for both platforms.

---

## 6a. Progress log

**Phase 0 — COMPLETE & verified.**
- New CMP project builds with Kotlin 2.3.10 + Compose Multiplatform 1.9.3, Java 17 bytecode on
  the installed JDK 24. Custom borderless window, titlebar (drag via AWT — `WindowDraggableArea`
  was removed in CMP 1.9), nav rail, theme switch (Settings), `Ctrl+Alt+R` rotation transform.
- Engine reuse **proven**: `engine/{source-api, domain, data}` vendored from Yōkai and compiled
  **JVM-only** (androidTarget/AGP dropped). Total de-Android surface was tiny — dropped the 3
  online `HttpSource` classes (deferred with extensions), a source-config file, retyped
  `Page.uri` off `android.net.Uri`, added a 3-line `ProgressListener`, and wrote a JVM
  `awaitSingle` actual. `core:main`/`i18n` intentionally not vendored.
- **DB on JVM verified**: SQLDelight schema opens/creates via `JdbcSqliteDriver`; `tachimana.db`
  created at `%LOCALAPPDATA%/TachiMana/` with all lineage tables (mangas, chapters, categories,
  history, manga_sync, …) at schema **v29**. Titlebar shows live DB status.

**Phase 1 — COMPLETE.**
- **Local import**: `LocalImporter` scans `local/<manga>/<chapter>.{cbz,zip,dir}` → lineage rows
  (source 0, `manga.url`=folder, `chapter.url`="<manga>/<file>"), `ChapterRecognition` numbers.
  Idempotent re-scan. Verified via DB rows.
- **Library master–detail** bound reactively to the DB (covers, chapters, Continue reading),
  empty states, rescan. **Reader**: vertical strip fit, Commons-Compress CBZ extraction,
  Skia off-thread decode.
- **LAN sync — desktop↔desktop, DONE & self-tested.** New `:sync` module:
  - Mihon-lineage **backup protobuf** models (dependency-free copies with identical
    `@ProtoNumber` tags → byte-compatible with Mihon/Tachiyomi/SY/Yōkai/J2K) + gzip container
    codec (`.tachibk` format).
  - `SyncTransport` abstraction; **LAN backend**: JmDNS advertise/browse
    (`_tachimana._tcp.local.`) + manual host:port fallback, Ktor CIO server
    (`/ping`,`/manifest`,`/backup`,`/file`) gated by a 6-digit pairing code, Ktor client puller.
  - Desktop orchestration: `BackupRepo` (DB↔backup export/import, upsert by (source,url) &
    (url,mangaId), read-progress merge, local-cover relink), `DesktopSyncProvider`,
    `SyncManager`, and a **Sync** screen (device card + pairing code + discovered peers + manual
    add + progress).
  - **`./gradlew :desktop:syncSelfTest`** spins up two independent instances (separate DB +
    local root) in one process and proves: discovery, code enforcement (wrong code rejected),
    backup pull+import (manga+chapters), byte-identical CBZ transfer, reader extraction on the
    receiver, cover relink, and idempotent re-pull. **All 14 checks pass.**
  - Two live instances share a machine via `-Dtachimana.dataDir=` / `TACHIMANA_DATA_DIR=`.

**Post-Phase-1 fixes.** Rotate no longer resets the page (root cause: two rotation call sites
rebuilt `AppShell` → hoisted to one call site, modifier-only branch). **Backup file restore**
built: Sync tab → "Restore from backup" imports a Yōkai/Mihon/Tachiyomi `.tachibk`/`.proto.gz`
(library + chapters + read-state) via the same protobuf codec; proven by 3 added self-test
checks (now **16 checks**, all pass). Local-source entries readable; online-source entries need
the extension system (below).

## 6b. Extension system — DECISION LOCKED (in-process JVM loader)

**Goal:** be a "Suwayomi/Tachidesk competitor, done better" — run the full Tachiyomi/Mihon
extension ecosystem, but inside one native Compose app (vs. their server + web UI), with our
media+library LAN sync and whole-app rotation as differentiators.

**Chosen approach (user decision):** load extension APKs **in-process on the JVM**, driven by
our vendored `source-api` as the ABI. Reimplement the loader (learning from Suwayomi, which is
AGPL — study, don't copy) so TachiMana stays non-AGPL.

**Reference:** Suwayomi-Server (to be cloned at `X:\Projects\Suwayomi-Server`). Its proven
pipeline: **apksig** (verify) → **apk-parser** (read `tachiyomi.extension.class` meta-data for
source classes + pkgName) → **dex2jar** (DEX→JVM bytecode) → URLClassLoader → instantiate
`Source`/`SourceFactory`, with an **AndroidCompat** layer stubbing `android.*`
(`Application`, `net.Uri`, `JavaSharedPreferences`, `Environment`, `Toast`, `Log`…) against a
stripped android.jar (API 30), and an **Injekt** graph providing `NetworkHelper`, `Json`, etc.

**ABI gap to close (our `source-api` is currently a subset):**
- Restore `source/online/HttpSource` + `ParsedHttpSource` (dropped during de-Android).
- Provide **JVM impls of `eu.kanade.tachiyomi.network.*`** — `NetworkHelper` (OkHttpClient),
  `Requests` (`GET`/`POST`), `OkHttpExtensions` (`asObservableSuccess`, `awaitSuccess`,
  `newCachelessCallWithProgress`, `asObservable`). Yōkai's live in `core:main/androidMain` and
  depend on Android WebView/cookiejar → model on Suwayomi's clean JVM versions instead.
- Injekt runtime + `NetworkHelper`/`Json`/preferences bindings.

**Licensing (corrected):** Suwayomi-Server is **MPL-2.0**, and `AndroidCompat` is **Apache-2.0**
(originally TachiWeb by @null-dev). So we *vendor* AndroidCompat and *adapt* the loader with
attribution — no need to reimplement from scratch. Scope decision (user): **full compatibility**,
including JCEF WebView + GraalVM JS.

**Progress:**
- ✅ `:android-compat` (+ `:android-compat:config`) **vendored and compiling** — 322 files, the
  full `android.*`/`androidx.*` runtime, with JCEF (Chromium WebView), GraalVM JS, dex2jar,
  apk-parser, apksig, icu4j, twelvemonkeys all resolved on our Kotlin 2.3.10 / JDK 24 toolchain.
  `./gradlew :android-compat:jar` is green. One local fix (Canvas.java qualified switch labels,
  rejected by JDK 24). Attribution in `android-compat/NOTICE.md`. Repos added: Suwayomi
  android-jar raw-maven, JetBrains intellij-deps (JCEF), jogamp. Prebuilt stripped android.jar =
  `com.github.Suwayomi:android-jar:1.0.0`.
- ✅ **`source-api` reconciled to the full extensions-lib ABI.** Vendored (MPL-2.0) into
  `source-api/jvmMain`: `HttpSource`, `ParsedHttpSource`, `ResolvableSource`, `ConfigurableSource`,
  and the whole `eu.kanade.tachiyomi.network.*` package (`NetworkHelper`, `Requests` GET/POST/PUT/
  DELETE, `OkHttpExtensions` asObservable(Success)/await(Success)/newCachelessCallWithProgress/
  parseAs, `HttpException`, `ProgressResponseBody`, cookie jars/store, UA + UncaughtException +
  RateLimit interceptors) plus `eu.kanade.tachiyomi.AppInfo`. Deltas from upstream: dropped
  Suwayomi's FlareSolverr `CloudflareInterceptor` + `GetSource` reset; `parseAs` uses Injekt (not
  context params); `asObservable` uses `java.util.concurrent.atomic`. `awaitSingle` maps to our
  existing jvmMain actual. `./gradlew :engine:source-api:compileKotlinJvm` green.
- ✅ **`:extension` module built + headless spike PASSES 8/8.** Loader (`loader/PackageTools`
  dex2jar chain + `loadExtensionSources`, `ChildFirstURLClassLoader`, `BytecodeEditor` — adapted
  from Suwayomi, MPL, see `extension/NOTICE.md`), `ApkMetadata` reader (apk-parser),
  `ExtensionInstaller`, `repo/ExtensionRepoClient` (`index.min.json` + apk download), and the
  `bootstrap/ExtensionRuntime` (Koin graph `configManagerModule` + `androidCompatModule` +
  `Application`/`NetworkHelper`/`Json`, `AndroidCompatInitializer().init()`, `AndroidCompat().startApp`).
  Spike `./gradlew :extension:extensionSpike` downloads the real Keiyoushi MangaDex APK →
  dex2jar → child-first load → **61 sources** → `getPopularManga` returns live titles from
  MangaDex's API. Bootstrap fixes: shipped a minimal `server-reference.conf` (config layer reads
  `server.debugLogsEnabled` eagerly at load); added `AppInfo` (extensions reference it).
- ✅ **Browse UI wired into the app — full online read path PROVEN 8/8.** `ExtensionService`
  (`desktop/.../browse/`) boots the runtime, reloads installed APKs from the data dir, installs
  from a repo, and loads cover/page image bytes through each source's own client; wired into
  `AppContainer`. `BrowseScreen` = installed-source list → repo catalog (install) → popular/search
  grid → manga detail + chapters → reader. The reader is now source-agnostic (`ReaderPage` loader;
  local archive or online). Headless gate `./gradlew :desktop:browseSelfTest` covers install →
  popular → cover bytes → chapters → page list → **page image bytes** (real MangaDex page, 343 KB).
  Two ABI fixes landed here: (a) **APK asset merge** — dex2jar drops the APK's `assets/`, so
  `PackageTools.mergeApkAssetsIntoJar` folds them back in (MangaDex reads them via the classloader);
  (b) **`Page.uri` must be `android.net.Uri`** (extensions compile against it) — relocated the
  `Source`/`CatalogueSource`/`SourceFactory`/`UnmeteredSource`/`Page` ABI from source-api commonMain
  to jvmMain (domain/data only use `SManga`/`UpdateStrategy`, so common stays platform-free).
- ✅ **Bytecode legalizer — R8/modern extensions load (Asura Scans PROVEN 8/8, 2026-07-20).**
  Modern R8-built Keiyoushi extensions produce DEX that is ART-legal but JVM-illegal; dex2jar
  translates it 1:1 and the JVM verifier rejects it (this class of extension likely fails on
  Suwayomi too). `BytecodeEditor.fixAndroidClasses` now (a) recomputes every StackMapTable via
  `COMPUTE_FRAMES` backed by a non-classloading `ClassHierarchy` (jar classes + host classpath
  resources; the dex2jar fork's own `computeFrames` is buggy — emits a literal `java/util/Object`
  type); (b) runs `TypeLegalizer` pass 1: **retypes R8 constructor-inlined allocations**
  (`new-instance X` + `invoke-direct Super.<init>`, X's ctor stripped → dex2jar emits
  `new Super`) by flowing each NEW to its use-site demands (field owner/desc, invoke
  owner/args, areturn, checkcast, aastore-into-anewarray) via a copy-preserving
  SourceInterpreter, redirecting the paired `<init>`, synthesizing the stripped forwarding
  ctor, and redirecting ctors that skip the direct super (enum `<clinit>` → `Enum.<init>`);
  (c) pass 2: inserts `checkcast` where DEX's untyped registers merge broader than a use site
  needs (incl. below-top receivers via SWAP/DUP2_X1 juggling), using a lenient non-classloading
  `SimpleVerifier`. ABI additions: `CloudflareInterceptor` by-name in the default client
  (Keiyoushi audits interceptors via `Class.getSimpleName()`; passthrough that raises a clear
  error on CF challenges until JCEF), `okhttp-brotli` + `okhttp-zstd` artifacts,
  `SManga.memo`/`SChapter.memo: JsonObject` (tachiyomix 1.6). Gates: MangaDex spike 8/8,
  Asura Scans spike 8/8, browse self-test 8/8.
- ✅ **Extension-repo management — lineage model (self-test 12/12, 2026-07-20).** The hardcoded
  Keiyoushi catalog URL is gone. `ExtensionRepoStore` (desktop `browse/`) persists repos in the
  schema-inherited `extension_repos` table (so they round-trip through lineage backups); adding a
  repo fetches `<base>/repo.json` for name + signing-key fingerprint exactly like Mihon/Yōkai and
  rejects URLs that don't serve one. Browse gained an "Extension repos" view (add-by-URL, remove,
  empty-state with a one-tap *suggested* Keiyoushi chip — never auto-added), and the "Add source"
  catalog is now the union of the user's repos (per-repo failure notes, repo tag on rows when >1
  repo, install uses the entry's own repo URL). `browseSelfTest` covers add via repo.json →
  persisted → invalid-repo rejected → removed, plus the full online read path.
- ✅ **Reading-experience pass (self-test 15/15, 2026-07-20).** (a) **Browse pagination** —
  popular/search grids infinite-scroll through source pages (`hasNextPage`-driven, de-duped by
  url, spinner row while fetching). (b) **Reader auto-continue** — `ReaderTarget` now carries the
  whole reading sequence (`ReaderChapter` list, tapped chapter first); the strip loads the next
  chapter inline when scrolled near the end, with a "Finished/Next" transition header and an
  end-of-manga marker. Chapter loads run under `snapshotFlow` (never cancelled mid-flight); the
  title bar tracks the chapter under the viewport. Works for local archives and online sources.
  (c) **Add-to-library** — heart button on Browse detail persists manga (source id + url = lineage
  identity, backup round-trip safe) + chapter snapshot via `LibraryRepo.addToLibrary`; unfavorite
  keeps the row (read-state survives re-add). Library shows online entries (covers fetched via
  the owning source's client, Local/Online label) and opens them through the extension runtime —
  Browse↔Library connected both ways.
- ✅ **tachiyomix 1.6 combined API (2026-07-20).** Vendored `SMangaUpdate` + added
  `Source.getMangaUpdate(manga, chapters, fetchDetails, fetchChapters)` with a default bridging
  to the legacy per-call API. Modern extensions (Asura, etc.) implement ONLY this and stub the
  legacy paths with `UnsupportedOperationException` — the app now always calls it (Browse detail,
  spike, self-test). Fixed "no chapters" on Asura (151/321/210 chapters live); chapter-load
  failures now surface their real message in the detail pane instead of "No chapters".
- ✅ **DEX allocation ground truth (2026-07-20).** `DexAllocationIndex` reads the ordered
  `new-instance` types straight from the DEX (dex-reader node API); `TypeLegalizer` aligns them
  1:1 with the translated method's NEWs and retypes from ground truth (heuristic demand
  inference stays as fallback). Fixed Flame Comics (dex2jar emitted `new Object` for an
  R8-inlined Comparator whose real class only the DEX knew) — Flame 8/8 with chapters; MangaDex
  and Asura regression-clean.
- ✅ **Reader Phase-2 core + progress + Recents (2026-07-20, self-test 15/15).** Reader rewritten:
  reading modes — continuous **Strip**, paged **L→R**, **R→L**, **2-up double** — cycled from the
  top bar and persisted per-manga via the lineage `viewer` flag (5/1/2/6); position survives mode
  switches. Strip scrolls at full window width (wheel works over the side gutters), thin
  chapter-progress bar + page counter overlay, and chapter loading is now **bidirectional** —
  scrolling to the top prepends the previous chapter with viewport anchoring (no jump), forward
  appends as before. Flicker fixed with a global LRU decoded-image cache (revisited pages render
  synchronously). Paged modes: click-zone + arrow/space/page-key turning, RTL-reversed layout and
  spreads, same chapter window as strip. Progress persists per page (`last_page_read`,
  `pages_left`, `read` on finish) + history rows (`history` upsert) → **Recents screen** lists
  recently read with one-click resume at the stored chapter.
- ✅ **Lineage backup: extension repos + export + opt-in restore (2026-07-21, sync self-test
  22/22).** `Backup` gained Mihon tag **106** `backupExtensionRepo: List<BackupExtensionRepos>`
  (baseUrl 1 / name 2 / shortName 3 / website 4 / signingKeyFingerprint 5 — verified verbatim
  against Mihon v0.18.0; Mihon main renamed it `BackupExtensionStore` but stays wire-compatible
  on the fields we use). Export includes the `extension_repos` table; **Sync page gained "Export
  backup"** (native save dialog → `.tachibk` importable by Yōkai/Mihon/Tachiyomi). Restore is
  **opt-in by design**: `import()` only *surfaces* unknown repos as `ImportSummary.pendingRepos`
  (never auto-adds); the Sync page shows an accept/"Not now" card listing them, and accepting
  upserts from the backup's own metadata (works offline). Covers file-restore AND LAN-pull paths.
- ✅ **Reader fits + zoom (2026-07-21).** `ReaderFit` cycle in the top bar — **Fit / Fit W /
  Fit H / 100%** — plus a zoom level (25%–400%): toolbar − % + (click % to reset), `+`/`-`/`0`
  keys, and **Ctrl+wheel** (intercepted at `PointerEventPass.Initial` so lists don't scroll).
  Strip: width = fit×zoom (100% uses the image's own pixel size), clamped to the window. Paged:
  pages sized *explicitly* from image intrinsics per fit×zoom inside a both-axis scroll container
  (min-size-pinned box so small pages center — scroll containers measure unbounded and ignore
  `fillMaxSize`). Strip also gained keyboard scrolling (arrows/space/page keys, focus-requested).
- ✅ **Loader hardening (2026-07-21, browse self-test 17/17).** (a) **Lib-version gate** —
  installs now reject extensions outside `LIB_VERSION_MIN..MAX` (1.3–1.6) with a clear
  too-old/update-app message. Exposed a real bug: AAPT stores non-string manifest values as
  typed binaries which apk-parser renders `{type:data}` (`android:value="1.4"` →
  `{4:1068708659}`, float bits) — `ApkMetadata` now decodes float/int typed values.
  (b) **apksig signature trust** — `ApkSignature.certSha256` (Google's pure-Java apksig,
  v2/v3 from API 24) must match the repo.json `signingKeyFingerprint`; mismatch deletes the
  download and aborts install. (c) **ConfigurableSource settings** —
  `ExtensionService.preferenceScreen()` builds the extension's own `PreferenceScreen` against
  its file-backed shared prefs; `SourceSettingsDialog` renders Switch/CheckBox, List (dropdown),
  MultiSelect, EditText and plain prefs with androidx semantics (`callChangeListener` →
  persist only on true). Gear icon on Browse source rows. Self-test: MangaDex's 12-pref screen
  builds and a real toggle round-trips through the commit path.
- ✅ **AniList tracking (2026-07-21, track self-test 8/8).** Registered client **46514**;
  OAuth2 **implicit grant** with a one-shot localhost capture server
  (`http://localhost:43815/auth/anilist` — the token arrives in the URL fragment, the landing
  page JS bounces it into a query string), pasted-pin-token fallback, session in `anilist.json`
  (~1-year tokens, no refresh/scopes — AniList has neither). `AniListApi` = raw GraphQL over
  OkHttp (search unauthenticated; Viewer/MediaList/SaveMediaListEntry with bearer). Tracks live
  in lineage `manga_sync` (**sync_id 2**, Yōkai status ints) and now **export/import through
  `.tachibk`** (`BackupManga.tracking`, furthest-progress merge). UX: Track button on Browse
  detail (auto-favorites — the FK needs a library row) → TrackDialog (connect → search/bind,
  adopting any existing remote entry → bound view with status dropdown/untrack/open-in-browser);
  finished chapters push automatically (floored chapter number, never regresses remote).
  Self-test: live AniList search, status-mapping bijection, manga_sync persistence, backup
  round-trip. The interactive browser login is the one path needing a human test.
- ✅ **Cloudflare bypass via embedded Chromium (JCEF) (2026-07-21, cloudflare self-test 7/7 +
  live CEF bring-up verified).** `CloudflareInterceptor` (source-api) now consults a registered
  `CloudflareResolver` on a challenge (403/503 + `Server: cloudflare` or `cf-mitigated: challenge`):
  on solve it retries the request (the cookie jar + UA interceptor pick up the harvested state),
  else a clear error. The resolver — `WebViewCloudflareSolver` — is registered by the desktop app,
  gated by an opt-in `AppSettings.cloudflareBypassEnabled` toggle (Browse → Extension repos). CEF
  bring-up (`CefManager` + `Platform`, vendored from Suwayomi MPL-2.0, self-contained) downloads
  the JetBrainsRuntime Chromium bundle (~100 MB) into the app data dir **on first actual use** and
  inits a windowless `CefApp` — verified end-to-end on Windows (download → init → Chromium 144
  launch). Solve is two-stage: **headless** off-screen browser runs Cloudflare's JS proof-of-work
  ("I'm Under Attack" mode) and harvests `cf_clearance` + the real Chromium UA (drives OSR repaints
  via `wasResized`/`setWindowlessFrameRate`/`invalidate` so the challenge JS ticks); if that can't
  clear it, an **interactive window** opens so the user solves an interactive Turnstile CAPTCHA
  once (the Mihon/Tachiyomi approach — we harvest the cookie, we do NOT try to defeat the CAPTCHA).
  Self-test proves detection + resolver seam + retry + disabled-gating (7/7) without downloading
  Chromium; the JCEF path is exercised via `-Dtachimana.cef.live=<url>`.
- ✅ **GraalVM JavaScriptEngine (2026-07-21, js self-test 7/7).** Added the missing extensions-lib
  ABI class `eu.kanade.tachiyomi.network.JavaScriptEngine` (tachiyomix 1.4) — sources that
  de-obfuscate image URLs / unpack packed code resolve it from the DI graph and call
  `evaluate<T>(script)`. Backed by the already-vendored `app.cash.quickjs.QuickJs` (GraalVM `js`
  engine, polyglot artifacts already on the classpath). Registered as a Koin single in
  `ExtensionRuntime` (`single { JavaScriptEngine(app) }`). Verified it evaluates real JS on stock
  JDK 24 (string/int/array/JSON.parse/computed-URL/host-binding) — GraalJS runs interpreted
  (no Graal compiler) which is correct; silenced the per-eval "no optimizing runtime" warning via
  `-Dpolyglot.engine.WarnInterpreterOnly=false` set at runtime start.
- ✅ **Reader polish + Downloads + WebView handler (2026-07-21, download self-test 12/12).**
  (a) **Fit/zoom persistence** — reader fit mode + zoom + double-page cover-solo now persist
  globally in `AppSettings` (`readerFit`/`readerZoom`/`readerCoverSolo`), threaded through a
  `ReaderPrefs` bundle on `ReaderTarget`; last-used sticks across reads. (b) **Double-page
  cover-solo** — in DOUBLE mode the first page (cover) stands alone so later pages pair like the
  print volume; toggle appears in the reader top bar only in DOUBLE mode; backward-load anchor
  math accounts for the extra solo unit. (c) **Downloads** — new `DownloadManager` (queue via a
  Channel + single worker, reactive `jobs` StateFlow, on-disk layout
  `downloads/<sourceId>/<sha mangaUrl>/<sha chapterUrl>/NNN.<ext>` + `.done` marker + `meta.json`,
  extension sniffed from magic bytes). Depends on a `ChapterPageOpener` fun-interface (real impl
  wraps `ExtensionService`; tests fake it). Browse detail chapter rows gained a download control
  (idle/queued/progress/done); new **Downloads screen** behind the existing nav entry shows the
  queue with progress + clear/dismiss; the reader prefers downloaded files over the network.
  Self-test covers queue transitions, disk layout, failure handling, de-dup, delete. (d) **WebView
  `InitBrowserHandler`** — registered `CefCookieSeedingInitHandler` (source-api) as the DI
  dependency `KcefWebViewProvider` resolves on browser creation (previously any extension that
  opened a WebView directly crashed); it seeds CEF's cookie store from the OkHttp session.
- ✅ **Review-driven bug fixes (2026-07-21).** An adversarial multi-agent review (partially run —
  the account hit its monthly spend limit, so 2 of 5 dimensions completed) surfaced 6 real bugs,
  all fixed + regression-covered where testable. Downloads (self-test 12→16): `download()` now
  wipes the chapter dir before writing (a failed attempt left partial pages → a retry with a
  different page count/format returned stale/duplicate pages); `nextId`→`AtomicLong` + a
  `synchronized` enqueue dedup (concurrent enqueues could double-run / collide ids). Cloudflare:
  `CefManager.obtain` waits for `INITIALIZED` before returning; `hasClearance` completes on
  enumeration-done (was blocking the full 5s→1.5s per poll, defeating the cadence); per-host
  single-flight `Mutex` (concurrent challenges collapsed to one solve); interactive window aborts
  the poll on user-close + EDT teardown. The 3 un-run dimensions (reader math, tracking, backup)
  were re-reviewed inline — no further real bugs.
- ✅ **Library refresh + unread badges + reactive detail (2026-07-21, library-update self-test
  12/12).** `LibraryUpdater` (desktop `library/`) checks every favorited **online** manga for new
  chapters via the combined `getMangaUpdate(fetchChapters=true)` API and folds them in
  (`LibraryRepo.syncChapters`: insert new by url, realign `source_order`, **never touch read
  state/page progress**); progress surfaced in the Library header, one refresh at a time (`Mutex`).
  Depends on a `ChapterFetcher` fun-interface (real impl wraps `ExtensionService`; tests fake it).
  Library rows now show a live **unread badge** (`unreadCountFlow` off `findUnreadByMangaId`), and
  the detail chapter list is **reactive** (`chaptersFlow`) so new chapters appear after a refresh.
  Refresh button now does both paths (local folder rescan + online check); online refresh is manual
  (network), local auto-scans on open.
- ✅ **Library categories + grid/sort/filter/search (2026-08-11, category self-test 29/29).**
  Categories live on the schema-inherited `categories`/`mangas_categories` tables via a new
  `CategoryRepo` (create/rename/reorder/delete, per-manga assignment, reactive `categoriesFlow` +
  one `assignmentsFlow` for the whole library). **They round-trip through `.tachibk`**: export writes
  `backupCategories` (name + order + flags) and each manga's membership as the category **order**
  values — the exact lineage contract Mihon/Tachiyomi use — and import matches on NAME (creating only
  unknown ones, never duplicating), then adds memberships **additively** so a restore can't drop the
  user's own assignments. Library UI: category chip row with live counts (All / each / Uncategorized)
  + a category editor dialog (also reachable per-manga from the detail pane), a **cover grid** view
  toggle, **sort** (Title / Unread / Recently added / Last updated, click to flip direction),
  **filters** (unread-only, downloaded-only) and a **working search box** (title + author) — all
  persisted in `AppSettings`. Sorting/filtering is a pure `applyLibraryView` over
  `LibraryEntry(manga, unread, categoryIds)`, so the self-test pins the semantics; unread counts now
  come from one `chapters.unreadCounts` group-by instead of a per-row query. One real bug caught by
  the test: `last_insert_rowid()` returns 0 through the JDBC driver's connection pool, so `create()`
  re-reads the row by name.
- ✅ **Keiyoushi's new extension standard — index v2 + JVM jars (2026-08-23, browse self-test
  27/27, spike 8/8 on MangaDex *and* Asura via jar).** Keiyoushi changed how repos and artifacts are
  published, and our old assumptions had gone stale:
  - **Index v2.** A repo is now identified by its **index URL** (`…/extensions/raw/repo/index.pb`) —
    a gzipped protobuf `Index` message (schema: mihonapp/tachiyomix `index/index.proto`) carrying
    name/badgeLabel/signingKey/contact plus either an inline extension list or a URL to one. Legacy
    `repo.json` lives on and now holds an `index_v2` pointer. `ExtensionRepoClient` resolves all
    three shapes the way Mihon's `ExtensionStoreService` does — sniff gzip, then dispatch on the
    first byte (`[` legacy list → sibling repo.json, `{` JSON repo.json-or-index, else protobuf) —
    and follows `index_v2` once at add-time, persisting the URL that actually resolved.
    **This was a live breakage, not a nicety:** `index.min.json` has not been regenerated since
    2026-07-29 while `index.pb` updates daily, so our catalog was frozen (1374 extensions now visible).
  - **JVM jars — the big one.** Keiyoushi publishes a **JVM jar** per extension alongside the APK
    (`Resources.jarUrl`, proto field **501**, their documented divergence from upstream, for hosts
    like us). It carries real Java-11 classfiles, a *plain-text* `AndroidManifest.xml`, `assets/`,
    and a v1 JAR signature by the same cert as the APKs. Installing from it **skips dex2jar,
    `TypeLegalizer`, frame recomputation and the assets-merge hack entirely** — the whole class of
    R8-shaped failures we built that machinery for. Proven on Asura Scans, the extension that forced
    the legalizer's existence. The APK pipeline stays as the fallback for repos that ship only APKs.
  - **Trust.** Jars use v1 (JAR) signing, which apksig doesn't cover, so `ApkSignature.jarCertSha256`
    verifies them the JDK way (a verifying `JarFile`, every entry read to EOF, one signer for all
    entries) and matches it against the repo's declared key. Self-test covers both the real cert and
    a tampered jar with an injected unsigned class. The signature is stripped only *after*
    verification, before we rewrite classes — otherwise the class loader rejects its own jar.
  - **Manifest metadata + ABI gate.** Our keys were already the new `tachiyomix.*` ones; the gate is
    now the lineage's discrete set `{1.4, 1.6}` rather than a 1.3–1.6 range.
  - Bug found by the new legacy test: legacy repos write source ids as JSON *numbers or* strings —
    `RepoSource.id` now accepts either.
- ✅ **Mihon is now the upstream reference** (2026-09-19, sync self-test 32/32, new migration
  self-test 5/5). Tachiyomi is no longer maintained, so "what does upstream do?" is answered
  against Mihon from here on. Auditing the backup format against *current* Mihon exposed silent
  data loss:
  - **kotlinx protobuf drops undeclared tags on decode.** Any field Mihon writes that we didn't
    model was not "ignored" — it was destroyed by a restore-then-export. We were missing
    `BackupManga` 106/107/109/110/111/112 (incl. the user's per-manga **notes**),
    `BackupChapter` 11/12/13, `BackupTracking.private` (12), `BackupCategory.id` (3), and
    Mihon's whole preferences sections (104/105).
  - **Tag 106 changed meaning.** Mihon renamed extension *repos* to extension **stores** when
    repos moved to index v2: field 1 is now the index URL, and 6–8 (discord, isLegacy,
    extensionListUrl) were added. Our `BackupExtensionRepos` is now `BackupExtensionStore`.
  - **Widened types** to match Mihon: category `order`/`flags` and `BackupManga.categories` →
    `Long`, chapter `lastPageRead`/`sourceOrder` → `Long`.
  - **Migration 29** adds the columns that carry this (`notes`, `version`, `last_modified_at`,
    `favorite_modified_at`, `memo` on mangas; `last_modified_at`, `version`, `memo` on chapters)
    plus a `backup_passthrough` table that parks Mihon's preference messages **verbatim** — we
    have no mapping for its preference keys, but re-emitting the raw sub-message bytes keeps a
    Mihon → TachiMana → Mihon round-trip lossless.
  - **New `migrationSelfTest`.** Every other self-test opens a *fresh* DB and takes the
    `CREATE TABLE` path, so no `.sqm` was ever executed by the suite — a broken migration would
    have passed everything and only failed on a machine with a real library. It now rolls a live
    DB back one version and reopens it through `DatabaseFactory`.
- ⏭ **Next:** (1) Global search across installed sources. (2) Category assignment at add-to-library
  time (Browse detail) + default-category setting. (3)
  Extension-direct WebView still needs CEF *initialized* (same opt-in as the CF bypass) —
  auto-bring-up CEF when an extension opens a WebView. (4) Download: "download all", delete from a
  manga's list, scan-on-boot to rebuild the completed list. (5) GraalVM JS interpreted-only —
  revisit only if a source ships a heavy unpacker. (MAL tracking: dropped at the user's request.)

## 7. Key risks / open technical items
- **Extension loader fidelity** — ✅ **de-risked and proven** (§6b): the headless spike loads a
  real multi-source extension (MangaDex, 61 sources) and makes a live API call, 8/8. dex2jar
  (`de.femtopedia.dex2jar` 2.4.37) works on JDK 24; the AndroidCompat stub surface + Injekt/Koin
  graph are sufficient for at least API/JSON sources. Remaining fidelity work: Cloudflare/WebView
  sources (JCEF path), JS-engine sources (GraalVM binding), and asset-reading sources.
- **Compose Multiplatform vs. Yōkai's androidx Compose:** reused modules are non-UI, so the UI
  is authored fresh against `org.jetbrains.compose` — no Compose-flavour conflict.
- **Sync conflict semantics** need care once real two-way editing happens (Phase 3).
- **Sync hardening**: pairing is a shared code over plain HTTP today; TLS + trust-on-first-use
  before ship (isolated behind `SyncTransport`).
