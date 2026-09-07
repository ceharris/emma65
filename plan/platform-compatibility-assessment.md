# macOS and Windows Compatibility — Assessment

Status: assessment only. Nothing in this document has been implemented. Written 2026-09-07 against
`main` at b386b9f.

## 1. Purpose and scope

This document assesses what stands between emma65 (today: Linux-only in practice, and Linux-only
in CI, packaging, and documentation) and running on macOS and Windows. It covers the emulator
library and CLI (`emma65`, `emma65-tracer`), the three SDL2 peripheral binaries
(`emma65-display`, `emma65-led-matrix`, `emma65-lcd-display`), and the Tauri debugger
(`emma65-debugger`).

Sections 4 and 5 address macOS and Windows separately, because the two are not remotely
comparable in scope: macOS is one compile error away from building, while Windows requires
designing and writing a second implementation of the transport layer. Section 6 covers the work
that is shared between them, and section 7 proposes a sequencing.

## 2. Method

Findings below are separated into **verified** (reproduced in this repo) and **anticipated**
(reasoned from the code and platform behavior, not yet demonstrated). Verification was done by
adding the `aarch64-apple-darwin`, `x86_64-apple-darwin`, and `x86_64-pc-windows-msvc` targets to
the local rustup toolchain and running `cargo check --target ...` per workspace member. All
temporary edits made during the investigation have been reverted; the working tree is unchanged.

Cross-checking has one important limit: it type-checks against each target's `std` and platform
`libc` bindings, which is exactly what catches the class of problem this document is about, but it
does **not** link, does not run a build script that needs a platform SDK, and does not execute
anything. Native builds on real hosts remain necessary and are the subject of sections 4.4 and
5.5.

## 3. The platform-dependent surface

The most useful single result of this investigation: emma65's platform dependence is confined to
five files. Everything else — the CPU, bus, devices, disassembler, trace format, execution model,
watchpoint pipeline, configuration and templates, and every one of the peripheral binaries'
compositing paths — is portable Rust that type-checks unmodified against both Apple targets and
the Windows MSVC target.

| File | Platform dependency |
| --- | --- |
| `src/emulator/transport/internal_pipe.rs` | `libc::pipe2`, `libc::poll`/`pollfd`, `libc::fcntl`, `libc::dup`, `std::os::unix::io` raw fds |
| `src/emulator/transport/pty.rs` | `nix::pty::openpty`, `nix::sys::termios`, `nix::fcntl`, `tokio::io::unix::AsyncFd`, `std::os::unix::fs::symlink` |
| `src/emulator/transport/unix_socket.rs` | `tokio::net::UnixListener` / `tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf, UCred}` |
| `src/bin/emulator/tty.rs` | `libc::termios`, `tcgetattr`/`tcsetattr`/`cfmakeraw`, `AsRawFd` on stdin |
| `debugger/src-tauri/src/terminal.rs` | `tokio::io::unix::AsyncFd` over the console pipe's raw fd |

Two further platform assumptions are spread more thinly and are separately noted below:

- **`$HOME`** is read directly, with no fallback, in `src/emulator/config/path.rs` (`expand`,
  `portable_path`), `debugger/src-tauri/src/profile.rs` (`debugger_home_dir`),
  `debugger/src-tauri/src/recent.rs` (`display_label`), and `tests/emulator_binary.rs`.
- **`~/.emma/...` layout** is baked into the bundled config templates (`unix:~/.emma/sock/via6522`,
  `pty:~/.emma/dev/ttyS0`, `pty:~/.emma/dev/ttyS1` in `taliforth` and `msbasic`) and into the
  debugger's `~/.emma/debugger/{profiles,config}` tree.

Note that `transport` is an unconditional module of the `emma65` library, and the library is a
dependency of all three peripheral binaries and of the debugger. So a compile failure in
`unix_socket.rs` fails `emma65-tracer` too, even though the tracer never touches a transport. Any
Windows port should begin by making the Unix-only transports conditionally compiled, so that the
portable core can build independently of them (see §5.2).

There is also one piece of pre-existing Windows awareness in the tree:
`debugger/src-tauri/src/main.rs` already carries
`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, and the Linux-only GTK
workarounds (`GDK_BACKEND=x11`, the `gtk-menu-bar-accel` unbind, the resizable-toggle titlebar
repaint in `profile.rs` / `terminal.rs` / `display.rs` / `led_matrix.rs` / `lcd_display.rs`) are
all correctly `#[cfg(target_os = "linux")]`-gated already. The `gtk` crate dependency is
likewise gated to Linux in `debugger/src-tauri/Cargo.toml`. None of that will need untangling.

---

## 4. macOS

### 4.1 Build status — verified

`cargo check -p emma65 --all-targets --target aarch64-apple-darwin` produces **exactly one
error**:

```
src/emulator/transport/internal_pipe.rs:457:29: error[E0425]: cannot find function `pipe2` in crate `libc`
```

With that single call replaced (temporarily, for investigation) by `libc::pipe`, the entire
`emma65` library, both binaries, and the full test suite type-check clean for
`aarch64-apple-darwin` — no warnings, no second error behind the first. `openpty`, `cfmakeraw`,
`termios`, `poll`, `fcntl`, PTY symlinks, and `AsyncFd` are all available and correctly typed on
Darwin.

`cargo check -p emma65-display -p emma65-led-matrix -p emma65-lcd-display
--target aarch64-apple-darwin` also passes. This is a weaker result than it looks: `sdl2-sys`'s
build script emits link directives without probing for the library when cross-compiling, so this
confirms the Rust side compiles but says nothing about whether SDL2 will link (§4.3).

`cargo check -p emma65-debugger --target aarch64-apple-darwin` fails in the `objc2-exception-helper`
build script, which invokes `cc` with `-arch` and `-mmacosx-version-min=11.0` against the Linux
host compiler. **This is a cross-compilation toolchain limitation, not a project incompatibility** —
Cargo resolved Tauri's full macOS dependency graph (objc2, WKWebView path) without a resolution
error, and the failure is in compiling an Objective-C shim that needs a real macOS SDK. The
debugger cannot be usefully assessed further without a macOS host.

### 4.2 Known incompatibility: `libc::pipe2`

`os_pipe()` in `internal_pipe.rs:455` calls `libc::pipe2(fds, libc::O_CLOEXEC)`. `pipe2(2)` is a
Linux/Solaris/BSD extension that macOS does not provide; the `libc` crate correspondingly declares
it for `linux_like`, `freebsdlike`, `netbsdlike`, `solarish`, and several others, but not for
`unix/bsd/apple`. This is a hard compile error on every Apple target.

The obvious fix — `libc::pipe()` followed by `fcntl(fd, F_SETFD, FD_CLOEXEC)` on both ends — is
correct but not exactly equivalent, and the difference matters here. `pipe2` sets `O_CLOEXEC`
atomically with descriptor creation; the two-call fallback leaves a window in which a concurrent
`fork`/`exec` on another thread inherits the descriptors. emma65 does spawn child processes on
other threads — that is precisely what `PipeTransport::spawn` does for every SDL2 peripheral —
so this window is reachable in normal operation, not theoretical. A leaked pipe end into a
peripheral child would keep the pipe's write end open after the intended owner closed it, which
would defeat the EOF-based disconnect detection in `run_relay_thread`.

Recommended shape: keep `pipe2` on the platforms that have it and use the `pipe` + `fcntl`
fallback only on Apple, with a comment recording the race. Alternatively, adopt a crate that
already encapsulates this (`rustix::pipe::pipe_with`, or the `os_pipe` crate); given the codebase's
existing direct-`libc` idiom in this file, the `cfg` fork is probably the smaller change and the
one more in keeping with the surrounding code.

### 4.3 Anticipated issues — SDL2 peripherals

- **Library discovery.** The three peripheral crates depend on `sdl2 = "0.38"` with no
  `bundled`, `static-link`, or `use_mac_framework` feature. On macOS this means Homebrew's
  `sdl2` and `sdl2_gfx`, and on Apple Silicon the Homebrew prefix (`/opt/homebrew`) is not on the
  default linker search path — `LIBRARY_PATH=/opt/homebrew/lib` (or an equivalent
  `~/.cargo/config.toml` `rustflags` entry) is normally required. Worth evaluating whether
  enabling the `bundled` feature on macOS is preferable to documenting this: it compiles SDL2 from
  source, which trades build time for the elimination of an entire class of "it doesn't link"
  support questions. `SDL2_gfx` (needed by `emma65-led-matrix`) is not covered by `bundled` and
  would still need Homebrew.
- **Main-thread requirement.** macOS requires that the NSApplication event loop run on the main
  thread. All three peripherals already satisfy this — `sdl2::init()` and the event pump are in
  `main()`, and only the stdin frame reader is on a spawned thread — so no restructuring is
  expected. This should be confirmed by running, not assumed, since SDL2's main-thread enforcement
  on macOS is stricter than on X11.
- **HiDPI.** All three peripherals compute window geometry from pixel dimensions. On a Retina
  display SDL2 distinguishes window size (points) from drawable size (pixels) by a factor of 2.
  Expect the windows to be either half-size or blurry until `SDL_WINDOW_ALLOW_HIGHDPI` and
  `drawable_size()` are handled explicitly. This is a genuine visual-correctness issue for
  `emma65-lcd-display` and `emma65-led-matrix` in particular, whose whole point is precise dot
  geometry — see the several issues already closed on exactly that subject (#593, #595, #600, #603).

### 4.4 Anticipated issues — the debugger

- **Application menu structure.** `menu.rs` builds a `Menu` whose first submenu is File, and
  installs it with `app.set_menu()`. On macOS the first submenu of the application menu *is* the
  app menu — the bold, app-named one — so the current structure would render "File" in that slot
  and provide no About / Services / Hide / Quit items at all. macOS needs a leading app submenu
  that the other platforms do not want. This is a real structural difference, not a cosmetic one.
- **Quit.** The File > Exit item is deliberately a plain `MenuItem` with a `CmdOrCtrl+Q`
  accelerator rather than `PredefinedMenuItem::quit`, because muda's GTK backend silently drops
  the predefined Quit (the reasoning is recorded in `build_menu`'s comment). On macOS the
  predefined item is the one that behaves correctly, and Cmd+Q belongs in the app menu. Expect a
  `cfg`-conditional here.
- **Per-window menus.** The detached Terminal / Display / LED Matrix / LCD Display windows each
  call `window.remove_menu()`. Tauri documents this as unsupported on macOS (the menu is
  app-wide). It compiles and is a no-op, so this is a behavior note rather than a blocker: the
  global menu bar simply stays as-is when a detached window is focused, which is the correct macOS
  convention anyway.
- **Terminal copy/paste.** `TerminalPanel.tsx` binds Ctrl+Shift+C / Ctrl+Shift+V for terminal
  copy/paste — the correct Linux terminal convention, and wrong on macOS, where users expect
  Cmd+C / Cmd+V. The app-level bindings in `useAppKeyBindings.ts` are likewise `ctrlKey`-only
  (Ctrl+Shift+T/D/M/I). Only `App.tsx:44` and `NewProfileDialog.tsx:65` accept `metaKey`. The
  native menu accelerators use Tauri's `CmdOrCtrl` and are already correct; it is the
  JavaScript-level bindings that are not.
- **WKWebView vs WebKitGTK.** The frontend's CSS is unusually portable — the font tokens are
  `--font-mono: monospace` and `--font-ui: system-ui, sans-serif`, i.e. no hardcoded family names
  that could be missing on macOS. The main risks are in xterm.js glyph metrics under a different
  WebKit build and in dockview's drag-and-drop behavior. Both need visual UAT rather than analysis.
- **`--scale-factor`.** The debugger already carries a `--scale-factor` CLI override for terminal
  sizing (`TerminalScaleFactorOverride`), added because auto-detection was unreliable. On macOS the
  window scale factor is 2.0 on Retina; whether the existing auto-detection path handles that
  correctly is a UAT question.

### 4.5 Anticipated issues — configuration and paths

`$HOME` is set on macOS, so `path.rs`'s `expand`/`portable_path` and the debugger's profile tree
work unchanged. The `~/.emma/dev/ttyS0` and `~/.emma/dev/ttyS1` PTY symlinks in the `taliforth`
template also work — `openpty` yields `/dev/ttys00N` on macOS and the symlink points at it fine.
The `~/.emma/sock/via6522` Unix socket is likewise fine. macOS's 104-byte `sun_path` limit is
shorter than Linux's 108, but the paths in use here are far below both.

One genuine macOS difference worth checking during UAT: the default `taliforth` profile expects
external terminal programs to attach to those PTY symlinks. `screen /dev/ttys00N` works, but the
documentation in `doc/src/running-the-emulator.md` should be checked for Linux-specific tooling
advice.

### 4.6 Validation and testing approach

1. **CI first, as a build gate.** Add a `macos-latest` (Apple Silicon) job to `ci.yml` mirroring
   the existing `ci` job: `cargo fmt --check`, `cargo build -p emma65`, `cargo clippy`, `cargo test
   -p emma65`. This is the highest-value single step — the `emma65` library and CLI have a large
   automated test suite that will exercise the transports, the PTY code, and the config layer on a
   real macOS kernel, which is exactly the part of the codebase where the platform differences
   live. The GitHub-hosted macOS runners have SDL2 available via Homebrew, so the peripheral jobs
   can follow. Note that the existing Linux peripheral/debugger jobs run inside a prebuilt GHCR
   container image; macOS runners cannot use that, so those jobs need a Homebrew-based
   dependency-install step instead, which is a structural difference in the workflow rather than a
   parameter change.
2. **Targeted manual UAT** for the parts CI cannot cover: SDL2 window rendering at Retina scale for
   each of the three peripherals; the debugger's menu structure, keyboard shortcuts, terminal
   copy/paste, panel docking and detach/reattach; and a full run of each bundled starter template.
3. **Cross-check as a fast pre-flight.** `cargo check --target aarch64-apple-darwin` from a Linux
   dev box catches the type-level platform errors in seconds and needs no macOS hardware. It is
   worth keeping in the loop even after CI exists, as it is much faster than a CI round-trip. It
   cannot check the debugger (§4.1).
4. **Frontend tests are already platform-neutral** — vitest under jsdom — so they add no macOS
   signal and need not be duplicated on a macOS runner.

### 4.7 Packaging and distribution

Out of scope for initial compatibility testing but worth recording: `release.yml` builds a single
combined `.deb`/`.rpm` on Linux. macOS distribution means `cargo tauri build` producing a `.app`
and `.dmg`, plus Apple Developer ID code signing and notarization — without which Gatekeeper will
refuse to launch a downloaded build. Notarization requires an Apple Developer account
(paid, annual) and secrets in CI. The three SDL2 peripherals are plain CLI binaries and would
either ship inside the debugger's bundle or as a separate archive. A decision is also needed on
whether to ship universal binaries or separate Intel/Apple Silicon artifacts.

---

## 5. Windows

### 5.1 Build status — verified

`cargo check -p emma65 --target x86_64-pc-windows-msvc` produces **37 errors**, and every one of
them is in one of three files:

| File | Errors |
| --- | --- |
| `src/emulator/transport/internal_pipe.rs` | 23 |
| `src/emulator/transport/pty.rs` | 10 |
| `src/emulator/transport/unix_socket.rs` | 4 |

They are all of one kind: missing Unix primitives. `std::os::unix` and `std::os::fd` do not exist;
`libc` on Windows has no `poll`, `pollfd`, `nfds_t`, `POLLIN`/`POLLHUP`/`POLLERR`, `fcntl`,
`F_GETFL`/`F_SETFL`, `O_NONBLOCK`, or `pipe`-with-two-arguments; `nix` compiles to essentially
nothing on Windows, so `nix::pty`, `nix::fcntl`, `nix::sys`, `nix::unistd`, and `nix::libc` are all
unresolved; `tokio::io::unix` and `tokio::net::UnixListener` are absent.

No errors were reported outside those three files. The check stops at the library, so the binaries
and tests were not reached — `src/bin/emulator/tty.rs` will certainly fail next, since
`libc::termios`, `tcgetattr`, `tcsetattr`, and `cfmakeraw` are all Unix-only — but the shape of the
result is clear and matches the surface mapped in §3.

### 5.2 The core problem: three transports have no Windows analogue

This is the substance of the Windows port. Each of the three failing transports needs a decision,
and they are not the same decision.

**`InternalPipeTransport`** is the most important of the three: it is how the CLI console attaches
to the process's own stdin/stdout (`InternalPipeTransport::stdio`, used by
`src/bin/emulator/main.rs:49`), and how the debugger bridges its Terminal panel to the emulated
console (`pair()` + `into_split()`, consumed by `debugger/src-tauri/src/terminal.rs`). Nothing
works without it. Its design is built around a blocking `libc::poll()` on two file descriptors —
the real pipe and a self-pipe used to interrupt the wait — because, as the module documentation
explains, a blocking read has no other portable way to be interrupted. Windows has no `poll` over
pipe handles; the equivalent is `WaitForMultipleObjects` over a pipe handle and a manual-reset
event, or overlapped (asynchronous) I/O with an `OVERLAPPED` structure and a cancel event. Either
is a genuine reimplementation, not a shim. The self-pipe idiom maps cleanly onto a Win32 event
object, so the *architecture* survives; the syscalls do not.

**`PtyTransport`** has a Windows analogue in ConPTY (`CreatePseudoConsole`, Windows 10 1809+), but
it is a poor fit for what this transport does. `PtyTransport` exists to expose a device path
(`/dev/ttysNNN`) plus a stable symlink so that an *external, unrelated* program — a terminal
emulator the user launches separately — can attach to an emulated serial port. ConPTY has no
filesystem presence and no name; there is nothing for `screen` or `minicom` to open. The honest
options are (a) drop `pty:` on Windows and document `tcp:` as the substitute for the same use case
(external terminal attaches over a socket — this already works and is already supported), or
(b) implement a named-pipe transport (`\\.\pipe\emma65-ttyS0`) and map the `pty:` spec onto it,
which preserves the "external program attaches by a predictable name" property that is the point
of the feature. Option (b) is more work but keeps the bundled `taliforth` and `msbasic` templates
meaningful on Windows. Option (a) requires editing those templates per platform.

**`UnixSocketTransport`** is the easiest. Windows 10 1803+ has genuine `AF_UNIX` support, but
`tokio` does not expose `UnixListener` on Windows, so the tokio-based implementation cannot simply
be un-gated. The natural substitute is `tokio::net::windows::named_pipe`, mapping `unix:PATH` onto
a named pipe — the same mechanism option (b) above would use, so the two decisions should be made
together. The `UCred` peer-credential type used in `PeerInfo` has no equivalent and would need to
become platform-conditional (or be dropped; a quick check of whether anything actually consumes it
would settle that).

The recommended first move, regardless of which options are chosen, is to make these three modules
conditionally compiled: `#[cfg(unix)]` on the module declarations and re-exports in
`src/emulator/transport/mod.rs` and `src/emulator/mod.rs`, with the config layer
(`TransportSpec::to_transport`) returning a clear "not supported on this platform" error for the
corresponding spec strings. That alone gets the portable core — CPU, bus, devices, disassembler,
trace, watchpoints, config — building and testing on Windows, and lets `emma65-tracer` (which
never touches a transport) work immediately. It also converts the port from one large change into
a series of independently reviewable ones.

### 5.3 Other Windows-specific work

- **Console raw mode.** `src/bin/emulator/tty.rs` uses `termios`/`cfmakeraw` to put stdin into raw
  mode. The Windows equivalent is `SetConsoleMode` with `ENABLE_VIRTUAL_TERMINAL_INPUT` on the
  input handle and `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the output handle, and the
  save/restore/panic-hook structure of `RawModeGuard` carries over unchanged. Windows Terminal
  supports VT sequences well; `conhost.exe` on older Windows does not, which is worth knowing when
  interpreting UAT reports of garbled console output.
- **`$HOME` is not set on Windows.** `path.rs`'s `expand`, `portable_path`, `profile.rs`'s
  `debugger_home_dir`, `recent.rs`'s `display_label`, and `tests/emulator_binary.rs` all read it
  directly with no fallback, so `~/`-prefixed config paths would silently resolve as literal
  relative paths named `~`. Windows uses `USERPROFILE`, and the idiomatic answer is the `dirs` (or
  `directories`) crate rather than reading either variable directly. Note the `~/` shorthand is
  also *written* by `portable_path` into materialized profile configs, so this is a
  round-trip concern, not just a read concern. `.emma` as a directory name is fine on Windows,
  though a `%APPDATA%\emma65` location would be more conventional — that is a product decision.
- **Profile name validation.** `validate_profile_name` rejects `/`, `\`, and NUL. Windows
  additionally forbids `: * ? " < > |` and the reserved device names (`CON`, `PRN`, `AUX`, `NUL`,
  `COM1`–`COM9`, `LPT1`–`LPT9`), and its filesystem is case-insensitive, so `Default` and `default`
  would collide where they do not on Linux.
- **`pipe:` transport is already portable.** `PipeTransport` uses `tokio::process::Command` with
  piped stdin/stdout, which works on Windows unchanged. This is the mechanism by which all three
  SDL2 peripherals are spawned, so the peripheral architecture needs no redesign — only the
  peripherals themselves need to build and link.
- **SDL2 on Windows.** The `sdl2` crate on `windows-msvc` needs either the `bundled` feature
  (builds SDL2 from source via cmake), vcpkg with `use-vcpkg`, or hand-placed `SDL2.lib`/`SDL2.dll`.
  `bundled` is by far the least painful for CI, but again does not cover `SDL2_gfx`, which
  `emma65-led-matrix` requires. That crate specifically may need vcpkg regardless.
- **Tauri on Windows.** WebView2 is the runtime; it ships with Windows 11 and recent Windows 10,
  and Tauri's NSIS/MSI bundlers can embed a bootstrapper for older systems. Chromium-based WebView2
  differs from WebKitGTK considerably more than WKWebView does, so the frontend needs real UAT —
  particularly xterm.js metrics, dockview drag-and-drop, and the hand-styled form controls
  (`SelectPopover`, `ColorPickerPopover`) that exist specifically because native controls ignore the
  app theme.
- **Line endings.** Git's `core.autocrlf` on Windows checks out text files with CRLF by default.
  The `.lbl` VICE label files bundled in `src/emulator/config/templates/*/program.lbl` are
  `include_str!`'d and parsed; whether `load_vice_labels` tolerates a trailing `\r` should be
  checked, or a `.gitattributes` added to pin those files to LF. The `.bin` ROM images are already
  safe (`include_bytes!` on files Git treats as binary).
- **File replacement semantics.** Windows will not let an open file be deleted or renamed over.
  Anywhere the debugger rewrites a config, layout, or recent-profiles file while a session may
  still hold it open is a candidate for `Access is denied` errors that never occur on Linux.

### 5.4 What already works

Worth stating plainly, because it is a large fraction of the codebase: the CPU (both variants),
memory bus and address-region resolution, every I/O device including the VIA/PTM protocol codecs,
the display / LED matrix / LCD compositing paths, the disassembler, the binary trace format and
both its reader and writer, the execution and stepping model, the watchpoint scanner / parser /
compiler / evaluator, the entire configuration and template system, and `TcpSocketTransport` and
`PipeTransport` all type-check against `x86_64-pc-windows-msvc` today with no changes. The three
peripheral binaries' own source is likewise portable — they consume only pure compositing and font
code from the library. They fail to build on Windows solely because they link the `emma65`
library, which is why §5.2's conditional-compilation step unblocks so much at once.

### 5.5 Validation and testing approach

1. **Conditional-compile the Unix transports first, then add a `windows-latest` CI job** running
   `cargo build -p emma65` / `cargo test -p emma65`. Expect a meaningful number of tests to be
   `#[cfg(unix)]`-gated out along with the transports; the count of what still runs is itself a
   useful measure of how much of the port is done.
2. **Take the transports one at a time**, each with its own CI-green milestone: named-pipe
   substitute for `unix:`, then `InternalPipeTransport`, then the `pty:` decision. The console —
   and therefore every interactive use of the CLI, and the debugger's Terminal panel — is blocked
   until `InternalPipeTransport` lands, so it should be second, not last.
3. **Test the transport rewrites hard.** These are concurrent, shutdown-sensitive components whose
   existing Linux implementations already carry hard-won subtleties: the double-report-guarded
   disconnect edge in `run_relay_thread`, the interruptible park in `push_and_park`, the ring
   capacity sized to a worst-case synchronous burst rather than a single message. A Windows
   implementation that passes a naive test will still deadlock or drop frames under load. The
   existing transport test suites should be made to run against the Windows implementations rather
   than being gated out.
4. **Manual UAT** on the same footing as macOS: each bundled template, each peripheral, and the
   debugger's full panel/menu/profile surface.
5. **`cargo check --target x86_64-pc-windows-msvc` from Linux** is a cheap and effective progress
   meter throughout — it is what produced the 37-error breakdown above, in about a minute, with no
   Windows machine involved.

### 5.6 Packaging and distribution

Tauri's NSIS and MSI bundlers cover the debugger. The CLI and peripheral binaries have no
established Windows distribution path in this repo; options range from a plain `.zip` of the
`target/release` binaries to winget or Scoop manifests. Authenticode code signing needs a
certificate and CI secrets; without it, SmartScreen will warn on every download. `release.yml`
currently assumes a Linux container and `cargo deb` / `cargo generate-rpm`, so cross-platform
release would mean a build matrix with per-OS packaging steps and a publish job that collects
artifacts from all of them.

---

## 6. Shared considerations

- **CI restructuring.** `ci.yml` currently hardcodes `runs-on: ubuntu-latest` in six jobs, four of
  which run inside the `ghcr.io/ceharris/emma65-ci` container image that carries the SDL2 and
  WebKitGTK development packages. Neither macOS nor Windows runners can use that image. Making the
  workflow multi-platform therefore means restructuring, not parameterizing: a `strategy.matrix`
  over OS, with the container used only on Linux and per-OS dependency-install steps
  (Homebrew / vcpkg or `bundled`) elsewhere. It is also worth deciding deliberately which jobs run
  on which platforms — running the full peripheral and debugger matrix on all three OSes on every
  PR will be slow and, given the existing `dorny/paths-filter` gating, probably more than is
  wanted.
- **Documentation.** `doc/src/install.md` is entirely Linux-specific (apt and dnf blocks, an
  AppImage reference). It needs macOS and Windows sections. `doc/src/running-the-emulator.md` and
  the three `running-the-*-peripheral.md` pages use Unix paths and PTY-attach instructions
  throughout. Per the project's own convention, `doc/` must reflect current reality, so these
  cannot lag the port. This document lives in `plan/` and is frozen once superseded.
- **The `util/via_sr_peripheral.py` helper** uses `socket.AF_UNIX` and would need the same
  transport decision applied to it on Windows. Low priority — it is a manual testing aid.
- **A portability regression gate.** Once either platform builds, the cheapest way to keep it
  building is a CI step running `cargo check --target <triple>` for the *other* platforms from the
  Linux job. It catches a newly introduced `std::os::unix` import at PR time without adding a
  runner.

## 7. Suggested sequencing

macOS and Windows are so different in scope that they should not be treated as one project.

**macOS is a small, well-defined piece of work** and should go first: fix `pipe2`, add a
`macos-latest` CI job, then work through whatever the peripheral and debugger UAT turns up
(SDL2 linking, HiDPI, the app menu, the Cmd-key bindings). The verified result — one compile error
in the entire library and test suite — means the risk here is concentrated in the GUI layers, not
the emulator, and the automated test suite will start providing macOS signal almost immediately.

**Windows is a design project before it is an implementation project.** The transport-layer
decisions in §5.2 — particularly whether `pty:` becomes a named pipe or is dropped in favor of
`tcp:` — determine whether the bundled templates work unmodified, and should be settled before
code is written. The conditional-compilation step is worth doing early regardless of which way
those decisions go, since it is useful on its own and makes everything after it incremental.

A reasonable order:

1. macOS: `pipe2` fix + `macos-latest` CI for `emma65` (small, unblocks real signal)
2. macOS: peripherals and debugger UAT, fixing what it finds
3. Windows: decide the transport strategy (§5.2)
4. Windows: conditionally compile the Unix transports; `windows-latest` CI for the portable core
5. Windows: implement the replacement transports, one milestone each
6. Both: documentation, packaging, release-workflow matrix

## 8. Open questions

- Is the goal full parity on both platforms, or is one of them "CLI and tracer only" for now?
  Windows in particular is much cheaper if the debugger is deferred.
- Is macOS support targeting Apple Silicon only, or universal binaries?
- Is there an Apple Developer account available for notarization, and a certificate for
  Authenticode? Both affect whether distribution is feasible at all, independent of whether the
  code builds.
- For `pty:` on Windows: named-pipe substitute, or drop it and point users at `tcp:`?
- Is `UnixSocketTransport::peer_info`'s `UCred` return value actually consumed anywhere? If not,
  the Windows named-pipe substitute gets simpler.
