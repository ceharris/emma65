---
name: emma6k-rename
description: In-progress rename of the project to emma6k (branch chore/rename-emma6k) — what was renamed, what was deliberately kept, and the remaining steps that need the user
metadata:
  type: project
---

The project is being renamed to **emma6k** (decided by the user 2026-11-26) before the [[multi-cpu-effort]] refactor starts. The work is on branch `chore/rename-emma6k`.

**Naming rule (from the user):** any crate that isn't 6502-specific gets the `emma6k-` prefix. The apt/rpm package is `emma6k`; splitting it into 6502/6809 packages is a later decision.

**Renamed:**
- **Crates and binaries:** `emma6k-debugger` (lib target `emma6k_debugger_lib`, frontend package `emma6k-debugger-frontend`), `emma6k-display`, `emma6k-led-matrix`, `emma6k-lcd-display`, and `emma6k-build-info` (`emma6k_build_info`).
- **Debugger:** bundle identifier `org.emma6k.debugger`, "Emma6k …" window and page titles, dock theme `emma6k`.
- **Packaging:** deb/rpm package `emma6k`, docs under `/usr/share/doc/emma6k/`, `packaging/emma6k-debugger.desktop`, release title "emma6k X.Y.Z".
- **Docs:** README (badge, intro, links), mdbook (title, description, intro, contributor guide, URLs), CLAUDE.md, and the renamed crates' CHANGELOG headers ("named `emma65-*` through 0.1.0").
- **Plan docs:** the three `plan/*-external-protocol.md` specs.
- **Other:** `pyproject.toml`/`uv.lock` project name, the Dockerfile comment, the clock-bench scripts' messages.
- **Release skill:** new names, plus a fallback to the old `emma65-*-v` tag prefixes. The 0.1.0 tags `emma65-display-v0.1.0`, `emma65-led-matrix-v0.1.0`, `emma65-lcd-display-v0.1.0` and `emma65-debugger-v0.1.0` exist; without the fallback, each crate's next release would be wrongly treated as its first.

**Deliberately kept:**
- `emma65` (the 6502 crate and binary), `emma65-tracer`, and the `EMMA65_*` env vars.
- The `emma65-vX.Y.Z` tag as the trigger for the combined release build, because the package's version is the root `emma65` crate's version.
- `~/.emma/...` paths.
- The historical `plan/` documents.
- `.vscode/mcp.json` (contains a host-local path).

**Verified on macOS (2026-11-26):**
- `cargo check --workspace --all-targets` passes.
- `cargo test -p emma65 -p emma6k-debugger -p emma6k-build-info` passes.
- CI's clippy `-D warnings` passes for `emma65` and the three SDL crates.
- `cargo fmt --check` passes.
- **Not run there:** linking the SDL binaries (no SDL2 on that Mac), the debugger clippy step (existing macOS warnings, see [[macos-linux-portability]]), and the frontend tests (no `node_modules`). **On Linux, run the full `cargo build --workspace`, `cargo test --workspace`, every CI clippy command, and `npm test` in `debugger/frontend`.**

**Remaining steps:**
1. ~~The user renames the GitHub repo to `ceharris/emma6k`.~~ Done 2026-11-26.
2. The agent runs `git remote set-url origin git@github.com:ceharris/emma6k.git` on each host. Done on Linux 2026-11-26. **Still needed on macOS:** the user plans to rename that checkout's directory to `emma6k` and update its remote in a later session. Then run the tauri `cargo clean` described below, and point `.vscode/mcp.json` at the macOS path. That file is committed on purpose, with the Linux path.
3. Dispatch `docker-ci-image.yml` on main (confirm with the user first) so `ghcr.io/ceharris/emma6k-ci` exists. CI pulls `ghcr.io/${{ github.repository }}-ci`, so until then every CI job on the PR fails to find its image.
4. Open the PR. The branch was committed and pushed from the macOS workstation on 2026-11-26; open the PR only after steps 1–3.

**Verified on Linux (2026-11-26, from the renamed `~/git/emma6k` checkout):**
- `cargo fmt --check`, `cargo build --workspace`, `cargo test --workspace`, and every CI clippy `-D warnings` command pass.
- `npm test` and `npm run build` in `debugger/frontend` pass.
- `origin` is SSH, and `gh`, `libsdl2-dev` and `libsdl2-gfx-dev` are now installed.
- The Ubuntu 24.04 → 26.04 upgrade (around 2026-11-14) removed `libgtk-3-dev` and `libwebkit2gtk-4.1-dev`. The user reinstalled them. If the debugger fails to link with `unable to find library -lgdk-3`, those packages are missing again.

**Old GHCR images:** the CI image name follows the repo name. The `emma65-rust` → `emma65` rename (2026-09-07) left `ghcr.io/ceharris/emma65-rust-ci` orphaned, and this rename will orphan `ghcr.io/ceharris/emma65-ci`. Delete them only if the user asks. Links in `plan/` docs keep the old repo names on purpose ([[plan-frozen-doc-current]]); GitHub redirects renamed-repo URLs, but not Pages URLs.

**User-facing breakage to call out in the PR:**
- `pipe:` transports configured with `emma65-display`, `emma65-led-matrix` or `emma65-lcd-display` must use the new binary names.
- The docs site moves to `ceharris.github.io/emma6k`; GitHub likely won't redirect the old Pages URL.
- The debugger's OS-managed log directory follows the new identifier.

**Decided:** the future core crate is `emma6k-core`. **Local checkout directory:** on Linux it is now `~/git/emma6k` (renamed 2026-11-26, after the per-user memory was reconciled). The macOS checkout's directory name is unverified. Renaming a checkout has two consequences:
- **Claude Code's per-user memory path** is derived from the directory name, so it changes.
- **Cargo build-script outputs in `target/` keep the old absolute path.** Cargo doesn't re-run those build scripts after the move, so the debugger build fails with `failed to read plugin permissions: … <old path>/target/…`. Fix it with `cargo clean -p tauri -p tauri-plugin-clipboard-manager -p tauri-plugin-dialog -p tauri-plugin-fs -p tauri-plugin-log -p tauri-plugin-opener`, and repeat with `--release`.
