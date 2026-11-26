---
name: macos-linux-portability
description: Linux is the primary dev host and the only CI platform; macOS (Intel only) is verified by local builds; port plan in plan/macos-port-plan.md; new code must not add Linux-only APIs or paths
metadata:
  type: project
---

The user develops on two workstations. The **Linux** one has been the primary development host for this project, and CI runs only on Linux. The **macOS** workstation (Intel i9-9880H, Homebrew at `/usr/local`) is secondary. On 2026-11-26 the user stated explicitly that **the macOS port has NOT been completed**. The plan for porting the debugger and peripherals is `plan/macos-port-plan.md` (written 2026-11-26). It has six units, and Unit 1 is next. As of 2026-11-26, on that Mac:

- The SDL2 crates (`emma6k-display`, `emma6k-led-matrix`, `emma6k-lcd-display`) compile but fail to link with `ld: library 'SDL2' not found`. The cause is that `sdl2-sys` emits a bare `-lSDL2` with no search path. Enabling the `sdl2` crate's `use-pkgconfig` feature for macOS only fixes it (tried and reverted). `LIBRARY_PATH=/usr/local/lib` is the stopgap until that lands. Homebrew's `sdl2` is now `sdl2-compat`, which runs on SDL3.
- The debugger builds with four `unused variable: window_for_events` warnings (`terminal.rs`, `display.rs`, `led_matrix.rs`, `lcd_display.rs`). They're on main and only show up on macOS, so CI's debugger clippy command fails there.
- `emma65`, the debugger and the peripherals' tests pass.

**User decisions (2026-11-26):**
- **No macOS CI job**, because GitHub's macOS runners cost too much compared with Linux ones. macOS correctness depends on local builds until a cheaper option exists.
- **Linux CI cross-checks `emma65` for macOS.** The `ci` job runs `cargo clippy -p emma65 --all-targets --no-deps --target x86_64-apple-darwin -- -D warnings`. It doesn't cover the debugger (its `objc2` build script needs a macOS SDK) or the SDL2 peripherals, so those still need the local runs. On Linux, reproduce it with `rustup target add x86_64-apple-darwin` plus that command.
- **Keep the F-key run controls on macOS** (F5/F10/F11, Shift+F11) for now. The wiki documents the macOS system settings they need. Mac-specific alternatives can come later.
- **Target x86_64 only.** There's no Apple Silicon machine to build and test on. Don't claim arm64 support.
- **Distribution is a long way off.** No Apple Developer account, signing or notarization for now.

**Why:** PR #660 (2026-11-26) fixed two portability breaks that Linux CI never catches. `libc::pipe2` has no Apple binding in `libc`, and tests that spawned `/usr/bin/cat` failed because that path doesn't exist on macOS. The debugger's macOS-only warnings reached main the same way.

**How to apply:**
- Prefer std APIs (e.g. `std::io::pipe`) over Linux-only libc calls such as `pipe2`, `F_SETPIPE_SZ`, `epoll` or `eventfd`. If a platform-specific call is truly needed, gate it with `cfg` and provide a fallback. A variable used only inside a `cfg(target_os = "linux")` block must itself be declared under that `cfg`.
- In tests, use paths that exist on both systems, such as `/bin/cat` and `/bin/sh`.
- When working on the Mac, run the plan's §3.1 local verification commands (CI's Rust commands) for any platform-sensitive change. When a macOS build breaks, check recent `main` history before blaming the current change.
- Until plan Unit 1 lands, a failure in the SDL crates' link step or the debugger's clippy run on macOS is a known gap, not necessarily a regression. `cargo check --workspace --all-targets` (no linking) checks that the code still compiles.

Relates to [[ci-clippy-deny-warnings]].
