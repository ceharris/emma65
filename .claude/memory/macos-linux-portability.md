---
name: macos-linux-portability
description: Linux is the primary dev host and CI platform; the macOS port is incomplete, but new code must not add Linux-only APIs or paths
metadata:
  type: project
---

The user develops on two workstations. The **Linux** one has been the primary development host for this project, and CI also runs on Linux. The **macOS** workstation (Intel i9-9880H) is secondary. On 2026-11-26 the user stated explicitly that **the macOS port has NOT been completed**. On that Mac:

- The SDL2 crates (`emma6k-display`, `emma6k-led-matrix`, `emma6k-lcd-display`) compile but fail to link with `ld: library 'SDL2' not found`.
- The debugger builds with four `unused variable: window_for_events` warnings (`debugger/src-tauri/src/display.rs:244`). These are on main and are a macOS-only gap, so CI's debugger clippy step would fail there.
- `emma65` and the debugger tests pass.

**Why:** PR #660 (2026-11-26) fixed two portability breaks that Linux CI never catches. `libc::pipe2` has no Apple binding in `libc`, and tests that spawned `/usr/bin/cat` failed because that path doesn't exist on macOS.

**How to apply:**
- Prefer std APIs (e.g. `std::io::pipe`) over Linux-only libc calls such as `pipe2`, `F_SETPIPE_SZ`, `epoll` or `eventfd`. If a platform-specific call is truly needed, gate it with `cfg` and provide a fallback.
- In tests, use paths that exist on both systems, such as `/bin/cat` and `/bin/sh`.
- On macOS, a build failure in the SDL crates or the debugger's macOS warnings is a known gap, not necessarily a regression. To check that code still compiles there, use `cargo check --workspace --all-targets`, which doesn't link.

Relates to [[ci-clippy-deny-warnings]].
