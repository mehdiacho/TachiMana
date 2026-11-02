# Rust port status
Updated: 2026-11-02 by Claude (implementing; the planned agent was unavailable)
Current phase: P2 Rust workspace, settings, database, branch feat/rust-p2-db
Done in this phase:
- `app/` Cargo workspace (lints: no unsafe, no unwrap/expect/panic outside tests).
- `tm-core`: AppPaths (same rules and sub-paths as AppPaths.kt), Settings (same keys and defaults as
  AppSettingsData, unknown keys preserved, atomic save, corrupt file kept aside), AppLock.
- `tm-db`: schema and named queries loaded straight from the Kotlin `.sq`/`.sqm` files; create,
  migrate (N.sqm upgrades N to N+1), newer-schema opens read-only; library, chapters, chapter sync,
  progress, history/recents, categories, extension repos, tracks.
Next step: P3 (archives, local importer, disk cache), then P1 once Gradle can resolve dependencies here.
Blockers / questions:
- Gradle in the cloud container gets HTTP 429 from Maven Central, so the Kotlin fixture DB (P2.4)
  and the ext-host build (P1) have to wait for a retry or run on CI.
Deviations from PLAN.md and why:
- Schema version is 30, not 29 (SQLDelight's version is migrations + 1). Plan corrected.
- SQL is embedded from the Kotlin sources rather than copied (one source of truth). Plan corrected.
- P1 and P2 are being done out of order because P1 needs Gradle.
Test results: `cargo test` in app/: tm-core 10/10, tm-db 13/13 (5 unit + 8 integration); clippy and fmt clean.
