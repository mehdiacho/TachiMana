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
- **Next step:** see "Next step" in STATUS.md.
