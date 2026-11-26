---
name: emma6k-rename
description: The project was renamed emma65 → emma6k (PR #661, merged 2026-11-26) — the naming rule, what deliberately kept the old name, and how each host moved its checkout
metadata:
  type: project
---

The project was renamed to **emma6k** in PR #661, merged 2026-11-26, before the [[multi-cpu-effort]] refactor. The GitHub repo is now `ceharris/emma6k`, and the CI image is `ghcr.io/ceharris/emma6k-ci`, which follows the repo name.

**Naming rule (from the user):** any crate that isn't 6502-specific gets the `emma6k-` prefix. The apt/rpm package is `emma6k`; splitting it into 6502/6809 packages is a later decision. The future core crate is `emma6k-core`.

**Deliberately kept the old name:**
- `emma65` (the 6502 crate and binary), `emma65-tracer`, and the `EMMA65_*` env vars.
- The `emma65-vX.Y.Z` tag as the trigger for the combined release build, because the package's version is the root `emma65` crate's version.
- `~/.emma/...` paths.
- The historical `plan/` documents, including their links to the old repo names ([[plan-frozen-doc-current]]).

**Old release tags:** the renamed crates' 0.1.0 tags still use the old prefixes, for example `emma65-display-v0.1.0` and `emma65-debugger-v0.1.0`. The release skill falls back to those prefixes. Without the fallback, each crate's next release would be wrongly treated as its first.

**Old GHCR images:** `ghcr.io/ceharris/emma65-rust-ci` and `ghcr.io/ceharris/emma65-ci` are orphaned. Delete them only if the user asks.

**Moving a checkout to a new directory** has two consequences:
- **Claude Code's per-user memory path** is derived from the directory name, so it changes. Before renaming, create the new per-user directory (for example `~/.claude/projects/-Users-ceharris-git-emma6k/memory/` on macOS) with a `MEMORY.md` that holds only a pointer to the repo's `.claude/memory/MEMORY.md`. Otherwise the first session in the new directory starts with an empty per-user memory, and the built-in memory instructions may save there. Past session transcripts stay in the old per-user directory and don't show up in `/resume` from the new path, unless you copy them over.
- **Cargo build-script outputs in `target/` keep the old absolute path.** Cargo doesn't re-run those build scripts after the move, so the debugger build fails with `failed to read plugin permissions: … <old path>/target/…`. Fix it with `cargo clean -p tauri -p tauri-plugin-clipboard-manager -p tauri-plugin-dialog -p tauri-plugin-fs -p tauri-plugin-log -p tauri-plugin-opener`, and repeat with `--release`.

**Linux workstation:** the checkout is at `~/git/emma6k`, and `origin` is `git@github.com:ceharris/emma6k.git`. The Ubuntu 24.04 → 26.04 upgrade (around 2026-11-14) removed `libgtk-3-dev` and `libwebkit2gtk-4.1-dev`, and the user reinstalled them. If the debugger fails to link with `unable to find library -lgdk-3`, they're missing again.

**macOS workstation (done 2026-11-26):** the checkout is at `~/git/emma6k`, and `origin` is `git@github.com:ceharris/emma6k.git`. The per-user pointer directory `~/.claude/projects/-Users-ceharris-git-emma6k/memory/` exists. The old per-user directory's transcripts were left behind on purpose (user). After the tauri `cargo clean`, a few `target/debug/build/*/output` files still name the old path; they date from 2026-09, belong to stale build hashes, and don't break the build.

**`.vscode/mcp.json`** uses `${workspaceFolder}` for CodeGraph's `--path`, so the one committed file works on both hosts. It used to hard-code the Linux path.
