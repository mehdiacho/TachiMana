# Instructions for coding agents

TachiMana is being ported from Kotlin (Compose Desktop) to a Rust + Tauri app with a TypeScript +
React UI, keeping a small Kotlin "extension host" for Tachiyomi/Mihon extensions.

Before doing anything:
1. Read `docs/rust-port/STATUS.md` (where the work stands and the next step).
2. Read `docs/rust-port/PLAN.md` in full. It is the instruction set: phases, gates, rules,
   interop invariants and technology choices. Follow it exactly; ask before deviating.
3. Update `docs/rust-port/STATUS.md` on every push.

Commits and PRs carry no AI attribution lines. Never push to `main`; one branch and PR per phase.
