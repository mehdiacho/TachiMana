# Handoff

Read this first if you are picking up TachiMana work in a fresh session.

- **What is going on:** the desktop app is being ported from Kotlin/Compose to Rust + Tauri 2 +
  React/TypeScript, keeping a Kotlin extension host as a sidecar. Plan: `docs/rust-port/PLAN.md`
  (phases P0 to P9). Where it stands, what is not done and how each phase was checked:
  `docs/rust-port/STATUS.md` (first line = current phase).
- **Branches and PRs:** one branch per phase (`feat/rust-p<N>-...`), PR into `main`; later phases
  are stacked on the previous branch until it merges. Never push to `main`.
- **Rules from Mehdi:** commits are authored as Mehdi with no AI trailers; deleting the Kotlin app
  (P9) needs his OK; do not state guesses as facts.
- **Building the Kotlin app in a cloud container:** use a Gradle init script that maps Maven Central
  to `https://maven-central.storage-download.googleapis.com/maven2/` and `github.com/<x>/raw/` to
  `raw.githubusercontent.com/<x>/` (see STATUS.md, Blockers).
- **Releases of the Rust app:** tag `app-v<version>` runs `.github/workflows/app-release.yml`
  (installers plus the extension helper zips). `v*` tags are still the Kotlin app's.
- **Running the app in a cloud container:** Xvfb on :99, `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
  `TACHIMANA_DATA_DIR` for a throwaway data folder; `TM_EXTHOST_DIR` points at an unpacked helper.
- **Data folder:** the Kotlin and Rust apps share `%LOCALAPPDATA%\TachiMana` on Windows (WebView
  data is in `%LOCALAPPDATA%\tech.mehdiacho.tachimana`).
- **Backups:** Mihon-compatible `.tachibk`; tags 900 (installed extensions, chapter downloaded)
  are ours and shared with Mehdi's Mihon fork: `docs/rust-port/backup-schema.md`. Do not renumber.
- **Logs and crash reports:** `<data>/logs/` (`tachimana.log`, `crash-*.txt`); ask Mehdi for these
  first when he reports a crash.
- **Mehdi's real backup** may be in a session scratchpad for testing. It is personal data: never
  commit it or anything derived from it.
- **Next step:** see "Next step" in STATUS.md.

## Planning in progress (2026-11-14)

Decided: D1 two-way by default, D2 a follower works normally but its changes are not sent, D3 the
existing remote storage (WebDAV) holds the shared state and direct talk stays on the same network,
D4 reading beside the library is a toggle, off by default (built in step B). Step B is the
library/manga-page work in STATUS.md; steps C to H wait on the phone side (mihon#13, #14).

Before more sync work, read `docs/plan/system-plan.md` (draft) and
`docs/protocol/README.md` (the TachiMana Link Protocol and the form for phone-app work orders).
