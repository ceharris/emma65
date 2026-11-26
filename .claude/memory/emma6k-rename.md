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
1. The user renames the GitHub repo to `ceharris/emma6k`.
2. The agent runs `git remote set-url origin git@github.com:ceharris/emma6k.git` on each host.
3. Dispatch `docker-ci-image.yml` on main (confirm with the user first) so `ghcr.io/ceharris/emma6k-ci` exists. CI pulls `ghcr.io/${{ github.repository }}-ci`, so until then every CI job on the PR fails to find its image.
4. Open the PR. The branch was committed and pushed from the macOS workstation on 2026-11-26; open the PR only after steps 1–3.

**User-facing breakage to call out in the PR:**
- `pipe:` transports configured with `emma65-display`, `emma65-led-matrix` or `emma65-lcd-display` must use the new binary names.
- The docs site moves to `ceharris.github.io/emma6k`; GitHub likely won't redirect the old Pages URL.
- The debugger's OS-managed log directory follows the new identifier.

**Decided:** the future core crate is `emma6k-core`. **Open question for the user:** whether to rename the local checkout directory (`~/git/emma65`) on each host.
