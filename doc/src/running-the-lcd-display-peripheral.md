# Running the LCD Display Peripheral

`emma65-lcd-display` is an SDL2 window that renders a `display/lcd` device's
composited dot-matrix output when running the plain `emma65` CLI standalone
(the debugger doesn't need it — its own LCD Display panel renders
in-process). Like `emma65-display` and `emma65-led-matrix`, it's spawned by
the emulator as a child process and streams data to it over the pipe
transport's stdin, per the
[LCD Display External Protocol](appendix-lcd-display-protocol.md).

Building it requires SDL2 development headers (`libsdl2-dev` on
Debian/Ubuntu, `sdl2` on Homebrew), the same as `emma65-display` and
`emma65-led-matrix`:

```bash
cargo build --release -p emma65-lcd-display
```

Configure a `display/lcd` device with a `pipe:` transport pointing at the
built binary:

```toml
[[devices]]
type = "display/lcd"
address = 0xD000
geometry = "16x2"
transport = "pipe:/path/to/target/release/emma65-lcd-display"
```

```
emma65 --device display/lcd@0xD000,geometry=16x2,transport=pipe:/path/to/target/release/emma65-lcd-display
```

The window opens as soon as the emulator attaches the transport, showing a
blank dot-matrix grid in the device's configured background color before
any writes occur. `--pitch` sets the initial on-screen dot center-to-center
spacing in pixels (default `12`); the window remains resizable afterward and
letterboxes/scales to fit. Closing the window ends `emma65-lcd-display`; it
also exits cleanly if the emulator process exits or is killed first, since
that closes its stdin.

Whether the window can ever show true 5×10 dots is fixed by the device's
configured `geometry=` — only `8-character-5x10` and `16-character-5x10`
have the physical common-line count a real 5×10 glyph needs (see
[LCD Display](io-devices.md#lcd-display-displaylcd)). On those two
geometries, `Function Set`'s `F` bit still switches the active font between
5×8 and 5×10 dots at runtime, and the window resizes on the fly to match,
since that changes every subsequent frame's pixel height — no configuration
is needed for the resize itself, it follows automatically from each frame
message's own dimensions. On every other geometry, a program setting `F=1`
has no visible effect: the font, and so the window's size, stays fixed at
5×8 for the life of the device.
