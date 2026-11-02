# Rust port status
Updated: 2026-11-01 20:10 UTC by Claude (coding agent, cloud session)
Current phase: P0 Measure the current app, branch claude/optimistic-newton-hqdd6i, PR opened (see GitHub)
Done in this phase:
- `docs/rust-port/baseline.md` with the Linux numbers: installer and installed sizes, 3 cold starts,
  memory idle and while reading a 30-page CBZ, and a run of every existing self-test.
- Found that the packaged app (jpackage) lacks `java.sql` and other modules, so the installed app
  cannot open its database. Details and the suggested `modules(...)` line are in baseline.md. Not
  fixed here: P0 must not change the Kotlin app. Needs Mehdi's decision (separate fix PR).
- Measurement scripts in `docs/rust-port/baseline/` for re-use in P8.
Next step: Mehdi fills the Windows column of baseline.md and runs `browseSelfTest` and
`extensionSpike` on a normal network; then merge this PR and start P1 on `feat/rust-p1-ext-host`.
Blockers / questions:
- The cloud container cannot reach `github.com/keiyoushi/extensions/raw/...` (proxy 403), so
  `browseSelfTest`, `extensionSpike` and the MangaDex memory number are open. P1's gate needs the
  same URL; P1 must run its selfTest on Mehdi's PC or a network without this restriction.
- Should the packaged-app module fix ship now, ahead of the port? (Recommended: yes, it is one line.)
Deviations from PLAN.md and why:
- Branch is `claude/optimistic-newton-hqdd6i`, not `feat/rust-p0-baseline`: the cloud session may
  only push to its assigned branch.
- Baseline measured on Linux in a container, not Windows; the Windows column is left for Mehdi.
- Start-up and memory were measured on a local build with the `modules(...)` fix (not committed),
  because the shipped build cannot open its DB.
- Added `:extension:jsEngineSelfTest` to the self-test list; it exists and P9's parity table needs it.
Plan vs Kotlin code disagreements (the Kotlin code wins, per §1.2; fix PLAN.md before the phase named):
- Before P2: the schema version is **30**, not 29. SQLDelight names a migration after the version it
  upgrades *from*, so `1.sqm`..`29.sqm` take 1→30 and `Database.Schema.version` is 30 (see
  `MigrationSelfTest.kt`). So: create at `user_version` 30; for 1..29 run `N.sqm` from the
  current version through 29; read-only above 30. Fixtures: `kotlin-v30.db` plus a v29 one.
- Before P2: the Kotlin `AppSettings` writes `settings.json` from its data class and drops keys it
  does not know. Rust-only keys (`diskCacheMb`, `extHostBaseUrl`, theme) are lost whenever the
  Kotlin app saves a setting while both apps share the data folder. Either accept that until P9 or
  make the Kotlin save merge into the existing JSON (an additive Kotlin change; needs Mehdi's OK).
- Before P2: `AppPaths.kt` also honours the `tachimana.dataDir` system property and falls back to
  `~/.tachimana-base/TachiMana` when neither `LOCALAPPDATA` nor a home-based XDG path applies.
- Before P7: `BackupManga.categories` (tag 17) is `List<Long>`, which kotlinx.serialization writes
  unpacked. prost packs repeated scalars by default, so the Rust structs need `packed = "false"`
  to keep re-exports byte-identical.
- Before P4: `Tokens.kt` has four themes (Yokai Dark, Yokai Light, Midnight, Ink & Vermilion); the
  P4 screenshots and checklist 6.6 should cover all four.
Test results:
- `:desktop:packageDeb` → BUILD SUCCESSFUL (209.1 MB .deb)
- Kotlin self-tests: 10 pass, 2 blocked by network (browseSelfTest, extensionSpike); 0 app failures
