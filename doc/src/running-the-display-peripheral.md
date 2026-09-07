# Running the Display Peripheral

`emma65-display` is an SDL2 window that renders a `display` device's
composited output when running the plain `emma65` CLI standalone (the
debugger doesn't need it — its own Display panel renders in-process). It's
not run directly against a live emulator process; instead, the emulator
spawns it as a child and streams frame data to it over the pipe transport's
stdin, per the [Character Display External Protocol](appendix-display-protocol.md).

Building it requires SDL2 development headers (`libsdl2-dev` on
Debian/Ubuntu, `sdl2` on Homebrew), the same way building the debugger
requires `gtk` on Linux:

```bash
cargo build --release -p emma65-display
```

Configure a `display` device with a `pipe:` transport pointing at the
built binary:

```toml
[[devices]]
type = "display"
address = 0xF000
transport = "pipe:/path/to/target/release/emma65-display"
```

```
emma65 --device display@0xF000,transport=pipe:/path/to/target/release/emma65-display
```

The window opens as soon as the emulator attaches the transport (immediately
on startup for a TOML/CLI-configured device), sized to the device's
configured grid at an initial `--scale` (default `3`, an integer multiple of
the native `columns*8` by `rows*8` pixel size); it remains resizable
afterward and letterboxes/scales to fit. Closing the window ends
`emma65-display`; it also exits cleanly if the emulator process exits or is
killed first, since that closes its stdin.

## Keyboard input

`emma65-display` also feeds keyboard input back to the emulated program — it
captures key presses from its own SDL2 window and sends them over the same
pipe (its stdout), the same way the debugger's Display panel supplies live
input in-process. This only does anything if the `display` device is
configured with a `keyboard-address=` range (see
[Character Display](io-devices.md#character-display-display)):

```toml
[[devices]]
type = "display"
address = 0xF000
transport = "pipe:/path/to/target/release/emma65-display"
keyboard-address = 0xF800
break = 0x03
```

With no keyboard range configured, keystrokes are simply not captured or
sent — the emulator would discard them anyway.

The `emma65-display` window must have keyboard focus (click it, the same as
any other window) to capture key presses; SDL2 doesn't deliver events to an
unfocused window. What's captured: ordinary printable characters (sent as
their ASCII code); `Enter`, `Backspace`, `Tab`, and `Escape` (as the
standard ASCII control codes); and `Ctrl+<letter>` (as `0x01`–`0x1A`).
Anything else — modifier keys on their own, function keys, non-ASCII input
from an IME — is not sent. See the
[inbound keystroke stream](appendix-display-protocol.md#inbound-keystroke-stream-peripheral--device)
for the exact byte-level encoding.
