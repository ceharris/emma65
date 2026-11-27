---
name: ci-path-filters-deliberate
description: CI's debugger and SDL jobs are deliberately path-filtered to their own directories, so src/- or core/-only changes don't build them; verify those crates locally instead
metadata:
  type: feedback
---

CI's debugger and SDL peripheral jobs are path-filtered to their own directories (`debugger/**` and so on), so they don't run for changes only under `src/`, `core/` or the root `Cargo.toml`, even though they depend on `emma65` and `emma6k-core`. This is deliberate for now (user, 2026-11-27). Don't propose adding the library paths to their filters.

**Why:** those jobs are long-running and expensive, and the user chose to leave them out of builds of the core emulator.

**How to apply:** for a PR that changes `src/`, `core/` or the root `Cargo.toml` in a way that could affect the debugger or SDL crates, run `cargo test --workspace` and the debugger's clippy command locally (see [[ci-clippy-deny-warnings]]). Say in the PR that you did, because CI won't check it.
