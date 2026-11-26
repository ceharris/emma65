# Project memory index

One line per memory file in this directory. See CLAUDE.md's "Agent memory" section for how to use and maintain it.

- [emma6k rename (in progress)](emma6k-rename.md) — branch chore/rename-emma6k: what's renamed or kept, remaining steps (GitHub repo rename, remote, CI image, PR)
- [Multi-CPU effort](multi-cpu-effort.md) — 6809 support via a `Processor` trait in this repo; step 1 plan in plan/; user decisions (core crate: emma6k-core)
- [Clock bench baselines](clock-bench-baseline.md) — per-host baselines (macOS i9: default ~26/28 MHz, minimal ~50/55 MHz); no Linux baseline yet
- [macOS + Linux portability](macos-linux-portability.md) — Linux is primary; macOS port NOT complete (SDL2 doesn't link); no Linux-only APIs or paths
- [CI clippy denies warnings](ci-clippy-deny-warnings.md) — any clippy warning fails CI; run CI's exact `-D warnings` commands before pushing
- [Git uses SSH](git-push-via-ssh.md) — origin is SSH; push normally, no HTTPS credential workarounds
