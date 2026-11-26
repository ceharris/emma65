# macOS Port of the Debugger and Peripheral Binaries — Plan

Status: plan only. Nothing in this document has been implemented. Written 2026-11-26 against
`main` at `ea50e6b`, on the macOS workstation (Intel i9-9880H, macOS 26.7.1, Homebrew at
`/usr/local`, rustc 1.100.0).

## 1. Purpose and scope

This plan covers what is left to make the three SDL2 peripheral binaries (`emma6k-display`,
`emma6k-led-matrix`, `emma6k-lcd-display`) and the Tauri debugger (`emma6k-debugger`) build,
lint, test and behave correctly on macOS. The `emma65` library and CLI are only in scope where
the GUI crates depend on them.

It builds on `plan/platform-compatibility-assessment.md` (2026-09-07), whose §4 covered macOS.
That document is frozen. Where this plan's findings differ from it, this plan is the current
one. Since that assessment:

- **#660** fixed its one verified macOS blocker (`libc::pipe2` in `internal_pipe.rs`) by moving
  to `std::io::pipe`, and fixed test paths (`/usr/bin/cat`) that don't exist on macOS. The
  `emma65` library, CLI and tests now build and pass natively on macOS.
- **#661** renamed the project and the crates, so the assessment's crate names are out of date.

Windows is out of scope.

### 1.1 Constraints (user decisions, 2026-11-26)

- **No macOS CI job.** GitHub Actions' macOS runners cost too much compared with Linux ones.
  Until a cheaper option exists, macOS correctness depends on local builds on the macOS
  workstation (§3.1).
- **x86_64 only.** The only Mac available is Intel. Apple Silicon stays untested, and isn't
  claimed as supported, until there's a machine to build and test on.
- **No distribution.** Packaging, signing and notarization are a long way off. This round's goal
  is "builds from source and works" (§5).

## 2. Current state on macOS (verified natively, 2026-11-26)

Each item below was reproduced on the macOS workstation. "Anticipated" items in §4 were not.

| Area | Result |
| --- | --- |
| `cargo build -p emma65`, `cargo test -p emma65` | Pass. |
| `cargo build -p emma6k-debugger` | Builds, with 4 warnings. |
| `cargo clippy -p emma6k-debugger --no-deps -- -D warnings` (CI's command) | **Fails**: 4 × `unused variable: window_for_events`. |
| `cargo build -p emma6k-{display,led-matrix,lcd-display}` | **Fails to link**: `ld: library 'SDL2' not found`. |
| Same, with `LIBRARY_PATH=/usr/local/lib` | Builds and links. All 33 peripheral tests pass. CI's clippy commands pass. |
| Same, with the `sdl2` crate's `use-pkgconfig` feature enabled for macOS only | Builds and links with no environment setup. |
| `cargo tauri dev` | Not tried: `tauri-cli` isn't installed on this host. |

### 2.1 Why SDL2 doesn't link

Homebrew's `sdl2-compat` and `sdl2_gfx` are installed, and `libSDL2.dylib` and
`libSDL2_gfx.dylib` are in `/usr/local/lib`. The peripheral crates depend on `sdl2 = "0.38"` with
no linking feature, so `sdl2-sys` emits a bare `-lSDL2` with no `-L` search path. Apple's linker
doesn't search `/usr/local/lib` (or `/opt/homebrew/lib` on Apple Silicon) by default.
`pkg-config --libs sdl2 SDL2_gfx` already reports the right flags.

Two details matter for later work:

- Homebrew's `sdl2` formula is now **`sdl2-compat`**, which is SDL2's API implemented on top of
  SDL3 (3.4.16 here). Windowing and HiDPI behavior come from SDL3, not from a real SDL2. Keep this
  in mind when reading SDL2 documentation or bug reports during UAT.
- `use-pkgconfig` gets the prefix from Homebrew's own `.pc` files instead of a hard-coded path.
  That's why it beats putting `-L /usr/local/lib` in `.cargo/config.toml`. It should also cover
  `/opt/homebrew` if Apple Silicon is ever added, but that's untested (§1.1).

### 2.2 Why the debugger fails clippy

`terminal.rs:274`, `display.rs:244`, `led_matrix.rs:243` and `lcd_display.rs:251` each clone the
window into `window_for_events` unconditionally. The only use is inside a
`#[cfg(target_os = "linux")]` match arm, the GTK titlebar-repaint workaround. On every other
platform the clone is unused.

## 3. Units of work

Each unit is one branch and one PR, and each must compile and pass CI on its own. Unit 1 is
small and mechanical. Units 2–5 depend on GUI UAT on the Mac, which is the user's to run.

### 3.1 Local verification instead of macOS CI

Linux CI can't see macOS-only breakage: the four clippy errors in §2.2 reached `main` exactly
that way. With no macOS runner (§1.1), every PR in this plan, and any later PR that touches
platform-sensitive code (transports, the `cfg(target_os)` blocks in the debugger, the
peripherals' windowing, `menu.rs`, keyboard bindings), is verified on the macOS workstation
before it's opened or merged:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy -p emma65 --all-targets --no-deps -- -D warnings
cargo clippy -p emma6k-debugger --no-deps -- -D warnings
cargo clippy -p emma6k-display -p emma6k-led-matrix -p emma6k-lcd-display --all-targets --no-deps -- -D warnings
```

These mirror the Rust commands in `ci.yml`. If `ci.yml` changes, update this list to match. Each
PR's description states whether the macOS run was done, the same way it lists UAT steps.

**Linux-runner cross-check (added with this plan, user decision 2026-11-26).** The Linux `ci`
job runs `cargo clippy -p emma65 --all-targets --no-deps --target x86_64-apple-darwin -- -D
warnings`. That catches Linux-only API use, such as #660's `pipe2`, and warnings that appear only
on macOS. It ran clean in about 25 seconds in a Linux `rust:latest` container (rustc 1.100.0)
before it was added. It covers only `emma65`. The debugger's `objc2` build script needs a macOS
SDK. The peripherals probably can't be covered either: with `use-pkgconfig`, the `sdl2-sys` build
script runs `pkg-config` for the cross target, which `pkg-config` refuses by default. So the
local runs above are still required for the GUI crates.

### Unit 1 — The workspace builds and passes clippy on macOS

- In each of `display/`, `led-matrix/` and `lcd-display/Cargo.toml`, add the `sdl2` dependency's
  `use-pkgconfig` feature for macOS only. Cargo merges the features with the existing
  unconditional dependency:
  ```toml
  [target.'cfg(target_os = "macos")'.dependencies]
  sdl2 = { version = "0.38", features = ["use-pkgconfig"] }
  ```
  Linux keeps its current linking behavior, and the CI container image is unaffected.
- In the four debugger files from §2.2, move the clone under the same `cfg` as its only use:
  `#[cfg(target_os = "linux")] let window_for_events = window.clone();`.
- Commit `debugger/src-tauri/gen/schemas/macOS-schema.json`, which Tauri generates on a macOS
  build. `linux-schema.json` is already committed, and committing the macOS one keeps macOS
  checkouts clean.
- Docs: add macOS prerequisites to the wiki's contributor build instructions:
  `brew install sdl2 sdl2_gfx pkgconf`, Node, and `cargo install tauri-cli --version "^2"`.

**Verify on macOS:** the §3.1 commands, with no environment variables set. **On Linux:** the
same, to confirm nothing changed there.

### Unit 2 — Peripheral UAT and HiDPI on a Retina display

The peripherals create their windows without `allow_highdpi()`, and render through
`set_logical_size` at the frame's native pixel size. On a Retina display (scale factor 2) the
expected result is a window of the right size in points, with dots drawn at half the available
resolution and upscaled by the compositor. That would look soft for `emma6k-lcd-display` and
`emma6k-led-matrix`, whose dot geometry was tuned carefully on Linux (#593, #595, #600, #603).
Because of sdl2-compat (§2.1), confirm this by running each peripheral before changing anything.

1. **UAT (user):** run each peripheral from the CLI with a `pipe:` transport, at `--scale` 1 and
   above. Check that each window opens, renders, resizes and takes keyboard input; check
   sharpness on the Retina panel; check behavior when dragged to a non-Retina external display,
   if one is available.
2. **Anticipated fix:** add `.allow_highdpi()` to each window builder, and keep
   `set_logical_size` so the renderer fills the full-resolution drawable. `emma6k-led-matrix`
   draws its LEDs with SDL2_gfx primitives at logical coordinates, so they may need to be drawn
   at drawable resolution to stay round and sharp. Leave the Linux output unchanged at scale
   factor 1.
3. Confirm that SDL2 initialization and the event pump stay on the main thread, as macOS
   requires. The assessment says they already do. Confirm it by running, without restructuring
   anything.

### Unit 3 — Debugger menu bar follows macOS conventions

`menu.rs` builds File / Edit / View / Run / Memory / Assembler / Window / Help. On macOS the
first submenu is the application menu, so "File" ends up in the app-name slot, and there's no
About / Hide / Quit. Changes, all `cfg(target_os = "macos")`:

- Add a leading app submenu: About emma6k Debugger (the existing custom About item, #423),
  separator, Services, separator, Hide / Hide Others / Show All (predefined items, which work on
  macOS), separator, and Quit with `Cmd+Q`. Quit stays a custom `MenuItem` that dispatches the
  existing `request_exit`, so the exit-confirmation dialog still runs; `PredefinedMenuItem::quit`
  would skip it.
- Remove Exit from File and About from Help on macOS.
- **Edit menu: check first, then fix.** In WKWebView, the standard text-editing shortcuts
  (`Cmd+C/V/X/A/Z`) reach a focused `<input>` only through key equivalents in the app's menu.
  This menu's Cut / Copy / Paste deliberately have no accelerators (on Linux, `Ctrl+C` would
  steal the terminal's interrupt key, #435), and it has no Select All or Undo. So these
  shortcuts probably do nothing in the debugger's text fields on macOS today. The #435 conflict
  doesn't exist on macOS, because `Cmd` isn't `Ctrl`. Once UAT confirms the problem:
  - On macOS, give Cut / Copy / Paste their `Cmd` accelerators, still dispatched through
    `EditMenuContext`.
  - Add predefined Undo, Redo and Select All.
  - Check that `Cmd+C` with a selection in the Terminal panel copies the selection, and that
    `Ctrl+C` still sends an interrupt.
- Window menu: optionally add the predefined Minimize and Zoom items that macOS users expect.
- **Detached windows:** `remove_menu()` is a no-op on macOS, so the app menu stays active while
  a detached Terminal / Display / LED Matrix / LCD window has focus. #351 made menu accelerators
  main-window-only; on macOS they become app-wide. Check during UAT that `F5` and the Edit items
  pressed in a detached window either act on the right target or are harmless.

### Unit 4 — Frontend key bindings use Cmd on macOS

- Add one platform-modifier helper: `metaKey` on macOS, `ctrlKey` elsewhere. Use it in
  `useAppKeyBindings.ts` (`Ctrl+Shift+T/D/M/I`). `App.tsx` and `NewProfileDialog.tsx` already
  accept either key and can stay as they are.
- `TerminalPanel.tsx`: on macOS, copy and paste with `Cmd+C` / `Cmd+V`. Keep `Ctrl+C` going to
  the emulated machine. This needs to be coordinated with Unit 3's Edit-menu accelerators so the
  two don't both handle the same keystroke.
- Tooltips and labels that name a shortcut ("Auto-Step (Ctrl+Shift+F5)") should show the macOS
  form on macOS.
- Function keys stay as they are for now (user decision, 2026-11-26): Run uses `F5`, Step Over
  `F10`, Step Into `F11`, and Step Return `Shift+F11`. On a Mac keyboard, F-keys need `Fn` unless
  the user changes a system setting, and `F11` is bound to Show Desktop by default. Unit 6
  documents both system settings. Mac-specific alternatives can be added later.
- Extend the vitest coverage for the helper and the bindings. jsdom lets the tests stub
  `navigator.platform`, so these tests run on Linux CI too.

### Unit 5 — Full debugger UAT on macOS, with fixes as separate PRs

Run this after Units 1, 3 and 4. It's for the user to run, and each defect gets its own PR.

- Startup: the default profile is created under `~/.emma/debugger/`, window geometry restores
  (#419), and titles update when the profile changes.
- Each bundled template (taliforth, msbasic, ehbasic, rain, lcd, snake): run, stop, step and
  breakpoints.
- WKWebView rendering: xterm.js glyph metrics and terminal sizing at scale factor 2, including
  whether `--scale-factor` is ever needed on macOS. Also dockview docking, drag-and-drop and
  detach/reattach for every detachable panel, the hand-styled popovers (`SelectPopover`,
  `ColorPickerPopover`), the light and dark themes, and the canvas sharpness of the in-process
  LED matrix and LCD panels (which use `devicePixelRatio`).
- PTY serial ports: `~/.emma/dev/ttyS0` and `ttyS1` symlink to `/dev/ttys00N`. Attach with
  `screen` and check that the user docs work as written on macOS.
- File dialogs and the clipboard plugin: load memory from a file, and the Assembler's
  open/save.

### Unit 6 — Documentation

The wiki has to reflect current reality as each unit lands. The doc changes ship with the units
they describe; this list exists so none get missed.

- `install.md`: a "macOS (Intel, build from source)" section (§1.1, §5).
- The contributor guide: macOS prerequisites (Unit 1), and the §3.1 rule that platform-sensitive
  changes are verified locally on macOS because CI doesn't cover it.
- `running-the-emulator.md` and the three `running-the-*-peripheral.md` pages: macOS device
  paths (`/dev/ttys00N`), `screen` usage, and Retina notes if Unit 2 leaves any.
- The debugger page: macOS shortcuts (Units 3–4), and the two macOS system settings the F-key
  run controls need: "Use F1, F2, etc. keys as standard function keys", and turning off or
  rebinding Show Desktop's `F11` shortcut.
- Update the `macos-linux-portability` agent memory as each unit lands, since it currently says
  the port is incomplete.

## 4. Anticipated risks (not yet observed)

- **sdl2-compat differences.** Running SDL2 on top of SDL3 may change event or HiDPI behavior in
  ways that don't match SDL2 documentation. If a peripheral bug can't be explained, test it
  against a real SDL2 build (the `sdl2` crate's `bundled` feature) to rule this out.
- **Silent macOS regressions between local runs.** Without macOS CI, a Linux-only change can sit
  on `main` until the next time someone builds on the Mac, as §2.2's warnings did. §3.1 limits
  this but can't eliminate it. When a macOS build breaks, check recent `main` history before
  assuming the change you're working on caused it.
- **Apple Silicon is untested.** Nothing in this plan is verified on arm64 (§1.1). Code should
  stay architecture-neutral (no x86-specific assumptions) so the eventual arm64 port is only a
  build-and-test exercise, but don't claim arm64 support in the docs.

## 5. Packaging and distribution (out of scope)

Distribution is a long way off (§1.1), so this round is source builds only and needs no Apple
Developer account. For the record, when it comes up: `cargo tauri build` can already produce a
`.app`/`.dmg` (bundle `targets` is `"all"` and `icon.icns` exists). Gatekeeper requires signing
and notarization for downloaded builds. The CLI and peripherals depend on Homebrew's SDL2 at
runtime, which makes a Homebrew tap formula a natural fit. `release.yml` is Linux-only today.

## 6. Decisions

Decided by the user on 2026-11-26:

- No macOS CI job, x86_64 only, and no distribution (§1.1).
- Add the Linux-runner cross-check for `emma65` (§3.1). It ships in the same PR as this plan.
- Keep the F-key run controls on macOS for now (Unit 4).

Nothing is open at the moment. Later questions (Apple Silicon, distribution, Mac-specific
run-control keys) wait until the constraints in §1.1 change.

## 7. Suggested order

1. Unit 1 first: it makes the macOS build green, which §3.1's local runs need.
2. Unit 2 (peripheral UAT and HiDPI) can run in parallel with Unit 3 (menu bar); they touch
   different crates.
3. Unit 4 after Unit 3, because the Edit-menu accelerators and the terminal copy/paste bindings
   have to agree.
4. Unit 5 once Units 1, 3 and 4 are merged; it produces its own follow-up PRs.
5. Unit 6 continuously, with the docs shipping alongside each unit.
