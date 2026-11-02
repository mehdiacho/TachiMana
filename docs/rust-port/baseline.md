# Baseline: the Kotlin (Compose Desktop) app before the port

P0 of `PLAN.md`. These are the numbers the Rust app is compared against in P8. Each row says where it
was measured. The Linux column was measured in a cloud container; the Windows column needs a run on
Mehdi's PC (steps at the end).

Commit measured: `19435e4` (`main`, after PR #1). App version 1.0.0.

## Environment of the Linux column

- Linux x86-64 container, 4 vCPU, 15 GB RAM, kernel 6.18.
- No GPU and no real display: X11 is `Xvfb` at 1920×1080, Skia renders with `SKIKO_RENDER_API=SOFTWARE`
  (the GL context cannot be created there). Start-up and memory on a real desktop will differ.
- "Cold" start means a fresh JVM process. The OS file cache stays warm between runs, so these are
  warm-disk starts.
- Build JDK: OpenJDK 21.0.12 (jpackage from the same JDK).

## ⚠ The packaged app cannot open its database (found during P0)

`desktop/build.gradle.kts` sets no `nativeDistributions.modules(...)`, so jpackage/jlink bundles
only Compose's default module set:

```
java.base java.datatransfer java.xml java.prefs java.desktop java.logging jdk.crypto.ec
```

The installed app then starts with `DB open failed: java.lang.NoClassDefFoundError:
java/sql/DriverManager`: no library, no sync, nothing that needs the database. `release.yml` builds
the Windows installer with `:desktop:packageExe` from the same config, so released installers are
very likely affected too. `./gradlew :desktop:suggestRuntimeModules` asks for:

```
modules("java.instrument", "java.management", "java.naming", "java.prefs", "java.scripting",
        "java.sql", "jdk.httpserver", "jdk.javadoc", "jdk.unsupported")
```

Fixing the Kotlin app is outside P0 (the plan says not to change existing Kotlin), so this PR does
not change it. **The start-up and memory numbers below come from a local build with that one
`modules(...)` line added and not committed**, because the shipped build cannot reach a usable
Library. Sizes are given for both.

## Results

| Measurement | Linux (container, see above) | Windows (Mehdi's PC) |
| --- | --- | --- |
| Installer size, as shipped (`.deb`) | 209.1 MB (209 084 386 B) | _to measure: `packageExe` .exe_ |
| Installer size, with `modules(...)` fix | 216.9 MB (216 931 164 B) | _to measure_ |
| Installed folder, as shipped | 335 MB (`Installed-Size` 342 259 KiB) | _to measure_ |
| Installed folder, with fix | 360 MB (`Installed-Size` 367 357 KiB) | _to measure_ |
| ‣ of which bundled JRE (as shipped) | 83 MB | |
| ‣ of which app jars | 253 MB | |
| Cold start → window mapped, 3 runs | 2.51 s · 2.01 s · 2.51 s (±0.5 s, see script) | _to measure_ |
| Cold start → Library painted and still, 3 runs | 2.75 s · 2.38 s · 2.77 s (mean 2.63 s) | _to measure_ |
| Memory idle on Library (PSS, 15 s after start), 3 runs | 277 · 270 · 266 MB (RSS 282 · 275 · 271) | _to measure (Task Manager "Memory")_ |
| Memory reading a local 30-page CBZ (strip, after paging to page 30) | 613 MB PSS (618 MB RSS) | _to measure_ |
| Memory browsing MangaDex | not measured: blocked, see self-tests | _to measure_ |

The library used for start-up and memory has one local manga with one 30-page CBZ chapter
(1100×1600 JPEG pages, 8.4 MB). The DB was created by a warm-up run before the three measured runs.

## Build

| Task | Linux | Windows |
| --- | --- | --- |
| `:desktop:packageDeb` | ✅ (needs `fakeroot` installed) | n/a |
| `:desktop:packageExe` | n/a | _to run_ |

## Existing self-tests

Run once each on Linux under `xvfb-run`, commit `19435e4`.

| Task | Result | Checks |
| --- | --- | --- |
| `:desktop:syncSelfTest` | ✅ pass | 32/32 |
| `:desktop:libraryUpdateSelfTest` | ✅ pass | 12/12 |
| `:desktop:categorySelfTest` | ✅ pass | 29/29 |
| `:desktop:downloadSelfTest` | ✅ pass | 16/16 |
| `:desktop:cloudflareSelfTest` | ✅ pass | 7/7 |
| `:desktop:trackSelfTest` | ✅ pass | 8/8 |
| `:desktop:browseSelfTest` | ⛔ blocked by the container network | 3 pass, 4 fail, then aborts |
| `:desktop:remoteSelfTest` | ✅ pass | 9/9 |
| `:desktop:migrationSelfTest` | ✅ pass | 5/5 |
| `:desktop:updateSelfTest` | ✅ pass | 9/9 |
| `:extension:extensionSpike` | ⛔ blocked by the container network | 1 pass, then aborts |
| `:extension:jsEngineSelfTest` (not in the plan's list; added) | ✅ pass | 7/7 |

Both blocked tests fail on the same request: the Keiyoushi repo resolves to
`https://github.com/keiyoushi/extensions/raw/repo/index.pb`, and this container's proxy refuses
`github.com/<repo>/raw/...` for repositories outside the session (HTTP 403). The same files load
fine from `raw.githubusercontent.com`. These are not app failures, but they are not passes
either; they need a run on a normal network (below). P1's gate (`:ext-host:selfTest` installs
MangaDex from Keiyoushi) has the same dependency.

Build-time network notes for this container (not committed): Maven Central answered 429 under load,
and the `github.com/Suwayomi/Suwayomi-Server/raw/android-jar/` Maven repo is behind the same 403.
A Gradle init script that adds Google's Maven Central mirror and the `raw.githubusercontent.com`
form of the android-jar repo got the build through. Nothing in the repo needed to change.

## To fill in on Windows (Mehdi)

1. `./gradlew :desktop:packageExe` and note the `.exe` size from
   `desktop/build/compose/binaries/main/exe/`. Check whether the installed app shows
   "Library DB ready" in the title bar or a `DB error` (the module problem above).
2. Install, note the size of the install folder.
3. Cold start to a visible Library, 3 runs (stopwatch or screen recording).
4. Task Manager "Memory": idle on Library; reading a ~30-page local CBZ; browsing MangaDex popular.
5. `./gradlew :desktop:browseSelfTest` and `./gradlew :extension:extensionSpike`, record pass/fail.

## Scripts

`docs/rust-port/baseline/measure-start.sh` (start-up timing) and `docs/rust-port/baseline/mem.sh`
(PSS/RSS of a process tree), so P8 measures the Rust app the same way.
