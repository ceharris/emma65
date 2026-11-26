---
name: macos-port-effort
description: Resume point for the macOS port (plan/macos-port-plan.md, merged in #663) — Unit 1's exact changes, what was already verified, verification commands, and the PR shape
metadata:
  type: project
---

The macOS port of the debugger and the SDL2 peripherals follows `plan/macos-port-plan.md`, merged to `main` in #663 on 2026-11-26. The plan is frozen ([[plan-frozen-doc-current]]), so record progress here. It has six units, one PR each ([[multi-unit-plan-workflow]]). Constraints and decisions are in [[macos-linux-portability]]: no macOS CI, x86_64 only, no distribution, and keep the F-keys.

**Where we left off (2026-11-26):** `main` is at `ec12dd8` (the #663 merge). No branch or PR is open, and no Unit 1 code is written. The only untracked file in the macOS checkout is `debugger/src-tauri/gen/schemas/macOS-schema.json`, which Unit 1 commits. **Priority isn't set:** [[multi-cpu-effort]] step 1 PR 1 is also marked "next up". Ask the user which one comes first.

**Unit 1 must run on the macOS workstation.** Its whole point is the macOS build, and Linux can't verify it. Cut the branch from a freshly fetched `origin/main` ([[branch-from-origin-main]]), for example `fix/macos-build-clean`. The changes:

1. **SDL2 linking.** Add this to `display/Cargo.toml`, `led-matrix/Cargo.toml` and `lcd-display/Cargo.toml`:
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   sdl2 = { version = "0.38", features = ["use-pkgconfig"] }
   ```
   Cargo merges it with the existing unconditional `sdl2` dependency; `led-matrix` keeps its `features = ["gfx"]`. Tried on `display` only on 2026-11-26: it linked with no environment variables, and was then reverted. For `led-matrix`, check that `SDL2_gfx` links the same way; `pkg-config --libs sdl2 SDL2_gfx` already reports it. If `Cargo.lock` changes (for example, `pkg-config` added under `sdl2-sys`), commit it.
2. **Debugger clippy errors.** In `debugger/src-tauri/src/terminal.rs:274`, `display.rs:244`, `led_matrix.rs:243` and `lcd_display.rs:251` (line numbers as of `ec12dd8`), put `#[cfg(target_os = "linux")]` on the `let window_for_events = window.clone();` line. Its only use is inside the Linux-gated `WindowEvent::Focused(true)` match arm.
3. **Commit** `debugger/src-tauri/gen/schemas/macOS-schema.json`. `linux-schema.json` is already tracked.
4. **Docs.** Build prerequisites live in the wiki's `doc/src/install.md` "System libraries" section (Ubuntu and Fedora subsections), not in `for-contributors.md`, which the plan names. Add a macOS (Intel) subsection: `xcode-select --install`, the Command Line Tools that stand in for `build-essential`/`gcc`, then `brew install sdl2 sdl2_gfx pkgconf` (Homebrew's `sdl2` installs `sdl2-compat`). The debugger needs no extra packages on macOS, because WKWebView ships with the OS. The section's intro paragraph currently names "Tauri's Linux dependencies", so reword it for both platforms. Read the rest of `install.md` (the "Build and install" section, and how it covers the Tauri CLI and Node) and add macOS notes where the Linux steps don't apply; for example, the `.deb` install step. Note that the Tauri CLI isn't installed on the Mac (`cargo tauri` → "no such command"). Keep it user-facing ([[wiki-user-facing-writing]]). Also say that Apple Silicon is untested.

**Verify on the Mac,** with no `LIBRARY_PATH` set. These are the plan's §3.1 commands:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy -p emma65 --all-targets --no-deps -- -D warnings
cargo clippy -p emma6k-debugger --no-deps -- -D warnings
cargo clippy -p emma6k-display -p emma6k-led-matrix -p emma6k-lcd-display --all-targets --no-deps -- -D warnings
cargo fmt --all -- --check
```
Baseline from 2026-11-26: with `LIBRARY_PATH=/usr/local/lib`, the peripherals linked, their 33 tests passed, and their clippy run was clean. Debugger clippy failed only on the four errors in step 2. Linux is covered by the PR's CI: the change touches `display/`, `led-matrix/`, `lcd-display/` and `debugger/`, so all the gated jobs run.

**PR:** the PR body says the macOS local verification was done, and lists the commands. Manual UAT: on macOS, a fresh `cargo build --workspace` links all three peripherals with no environment setup; on Linux, the same build still works.

**After Unit 1 merges:** update [[macos-linux-portability]] to drop its "SDL2 doesn't link" and "debugger warnings" known-gap bullets, and record Unit 1 as done here. Unit 2 is next: peripheral UAT plus HiDPI, which needs the user at the Mac with a Retina display.
