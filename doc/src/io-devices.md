# I/O Devices

Eleven built-in devices implement the `IoDevice` trait. Eight — Console,
R6551, Mc6850, Via6522, Mc6840, LedMatrix, CharDisplay, and Lfsr16 — are
register-window devices that can be mapped into any address range on the
bus; each integrates with the interrupt controller via `irq_active()` and
`take_nmi()` (CharDisplay is the one exception — see below), and most of
them exchange data with the outside world over a configurable
[Transport](#transport-options). The other three — Finch, Phoebe, and Vireo —
are complete bank-switched memory subsystems that occupy the entire 64 KB
address space in place of separate RAM/ROM regions; see
[Bank-Switched Memory Modules](#bank-switched-memory-modules) below.

Every device is placed on the bus with a TOML `[[devices]]` table — `type`
selects the device (shown in parentheses in each heading below) and
`address` is where it's mapped — or the equivalent
`--device type@address,key=value,...` CLI flag; see
[Running the Emulator](running-the-emulator.md) for the general TOML/CLI/env
conventions shared by every device type.

Most IRQ-capable devices below also accept an `irq` attribute. It isn't a
vectored interrupt number a 6502 program can read anywhere — it's just a
bookkeeping slot (0–63) the emulator uses at startup to make sure two
IRQ-capable devices don't collide on the shared interrupt line. Every device
type ships with its own default slot, so you only need to set `irq=`
yourself if you configure enough IRQ-capable devices that two collide (the
emulator refuses to start and tells you so). Whatever the slot number, a
6502 program still identifies *which* device is interrupting the same way
it would on real hardware: by polling each device's own status register,
since the 6502 has only one hardware IRQ line.

## Console (`console`)

A simple polling console device for byte-stream I/O over a configurable
[Transport](#transport-options). It occupies 2 bytes of address space:

| Offset | Register | Read | Write |
|--------|----------|------|-------|
| 0 | Data   | Returns the latch's value if non-zero, else the next buffered input byte, else `0`; either way, clears the latch and interrupt status | Sends the byte to the transport (no-op if unconnected) |
| 1 | Latch  | If the latch is currently zero, pulls the next buffered input byte into it (a one-byte lookahead); returns the latch; clears interrupt status | Overwrites the latch and drains the input buffer; if the value matches the configured break key, raises IRQ instead of clearing it |

- Input is buffered in a 64 kilobyte ring buffer as it arrives from the
  transport, so bytes aren't lost between polls.
- An optional break key (e.g. ASCII Ctrl+C) can be configured: when that
  byte is seen in the input, the buffer is drained, the break key value is
  latched, and the CPU's IRQ signal is asserted — a "stop the program" key
  that works even while the buffer holds unread bytes.
- This is the device behind the debugger's built-in terminal emulator, so a
  `console` device configured with no `transport=` gets stdin/stdout wired
  straight to it automatically — to the process's own terminal in the plain
  `emma65` CLI, or to the debugger's Terminal panel.

### Configuration

```toml
[[devices]]
type = "console"
address = 0xFFF8
break = 0x03
```

- `break` (optional, byte) — the break-key code described above.
- `transport` (optional) — see [Transport Options](#transport-options); omit
  it to use the default wiring described above.
- `irq` (optional, default `3`).

## 6522 Versatile Interface Adapter (`via/6522`)

A faithful emulation of the WDC 65C22 Versatile Interface Adapter (VIA) —
the same 16-register map, timers, shift register, and handshaking behavior
a 6502 program would see on real 65C22 hardware:

| Offset | Register | Purpose |
|--------|----------|---------|
| `$0` | ORB    | Port B input/output |
| `$1` | ORA    | Port A input/output (with CA1/CA2 handshaking) |
| `$2` | DDRB   | Port B data direction |
| `$3` | DDRA   | Port A data direction |
| `$4` | T1CL   | Timer 1 counter low (read) / latch low (write) |
| `$5` | T1CH   | Timer 1 counter/latch high |
| `$6` | T1LL   | Timer 1 latch low |
| `$7` | T1LH   | Timer 1 latch high |
| `$8` | T2CL   | Timer 2 counter low (read) / latch low (write) |
| `$9` | T2CH   | Timer 2 counter high |
| `$A` | SR     | Shift register |
| `$B` | ACR    | Auxiliary control register (latching, shift mode, timer modes) |
| `$C` | PCR    | Peripheral control register (CA1/CA2/CB1/CB2 edge/level select) |
| `$D` | IFR    | Interrupt flag register |
| `$E` | IER    | Interrupt enable register |
| `$F` | ORA    | Port A, bypassing CA1/CA2 handshaking |

- CA1, CA2, CB1, CB2 support every edge/level-triggering combination
  selectable via PCR.
- Timer 1 supports one-shot and free-run modes (with optional PB7
  square-wave output); Timer 2 supports one-shot and pulse-counting modes.
- The shift register supports all seven standard modes (input or output,
  clocked by T2, PHI2, or an external clock).
- IFR/IER give independent enable/mask control per interrupt source.

The VIA has no display or console of its own — whatever real (or emulated)
hardware would be wired to its ports and control lines connects instead over
a [Transport](#transport-options), exchanging port/pin state via the
[VIA Peer Protocol](appendix-via-protocol.md). On connection, the VIA sends a
full state dump so the peripheral starts with an accurate picture of every
pin and control line.

### Configuration

```toml
[[devices]]
type = "via/6522"
address = 0xFF80
transport = "unix:~/.emma/sock/via6522"
protocol = "ascii"
```

- `transport` (optional) — must be `tcp:` or `unix:`; `pipe:`/`pty:` are
  rejected because the peer protocol tags messages per connected peripheral,
  which only a multi-client transport supports.
- `protocol` (optional, `"ascii"` or `"binary"`, default `"ascii"`) — wire
  encoding for peer-protocol messages; see
  [VIA Peer Protocol](appendix-via-protocol.md).
- `irq` (optional, default `1`).

## MC6840 Programmable Timer Module (`ptm/6840`)

A faithful emulation of the Motorola MC6840 Programmable Timer Module (PTM):
three independent 16-bit timers, each capable of continuous or single-shot
generation (square-wave or pulse-width output), as well as frequency/period
or pulse-width *measurement* against an external gate/clock.

The PTM occupies 8 bytes of address space. Offset 0 is shared between two of
the three control registers:

| Offset | Write | Read |
|--------|-------|------|
| 0 | CR3 (if CR2 bit 0 clear) or CR1 (if set) | — |
| 1 | CR2 | Status register |
| 2 | Timer 1 latch MSB buffer | Timer 1 counter MSB (also loads the LSB buffer) |
| 3 | Timer 1 latch LSB (transfers the latched 16-bit value) | Timer 1 counter LSB buffer |
| 4 | Timer 2 latch MSB buffer | Timer 2 counter MSB |
| 5 | Timer 2 latch LSB | Timer 2 counter LSB buffer |
| 6 | Timer 3 latch MSB buffer | Timer 3 counter MSB |
| 7 | Timer 3 latch LSB | Timer 3 counter LSB buffer |

To load a 16-bit latch: write the MSB to the timer's MSB-buffer offset, then
the LSB to the timer's own offset — the full value transfers atomically on
the LSB write. To read a 16-bit counter: read the timer's own offset (which
also snapshots the LSB into its buffer), then read the adjacent LSB-buffer
offset. All three counters are big-endian in this register map (MSB first),
the opposite of the 6502's own little-endian convention.

Like the VIA, the PTM has no display or console of its own — a virtual
peripheral connects over a [Transport](#transport-options) to exchange
gate/clock/output signal state via the
[PTM Peer Protocol](appendix-ptm-protocol.md), with a full state dump sent
on connection.

### Configuration

```toml
[[devices]]
type = "ptm/6840"
address = 0xFF90
transport = "unix:~/.emma/sock/mc6840"
```

- `transport` (optional) — same multipoint (`tcp:`/`unix:`) restriction as
  the VIA, for the same reason.
- `protocol` (optional, `"ascii"` or `"binary"`, default `"ascii"`).
- `irq` (optional, default `2`).

## MC6850 Asynchronous Communications Adapter (`acia/6850`)

A faithful emulation of the Motorola MC6850 Asynchronous Communications
Interface Adapter (ACIA) — two addressable registers, matching the real
chip:

| Offset | Read | Write |
|--------|------|-------|
| 0 | Status register   | Control register |
| 1 | RX data register  | TX data register |

**Control register** (write offset 0): bits 1–0 select the counter divide
ratio (`11` triggers a master reset); bits 4–2 select word format (data
bits/parity/stop bits); bits 6–5 enable/configure the transmit interrupt;
bit 7 enables the receive interrupt.

**Status register** (read offset 0): bit 0 RDRF (receive data register
full), bit 1 TDRE (transmit data register empty), bit 5 OVRN (overrun), bit
7 interrupt pending. DCD, CTS, FE, and PE always read `0` in this emulation
— there's no real serial line to report a carrier, clear-to-send, framing,
or parity condition from.

TX is immediate: a byte written to the TX register goes straight to the
transport; TDRE clears on write and is restored on the next CPU tick. RX is
polled from the transport once per tick.

### Configuration

```toml
[[devices]]
type = "acia/6850"
address = 0xFFF4
transport = "pty:~/.emma/dev/ttyS1"
```

- `transport` (optional) — any [Transport](#transport-options) kind.
- `irq` (optional, default `4`).

## R6551 Asynchronous Communication Adapter (`acia/6551`)

An emulation of the Rockwell 6551 Asynchronous Communications Interface
Adapter (ACIA) — four addressable registers:

| Offset | Read | Write |
|--------|------|-------|
| 0 | RX data register  | TX data register |
| 1 | Status register   | Programmed reset (any value written) |
| 2 | Command register  | Command register |
| 3 | Control register  | Control register |

**Status register** (offset 1 read): bit 7 interrupt pending, bit 4 TDRE,
bit 3 RDRF, bit 2 OVRN.

**Command register** (offset 2): bit 1 disables the receive interrupt when
set; bits 3–2 `01` enables the transmit interrupt (any other value disables
it).

**Control register** (offset 3): bit 4 selects the receiver clock source —
`0` external (the device polls the transport every tick), `1` internal
(baud rate selected by bits 3–0, `0x1` = 50 baud … `0xF` = 19200 baud).

TX is immediate, like the MC6850. RX in internal-clock mode is timed to the
selected baud rate; in external-clock mode (the default) it's polled every
tick, for maximum responsiveness.

**WDC 65C51 bug compatibility**: the real WDC 65C51 has a well-known silicon
bug where TDRE gets stuck permanently set and never reflects transmit-busy
state, so software written for real 65C51 hardware uses fixed timing delays
instead of polling TDRE. This emulation defaults to *correct* TDRE behavior
(clears on write, restored after one byte period), but `with-tdre-bug` opts
into bug-compatible mode for software that expects the real chip's quirk.

### Configuration

```toml
[[devices]]
type = "acia/6551"
address = 0xFFF0
transport = "pty:~/.emma/dev/ttyS0"
```

- `transport` (optional) — any [Transport](#transport-options) kind.
- `with-tdre-bug` (optional bool, default `false`) — see above.
- `with-overrun` (optional bool, default `false`) — when `true`, a new byte
  arriving before the previous one is read sets the OVRN status bit
  (matching some real hardware); when `false` (the default, matching the
  common real-world case where OVRN doesn't reliably work), the old byte is
  simply kept until read and OVRN never sets.
- `irq` (optional, default `5`).

## RGB LED Matrix Display (`display/matrix`)

A memory-mapped RGB LED matrix display supporting 1, 2, 4, or 8 attached
32×32 matrices, fixed at configuration time. It occupies two separate
ranges: a block of pixel memory (one byte per pixel) and a 2-byte
command/data register pair elsewhere in the address space, so pixel memory
can start on a convenient boundary without the registers getting in the way.

**Pixel memory** is a flat, row-major raster of the composed canvas —
`columns * 32` pixels wide by `rows * 32` tall (from `arrangement`, below) —
addressed exactly like a real framebuffer: byte `row * width + col`. Each
pixel byte indexes one of 256 shared palette entries (16-bit RGB565 color,
matching real LED matrix driver hardware); the default palette follows the
Xterm 256-color layout (16 named colors, a 6×6×6 color cube, a 24-level
grayscale ramp). Writes target an off-screen buffer per matrix — nothing
appears on screen until that matrix is swapped to its visible buffer.

**Command/data registers** — write the command byte, then the argument
bytes it expects, one per write; a command that produces a reply is read
back one byte per read of the data register:

| Command | Value | Write bytes | Read bytes | Effect |
|---------|:-----:|-------------|------------|--------|
| `SWAP`             | 0 | 1 (matrix bitmask)                | —              | Swaps each matrix whose bit is set to its visible buffer immediately, regardless of whether it's actually changed |
| `SET_AUTOREFRESH`  | 1 | 1 (matrix bitmask)                | —              | Replaces which matrices auto-swap on every dirty vsync (all matrices, by default) |
| `SET_POWER`        | 2 | 1 (matrix bitmask; bit set = on)  | —              | Turns matrix drivers on/off (all on, by default) |
| `SET_BRIGHTNESS`   | 3 | 1 (`0`–`255`)                     | —              | Sets overall brightness uniformly across every attached matrix |
| `PALETTE_WRITE`    | 4 | 4: `index`, `red`, `green`, `blue`| —              | Sets palette entry `index` (colors are down-converted to RGB565) |
| `PALETTE_READ`     | 5 | 1: `index`                        | 3: `red`, `green`, `blue` | Reads back palette entry `index` (scaled up from its stored RGB565 value) |

The command register always reads `0`; writing it discards whatever partial
command sequence was in progress and arms a new one. There's no interrupt
capability — swaps are always synchronous, so there's nothing to wait on.

Like `display` and `display/lcd`, this device has no in-process
console-style rendering when running the plain `emma65` CLI:

- **The debugger** — the LED Matrix panel renders each matrix as an
  independent, composited canvas in-process, no configuration needed.
- **Standalone `emma65`** — configure a `pipe:` transport pointing at the
  bundled `emma65-led-matrix` SDL2 peripheral binary (see
  [Running the LED Matrix Peripheral](running-the-led-matrix-peripheral.md)
  below). A block message streams per matrix swap, and a palette message
  streams per palette write, over the
  [LED Matrix External Protocol](appendix-led-matrix-protocol.md).

### Configuration

```toml
[[devices]]
type = "display/matrix"
address = 0x9000
register-address = 0x9400
arrangement = "2x2"
transport = "pipe:/path/to/emma65-led-matrix"
```

`arrangement` (required, `COLSxROWS`, e.g. `2x2`) describes how the matrices
are physically daisy-chained: the matrix count (`columns * rows`, must be
1, 2, 4, or 8) and how bus addresses map onto them. Matrix *n* occupies the
`32x32` sub-rectangle at `((n / columns) * 32, (n % columns) * 32)` of the
composed canvas. There is no separate `matrix-count` attribute — a bare
count doesn't say how the matrices are wired, and having both invited them
to silently disagree. A `1xN` (single column) arrangement reproduces the
original one-matrix-per-1024-contiguous-bytes layout.

`register-address` (required) selects where the 2-byte command/data register
pair is mapped, separately from pixel memory. `transport` (optional) accepts
`pipe:` only — other transport kinds don't support the atomic bulk sends
this protocol relies on.

## Character LCD Display (`display/lcd`)

A memory-mapped character LCD module emulating a Hitachi HD44780-compatible
controller/driver, faithfully reproducing its real two-register bus
interface rather than mapping display memory directly:

- A 2-byte register pair (instruction/status and data), regardless of
  configured geometry — exactly like a real HD44780, all display state
  (DDRAM, CGRAM, address counter) is reached only indirectly through these
  two registers
- Command execution takes simulated time, reported via a busy flag on the
  instruction register, matching real HD44780 timing so programs written
  against real hardware assumptions behave the same way here
- Supports both the 8-bit and "software enabled" 4-bit interface widths,
  selected at runtime via `Function Set`, including the classic 5×8/5×10
  font height switch
- Geometry (rows × columns) is fixed at configuration time from a set of
  real-world HD44780 module layouts, quirks (like 16x1's split-segment
  addressing) included
- Not IRQ-capable — the HD44780 interface has no interrupt output

See `plan/memory-mapped-lcd-display-device-spec.md` in the repository for
the full register-level specification. Like `display` and `display/matrix`,
`display/lcd` has no in-process console-style rendering when running the
plain `emma65` CLI:

- **The debugger** — the LCD Display panel renders composited frames
  in-process, no configuration needed.
- **Standalone `emma65`** — configure a `pipe:` transport pointing at the
  bundled `emma65-lcd-display` SDL2 peripheral binary (see
  [Running the LCD Display Peripheral](running-the-lcd-display-peripheral.md)
  below). A frame streams on every register write that could change what's
  rendered, over the
  [LCD Display External Protocol](appendix-lcd-display-protocol.md).

### Configuration

```toml
[[devices]]
type = "display/lcd"
address = 0xD000
geometry = "16x2"
transport = "pipe:/path/to/emma65-lcd-display"
```

`geometry` (optional, default `16x2`) selects one of the ten supported
real-world module layouts: `8-character-5x10`, `16-character-5x10`, `8x2`,
`16x1`, `16x2`, `16x4`, `20x2`, `20x4`, `40x1`, `40x2`. Only the first two
have the 11 physical common lines a true datasheet 5×10 glyph needs -- an
HD44780 drives 16 common outputs total, and every wider common module (even
one, like `16x1`, whose name refers to a 16-*character* row) tops out at
what 5×8 needs, so `Function Set`'s `F=1` is a no-op (logged as a warning)
on every geometry but those two. `cgrom` (optional) selects the bundled character
generator ROM by name -- `a00` (the default, and the ROM code most HD44780
clones ship with) or `a02` (the European-font variant), case-insensitive --
or overrides it with a file of the same format.

`polarity` (optional, default `positive`) and `backlight` (optional, default
`yellow`) together select one of 8 color-scheme presets modeling
commonly available real LCD modules, rather than requiring hand-picked RGB24
values: `positive` polarity renders dark pixels over a backlight-colored
background; `negative` polarity renders backlight-colored pixels over a dark
"opaque near-black" background. Not every `backlight` value is valid for
every `polarity` — only the combinations below are:

| `polarity`  | `backlight` |
|-------------|-------------|
| `positive`  | `yellow`    |
| `positive`  | `white`     |
| `positive`  | `amber`     |
| `positive`  | `blue`      |
| `negative`  | `blue`      |
| `negative`  | `white`     |
| `negative`  | `amber`     |
| `negative`  | `red`       |

`background`/`foreground` (optional, hex RGB24) remain available for fully
custom colors — each, if given, overrides the corresponding channel of the
`polarity`/`backlight` preset. None of these are part of the HD44780's own
behavior, and none are bus-addressable.

## Character Display (`display`)

A memory-mapped character/color-cell text display, structurally similar to
the VIC-II in the Commodore 64 (separate character RAM and color RAM over a
fixed grid), but with a full 8-bit palette index per cell rather than 4-bit,
and a grid size that's configurable rather than fixed (40×25 by default).
The 8×8 glyph font and RGB24 color palette are supplied at configuration
time and are *not* part of the device's bus-addressable memory — only the
two per-cell RAM arrays and two control registers are:

| Region | Offset | Size | Access | Notes |
|--------|--------|------|--------|-------|
| Character RAM | `0` | `cells` | R/W | Glyph index per cell (`cells = columns * rows`) |
| Color RAM | `cells` | `cells` | R/W | Palette index per cell |
| Control register | `2*cells` | 1 | R/W | Bit 0: request a swap now. Bit 1: auto-swap on every vsync. Bit 3: arm a palette update. Bit 7 (read-only): a requested swap is still pending |
| Status/data register | `2*cells + 1` | 1 | R/W | Read: bit 0 vsync occurred, bit 1 a palette update was accepted (both clear on read). Write: feeds a 4-byte armed palette-update sequence (`index`, `red`, `green`, `blue`), ignored unless control bit 3 was set first |

Character/color RAM writes always target an off-screen buffer; nothing
changes on screen until a swap — either requested explicitly (control bit
0) or automatically on every vsync (control bit 1).

**Keyboard input** (optional): configuring `keyboard-address=` maps a
second, separate 2-byte data/latch register pair — behaviorally identical
to [Console](#console-console)'s (the same latch-and-clear-on-read
semantics, the same optional break-key handling) — anywhere else in the
address space, so a program can treat the display as a combined
screen-and-keyboard console. This is also what makes the device IRQ-capable
at all; with no keyboard range configured it never asserts IRQ. **In the
plain `emma65` CLI, configuring `keyboard-address=` maps the registers but
nothing feeds them input** — it's the debugger's Display panel that
actually captures keystrokes and wires them into that range, so this
attribute is only useful when running under the debugger.

See `plan/memory-mapped-display-device-spec.md` in the repository for the
full register-level specification. Unlike the other register-window devices,
`display` has no in-process console-style rendering when running the plain
`emma65` CLI: it needs an external peripheral to actually put pixels on
screen. Two ways to view it:

- **The debugger** — the Display panel renders composited frames in-process,
  no configuration needed, and also supplies the live keyboard input
  described above.
- **Standalone `emma65`** — configure a `pipe:` transport pointing at the
  bundled `emma65-display` SDL2 peripheral binary (see
  [Running the Display Peripheral](running-the-display-peripheral.md) below).
  Composited frame data (char RAM, color RAM, palette, and the font) streams
  to the peripheral once per vsync over the
  [Character Display External Protocol](appendix-display-protocol.md).

### Configuration

```toml
[[devices]]
type = "display"
address = 0xF000
columns = 40
rows = 25
transport = "pipe:/path/to/emma65-display"
```

- `columns`/`rows` (optional, default 40×25) — grid size; both must be
  positive.
- `palette` (optional, path) — a text file, one `RRGGBB` (or `#RRGGBB`)
  color per line, either 16 or 256 entries; overrides the compiled-in
  default palette.
- `font` (optional, path) — a raw 2048-byte file (256 glyphs × 8 bytes, one
  byte per row, bit 0 = leftmost pixel); overrides the compiled-in default
  8×8 font.
- `double-buffered` (optional bool, default `true`).
- `frame-rate-hz` (optional, default `60`) — vsync/auto-swap cadence; not
  the same as the external protocol's own frame rate.
- `transport` (optional) — `pipe:` only, for the same atomic bulk-send
  reason as `display/matrix`.
- `keyboard-address` (optional) — see above.
- `break` (optional, byte) — break-key code for the keyboard sub-range; has
  no effect unless `keyboard-address=` is also set.
- `irq` (optional, default `7`) — only meaningful (and only allocated) when
  `keyboard-address=` is set.

## 16-bit Galois LFSR (`lfsr`)

A memory-mapped pseudo-random number generator based on a 16-bit Galois
linear-feedback shift register. It occupies 2 bytes of address space:

| Offset | Read | Write |
|--------|------|-------|
| 0 (LOW)  | Latches the current state and returns its low byte; in step mode, this read is also what advances the register | Buffers a low seed byte |
| 1 (HIGH) | Returns the latched high byte (no side effect) | Loads the seed `(buffered low byte) \| (value << 8)` into the register |

- **Continuous** mode (the default) advances the register once per CPU
  clock cycle, so successive reads (without reseeding) return a fresh
  pseudo-random value each time.
- **Step** mode advances the register only when the LOW register is read,
  for a sequence driven entirely, and reproducibly, by the program.
- To reseed: write the low byte first (buffered, not yet applied), then the
  high byte (loads both into the register together). A seed of `0x0000` is
  clamped to `0x0001`, since an all-zero state would never change.

This device is never IRQ-capable.

### Configuration

```toml
[[devices]]
type = "lfsr"
address = 0xFFF6
mode = "step"
```

- `taps` (optional, default `0xB400`) — the Galois tap mask; the default
  gives a maximal-length, 65535-state sequence.
- `mode` (optional, `"continuous"` or `"step"`, default `"continuous"`).

## Bank-Switched Memory Modules

Finch, Phoebe, and Vireo are complete memory subsystems — RAM, ROM, and a
bank-switching MMU — rather than register-window devices, each modeled on a
different real single-board-computer design. Each claims the entire 64 KB
address space when configured, so no separate `ram`/`rom` entries are needed
alongside them, and their `address` device-spec field is unused — the
addresses that matter are the ones given to their control register(s)
instead. All three share these attributes:

- `image` (required, path) — a ROM image loaded at `offset` (default `0`)
  within the module's ROM region.
- `labels` (optional, path) — a VICE-format label file, for symbol
  resolution in the debugger and tracer.
- `write-policy` (optional, `"ignore"` or `"error"`, default `"ignore"`) —
  what happens when the 6502 program writes to ROM: silently discard the
  write, or report it as a bus error.
- `fill` (optional, byte) — value used to initialize any ROM bytes the
  image doesn't cover.

### Finch bank-switched MMU (`mem/finch`)

512 KB RAM and 512 KB ROM behind a simple MMU: the top four bits of the 6502
address bus (`A12..A15`) index into 16 one-byte bank registers, each
selecting which 4 KB segment of the module's 1024 KB memory space is mapped
into that 4 KB window of the 6502's address space — banks `0x00`–`0x7F` are
RAM, `0x80`–`0xFF` are ROM. The bank registers support both read and write,
so a program doesn't need to keep a shadow copy.

At reset, the MMU is disabled and a fixed mapping is used instead: the low
32 KB of RAM (banks `0x00`–`0x07`) fills the lower half of the address
space, and the low 32 KB of ROM (banks `0x80`–`0x87`) fills the upper half
— so the reset vector always comes from ROM regardless of what's currently
in the bank registers. Setting bit 7 (`MMUE`) of the control register
switches to the MMU-driven mapping; program the bank registers first, since
flipping this bit mid-configuration changes what's mapped where
immediately. Because the control register may share its address with other,
unrelated configuration bits on real Finch hardware, read-modify-write the
register rather than writing it outright:

```text
        LDA $FFD8       ; fetch the config register state
        ORA #$80        ; set the high order bit (MMUE)
        STA $FFD8       ; store the new config register state
```

```toml
[[devices]]
type = "mem/finch"
bank-registers = 0xFC00
control-register = 0xFFD8
image = "rom.bin"
labels = "rom.lbl"
```

- `bank-registers` (required, alias `banks`) — base address of the 16
  one-byte bank registers (must be paragraph-aligned, i.e. a multiple of
  16).
- `control-register` (required, alias `ctrl`) — address of the 8-bit MMU
  control register (bit 7 = MMUE, the rest reserved).

### Phoebe bank-switched memory (`mem/phoebe`)

56 KB RAM and 32 KB ROM. The ROM is split into four 8 KB banks (numbered
0–3, at offsets `0x0000`, `0x2000`, `0x4000`, `0x6000` in the image file);
bank 3 is permanently mapped into the upper half of a 16 KB switchable
region at `0xC000` and must contain the 6502 machine vectors (NMI at
`0x7FFA`, Reset at `0x7FFC`, IRQ at `0x7FFE`, relative to the bank start). A
single control register selects what occupies the lower half of that
region:

| Bit 1 | Bit 0 | Selection       |
|-------|-------|-----------------|
|   0   |   0   | ROM Bank 0      |
|   0   |   1   | ROM Bank 1      |
|   1   |   0   | ROM Bank 2      |
|   1   |   1   | RAM (the 8 KB of RAM sharing this region becomes visible) |

Only these two bits are significant — the rest of the register is ignored
on write and always reads `0`. The register resets to `0` (ROM bank 0) at
system reset.

```toml
[[devices]]
type = "mem/phoebe"
control-register = 0xFFF7
image = "rom.bin"
```

- `control-register` (required, alias `ctrl`) — address of the 8-bit
  bank-selection register (bits 1–0 only).
- `ram-fill` (optional, byte) — value used to initialize RAM at startup,
  separate from `fill` (which covers ROM).

### Vireo bank-switched memory (`mem/vireo`)

128 KB RAM and 32 KB ROM behind a bank-switching scheme with four
configurations, selected via one control register:

| Mode | `0x0000`–`0x7FFF` | `0x8000`–`0xFFFF` |
|------|--------------------|--------------------|
| 0 | RAM `0x00000`–`0x07FFF` | ROM |
| 1 | RAM `0x10000`–`0x17FFF` | ROM |
| 2 | RAM `0x00000`–`0x0FFFF` (whole address space) | — |
| 3 | RAM `0x10000`–`0x1FFFF` (whole address space) | — |

In every mode, the 8 KB region at `0xC000`–`0xDFFF` can additionally be
pointed at any 8 KB, 4 KB-aligned segment of whichever RAM half the current
mode doesn't otherwise map — giving a program access to RAM beyond the 64 KB
address space without leaving its current mode. Control register bit layout
(bit 7 always reads `0`):

```text
    ┌────┬────┬────────┬────────────────┐
    │ -- │ WI │ M1  M0 │ S3  S2  S1  S0 │
    └────┴────┴────────┴────────────────┘
```

- **WI** (bit 6) — Window Inhibit: `1` disables the `0xC000` window,
  exposing whatever it normally shadows instead.
- **M1–M0** (bits 5–4) — selects Mode 0–3 from the table above.
- **S3–S0** (bits 3–0) — which 8 KB segment of the *other* RAM half is
  mapped into the window; the complement of mode bit `M0` plus this 4-bit
  field form the segment's base address (e.g. in Mode 0 or 2, segment `0x9`
  maps physical address `0x19000`). Segment `0xF` wraps: its upper half
  comes from the top of the region and its lower half from the bottom.

```toml
[[devices]]
type = "mem/vireo"
control-register = 0xFFF4
image = "rom.bin"
```

- `control-register` (required, alias `ctrl`) — address of the 8-bit
  control register described above.
- `ram-fill` (optional, byte).

## Transport Options

Devices that exchange byte streams attach a `Transport`. Configurable via TOML/CLI:

| Transport             | Shorthand                       | Best for                                                              |
|------------------------|---------------------------------|-------------------------------------------------------------------------|
| `PipeTransport`        | `pipe:/path/to/exe,arg1,arg2`   | Spawning a child process and bridging its stdin/stdout to the device    |
| `TcpSocketTransport`   | `tcp:PORT` or `tcp:IP:PORT`     | Connecting a terminal emulator or remote process over the network       |
| `UnixSocketTransport`  | `unix:PATH`                     | Low-latency local IPC (lower overhead than TCP)                         |
| `PtyTransport`         | `pty` or `pty:SYMLINK_PATH`     | Any program that expects a real TTY — `screen`, `minicom`, `cu`, etc.   |

A fifth implementation, `InternalPipeTransport`, isn't configured via
TOML/CLI — the `emma65` binary and the debugger UI use it internally to wire
a console device directly to the host process's own stdin/stdout (CLI) or
terminal window (debugger) when no `transport` attribute is given.

Every transport's actual I/O runs on its own thread or async task, decoupled
from device `tick()` by a lock-free `rtrb` ring buffer (`ChannelRelay`): the
transport side pushes into the ring as bytes arrive, and `tick()` drains
whatever is currently available and returns immediately, whether that's
nothing, one byte, or a burst. Neither side ever blocks the other. Because
the CPU thread is practically unburdened by communication with external
peripherals, it can easily sustain common effective clock speeds — and much
higher ones with clock throttling disabled (`ClockSpeed::unlimited()`).

The VIA and MC6840 additionally support framing their transport traffic with
a structured peer-communication protocol (`protocol = "ascii"` or `"binary"`)
that exchanges full port/pin state on connection and incremental updates
thereafter, so a real or emulated peripheral always has an accurate picture
of the device's signals — see the [VIA Peer Protocol](appendix-via-protocol.md)
and [PTM Peer Protocol](appendix-ptm-protocol.md) appendices.
