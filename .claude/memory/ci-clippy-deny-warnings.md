---
name: ci-clippy-deny-warnings
description: CI runs clippy with -D warnings, so any local clippy warning fails CI, even one already on main
metadata:
  type: feedback
---

Treat every clippy warning as a CI failure. CI runs `cargo clippy -p <crate> --all-targets --no-deps -- -D warnings` for each crate. The debugger job instead runs `cargo clippy --no-deps -- -D warnings` from `debugger/src-tauri`. Plain `cargo clippy` only warns, so warnings look harmless locally.

**Why:** in PR #660 (2026-11-26) three `needless_range_loop` warnings in `mc6840.rs` were dismissed as "not new" and pushed anyway, and CI failed. Those lints were already on main and had kept main red since 2026-11-22, probably because a newer stable clippy added the check. The rolling `stable` toolchain can add new lints at any time.

**How to apply:** before pushing, run CI's exact clippy commands from `.github/workflows/ci.yml`. Clippy doesn't link, so it runs even where the SDL crates can't link. Fix any warning or tell the user about it; don't dismiss it. When a PR's CI fails, check whether main fails too (`gh run list --branch main`). Relates to [[macos-linux-portability]].
