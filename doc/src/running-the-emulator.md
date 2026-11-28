# Running the Emulator

## Choosing the CPU

The emulator is the `emma6k` command, which runs any supported CPU. Two
shorter commands run it with the CPU family fixed:

- **`emma65`** runs the 65C02 family. With no CPU named, it uses the CMOS
  `65C02`.
- **`emma09`** runs the Motorola 6809.

`emma6k` itself has no default CPU, so name one with the `cpu-variant`
setting, or its shorter synonym `cpu`:

| Value | CPU |
|---|---|
| `65C02` (or `C02`) | CMOS 65C02 |
| `WDC65C02` (or `WDC02`) | WDC 65C02, which adds STP, WAI, BBR/BBS and RMB/SMB |
| `6809` | Motorola 6809 |

Values are case-insensitive. Set it in a configuration file
(`cpu-variant = "6809"`), on the command line (`--cpu 6809` or
`--cpu-variant 6809`), or in the environment (`EMMA6K_CPU=6809`). Use one
spelling per source: setting both `cpu` and `cpu-variant` in the same file,
or both flags, is an error.

`emma65` and `emma09` refuse a CPU from the other family. For example,
`emma65 --cpu 6809` stops and suggests `emma09`.

`--profile <name>` runs one of the bundled starter profiles, which chooses
its own CPU. The 65C02 profiles are `taliforth`, `msbasic`, `ehbasic`,
`rain`, `lcd` and `snake`; the 6809 profile is `camelforth`. A profile can't be combined with `--config`, or
with a CPU from another family.

## Default configuration

When `emma65` is launched with no devices configured, it runs with a built-in
[TaliForth 2](https://github.com/SamCoVT/TaliForth2) ROM and a full set of
peripherals:

- 32 KB zero-filled RAM at `0x0000`–`0x7FFF`
- TaliForth ROM at `0x8000`–`0xFFFF`
- VIA at `0xFF80` on a Unix-domain socket (`~/.emma/sock/via6522`)
- R6551 ACIA at `0xFFF0` on a pseudo-terminal (`~/.emma/dev/ttyS0`)
- MC6850 ACIA at `0xFFF4` on a pseudo-terminal (`~/.emma/dev/ttyS1`)
- LFSR at `0xFFF6` in step mode
- Console device at `0xFFF8`–`0xFFF9`, with `Ctrl+C` (`0x03`) configured as
  its break key, connected to the process's own standard input and output
- WDC 65C02 variant at 1.8432 MHz

Interact with the Forth interpreter via standard input and output.

When `emma09` is launched with no devices configured, it runs CamelForth, a
Forth for the 6809 by Bradford J. Rodriguez, as ported to the Multicomp 6809
by Neal Crook:

- 56 KB zero-filled RAM at `0x0000`–`0xDFFF`
- CamelForth ROM at `0xE000`–`0xFFFF`
- Console device at `0xFFD0`–`0xFFD1`, with `Ctrl+C` (`0x03`) as its break
  key, connected to the process's own standard input and output
- [Exit device](io-devices.md#exit-exit) at `0xFFD2`
- 6809 at 2 MHz

Type Forth at the `OK` prompt. Ctrl+C abandons a running word, such as an
endless loop, and returns to the interpreter with a fresh `OK` prompt. To
quit, type `BYE`: the emulator exits with status 0. Or start the emulator
with `--keep-isig` so that Ctrl+C ends it instead.

CamelForth is licensed under the GNU General Public License, version 3 or
later, unlike the rest of Emma6k, which is MIT-licensed. The license text,
the Forth source, and the changes made for Emma6k are in the `camelforth`
template's directory in the source repository.

## TOML configuration file

Use `--config <file>` to load a TOML configuration file. Top-level keys map
directly to emulator fields — there is no `[emulator]` wrapper:

```toml
cpu-variant = "WDC65C02"      # or "65C02" (CMOS), or "6809"; `cpu` is a synonym
clock-speed-hz = 1843200      # omit for unlimited throughput
unmapped-policy = "ignore"    # or "error"; how unmapped-address accesses
                               # are handled (default: "ignore")

[[devices]]
type = "ram"
address = 0x0000
size = 32768               # or the quoted string "32K"

[[devices]]
type = "rom"
address = 0x8000
size = 32768
image = "~/roms/my.bin"    # .bin, .rom, .hex, .ihx, .ihex, .s19, .srec

[[devices]]
type = "console"
address = 0xFFF8
transport = { pty = { path = "~/.emma/dev/ttyS0" } }
```

## CLI flags

All config values can also be set from the command line. Command-line flags
take precedence over environment variables, which take precedence over the
TOML file. CLI arguments are the natural fit for the standalone emulator — a one-off run, a
quick experiment, a shell script — where writing a TOML file is more
ceremony than the task needs; the debugger drives its own profiles instead
and has no use for these flags.

```
emma65 --cpu-variant WDC65C02 \
       --clock-speed-hz 1843200 \
       --unmapped-policy error \
       --device ram@0x0000,size=32768,fill=0 \
       --device rom@0x8000,size=32768,image=~/roms/my.bin \
       --device console@0xFFF8,transport=pty:~/.emma/dev/ttyS0
```

`--device` accepts one spec and can be repeated (as above), or several specs
space-separated after a single `--device`:

```
emma65 --device ram@0x0000,size=32768 rom@0x8000,size=32768,image=~/roms/my.bin
```

Device shorthand format: `type@address[,key=value,...]`

- Address: decimal, `0x` hex, `0o` octal, or `0b` binary
- Size: bytes, or `K`/`k` suffix for kibibytes (e.g. `32K`) — since every
  `key=value` here is already a string, no quoting is needed (contrast the
  TOML form above, where the suffixed form must be a quoted string)
- Paths support `~/` tilde expansion
- Boolean attributes take `true`/`false` (e.g. `with-tdre-bug=true`)

### Transport arguments on the CLI

Any device with a `transport` attribute takes the same shorthand on the
command line as in TOML (see [Transport Options](io-devices.md#transport-options)
for the full description of each kind):

| Transport   | CLI shorthand           | Example |
|-------------|--------------------------|---------|
| Pipe        | `pipe:/path/to/exe`     | `--device display@0xF000,transport=pipe:/path/to/emma6k-display` |
| TCP (any interface)      | `tcp:PORT`  | `--device console@0xFFF8,transport=tcp:9600` |
| TCP (specific interface) | `tcp:IP:PORT` | `--device console@0xFFF8,transport=tcp:127.0.0.1:9600` |
| Unix domain socket       | `unix:PATH` | `--device via/6522@0xFF80,transport=unix:~/.emma/sock/via6522` |
| PTY (auto-named)         | `pty`       | `--device acia/6551@0xFFF0,transport=pty` |
| PTY (named symlink)      | `pty:SYMLINK_PATH` | `--device acia/6551@0xFFF0,transport=pty:~/.emma/dev/ttyS0` |

**Pipe transport arguments need a TOML file.** In TOML, `transport = { pipe
= { command = ["/path/to/exe", "arg1", "arg2"] } }` can pass arguments to the
spawned process. The CLI shorthand can't: a `--device` spec's own
`key=value,...` attributes are comma-separated, so a comma inside a `pipe:`
value is parsed as the start of the next attribute rather than as an
argument separator. `transport=pipe:/path/to/emma6k-display` (no arguments
to the peripheral) works fine on the command line; something like
`transport=pipe:/path/to/exe,--some-flag` does not — reach for a TOML config
file instead when the peripheral needs arguments.

### Device configuration examples

Every built-in device type from [I/O Devices](io-devices.md) and
[Memory Devices](memory-devices.md) can be configured entirely from the
command line. A few representative examples, one `--device` flag per device:

```
# RAM and ROM
--device ram@0x0000,size=32K,fill=0
--device rom@0x8000,size=32K,image=~/roms/my.bin

# Console wired to a PTY, with a break key configured
--device console@0xFFF8,transport=pty:~/.emma/dev/ttyS0,break=0x03

# 6522 VIA over a Unix socket, using the binary peer protocol
--device via/6522@0xFF80,transport=unix:~/.emma/sock/via6522,protocol=binary

# 6551 ACIA over a PTY, in bug-compatible TDRE mode
--device acia/6551@0xFFF0,transport=pty:~/.emma/dev/ttyS0,with-tdre-bug=true

# 6850 ACIA listening on a TCP port
--device acia/6850@0xFFF4,transport=tcp:9600

# MC6840 PTM over a Unix socket
--device ptm/6840@0xFF90,transport=unix:~/.emma/sock/mc6840

# Character display with keyboard input, rendered by emma6k-display
--device display@0xF000,keyboard-address=0xF800,break=0x03,transport=pipe:/path/to/emma6k-display

# LCD display, rendered by emma6k-lcd-display
--device display/lcd@0xD000,geometry=16x2,polarity=negative,backlight=blue,transport=pipe:/path/to/emma6k-lcd-display

# 2x2 RGB LED matrix, rendered by emma6k-led-matrix
--device display/matrix@0x9000,register-address=0x9400,arrangement=2x2,transport=pipe:/path/to/emma6k-led-matrix

# 16-bit Galois LFSR in step mode
--device lfsr@0xFFF6,mode=step,taps=0xB400

# Priority interrupt controller
--device pic/finch@0xFFFF

# Finch bank-switched MMU — address is required but unused, see Memory Devices
--device mem/finch@0,bank-registers=0xFC00,control-register=0xFFD8,image=rom.bin,labels=rom.lbl

# Phoebe bank-switched memory
--device mem/phoebe@0,control-register=0xFFF7,image=rom.bin

# Vireo bank-switched memory
--device mem/vireo@0,control-register=0xFFF4,image=rom.bin
```

## Environment variables

Any config key can be set with the `EMMA6K_` prefix, using `_` in place of
`-`:

```
EMMA6K_CPU_VARIANT=WDC65C02
EMMA6K_CLOCK_SPEED_HZ=1843200
```

Earlier versions used the `EMMA65_` prefix, which is no longer read. The
debugger reads the same `EMMA6K_` variables.

## Built-in device types

| Type            | Registers | Key attributes                                                                     |
|-----------------|:---------:|-------------------------------------------------------------------------------------|
| `ram`           |     —     | `size` (required, integer bytes or quoted `"K"`/`"k"`-suffixed string), `fill` (optional byte), `image` (optional path) |
| `rom`           |     —     | `size` (required, integer bytes or quoted `"K"`/`"k"`-suffixed string), `image` (required path), `fill` (optional byte) |
| `console`       |     2     | `transport` (optional), `break` (optional byte: break-key code)                     |
| `acia/6551`     |     4     | `transport` (optional), `with-tdre-bug` (bool), `with-overrun` (bool)               |
| `acia/6850`     |     2     | `transport` (optional)                                                              |
| `via/6522`      |    16     | `transport` (optional), `protocol` (`ascii` or `binary`, optional)                  |
| `ptm/6840`      |     8     | `transport` (optional), `protocol` (`ascii` or `binary`, optional)                  |
| `display/matrix`| variable  | `arrangement` (required `COLSxROWS`; `columns * rows` must be 1, 2, 4, or 8), `register-address` (required), `frame_rate_hz`, `transport` (optional, `pipe:` only) |
| `display/lcd`   |     2     | `geometry` (optional, default `16x2`), `cgrom` (optional, `a00`/`a02`/path), `polarity`, `backlight` (optional presets), `background`, `foreground` (optional hex overrides), `transport` (optional, `pipe:` only) |
| `display`  |  variable | `columns`, `rows` (optional, default 40×25), `palette`, `font` (optional paths), `double-buffered` (bool), `frame-rate-hz`, `transport` (optional, `pipe:` only), `keyboard-address` (optional address: maps a second 2-byte data/latch range, debugger-only for live input), `break` (optional byte, requires `keyboard-address`), `irq` (optional, only allocated when `keyboard-address` is set) |
| `lfsr`          |     2     | `taps` (optional u16), `mode` (`continuous` or `step`, optional)                    |
| `mem/finch`     |     2     | `bank-registers`, `control-register` (required addresses), `image` (required path), `write-policy`, `fill`, `offset`, `labels` (all optional) |
| `mem/phoebe`    |     1     | `control-register` (required address), `image` (required path), `write-policy`, `fill`, `ram-fill`, `offset`, `labels` (all optional) |
| `mem/vireo`     |     1     | `control-register` (required address), `image` (required path), `write-policy`, `fill`, `ram-fill`, `offset`, `labels` (all optional) |

`mem/finch`, `mem/phoebe`, and `mem/vireo` each occupy the entire 64 KB
address space rather than a fixed-size register window; their register count
above is the count of dedicated MMU/bank-control registers, placed at the
configurable addresses shown, not a contiguous block.

`display`'s register window is `2 * columns * rows + 2` bytes (char RAM + 
color RAM + a control register + a status/data register), so it grows with
the configured grid size rather than being fixed.

`display/matrix`'s pixel memory is `columns * rows * 1024` bytes (from its
`arrangement`), based at `address`; its command and data registers are a
separate 2-byte range based at `register-address` rather than immediately
following pixel memory, so the two can be placed independently on the bus
(e.g. keeping pixel memory aligned to a 1 KiB/N KiB boundary).

Every `transport` attribute above accepts the same shorthand string in TOML
(`transport = "unix:~/.emma/sock/via6522"`) as on the CLI — see
[Transport arguments on the CLI](#transport-arguments-on-the-cli) above for
the full set of forms and a CLI-specific limitation around `pipe:` arguments.
