# The Emulator Core

At the heart of Emma65 is a CPU model that faithfully emulates the 65C02
instruction set and interrupt behavior, paired with a flexibly configurable
memory bus and a growing library of virtual I/O devices — everything the
`emma65` command-line emulator, the debugger, and the tracer are all built
on. Memory and devices are mapped into the 16-bit address space however a
program needs them, devices talk to real or emulated peripherals over
pluggable transports, and execution can be inspected and controlled through
expression-based watchpoints and a recorded instruction trace. The following
sections describe this core in detail, starting with how closely it matches
real 65C02 hardware.

## Correctness

Emma65 passes
the [Klaus Dormann 65C02 test suite](https://github.com/Klaus2m5/6502_65C02_functional_tests),
which exhaustively exercises every instruction, addressing mode, flag
computation, interrupt sequence, and decimal-mode operation defined by the
65C02 architecture. It also passes
the [Bruce Clark decimal mode test](http://www.6502.org/tutorials/decimal_mode.html),
which independently verifies all 256×256 ADC and SBC operand combinations in
BCD mode against predicted CMOS 65C02 results. Users can rely on Emma65's
instruction-level behavior matching real hardware.

## Features

### Instruction Set

Emma65 emulates two variants of the 65C02 processor family:

- **CMOS 65C02** — the standard CMOS variant, including all instructions added
  over the original NMOS 6502: `BRA`, `STZ`, `TSB`, `TRB`, `PHX`, `PHY`,
  `PLX`, `PLY`, accumulator-mode `INC` and `DEC`, zero-page indirect
  addressing, and `JMP (abs,X)`.

- **WDC 65C02** — the Western Design Center variant, which adds 34 opcodes to
  the CMOS baseline: `STP` (stop the processor), `WAI` (wait for interrupt),
  `BBR0`–`BBR7` and
  `BBS0`–`BBS7` (branch on bit clear/set), and `RMB0`–`RMB7` and `SMB0`–`SMB7`
  (reset/set memory bit).

All 16 addressing modes are supported, including the zero-page relative mode
used by the WDC bit-branch instructions. Invalid opcodes can be configured to
either silently act as NOPs or to halt execution with an error.

Emulating the original NMOS 6502 — its undocumented opcodes, its
read-modify-write double-write behavior, and the various other quirks that
NMOS-focused emulators go to great lengths to reproduce — is explicitly not a
goal of this project. Both variants above are CMOS designs, and "invalid
opcode" above means exactly that: an opcode with no defined CMOS behavior,
handled by configuration rather than by reproducing whatever the NMOS die
happened to do with it. Projects that do emulate the NMOS 6502's undocumented
behavior, if that's what you're looking for, include:

- [VICE](https://vice-emu.sourceforge.io/) — a suite of Commodore computer
  emulators with a cycle-exact 6510/8500 core, illegal opcodes included
- [Mesen](https://www.mesen.ca/) — a NES/Famicom emulator whose 2A03 CPU
  core reproduces the NMOS 6502's unofficial opcodes cycle-accurately
- [Visual6502](http://www.visual6502.org/) — a transistor-level simulation
  of the original NMOS 6502 die, the reference many other emulators validate
  their undocumented-opcode behavior against
- the [NESdev wiki's unofficial opcodes
  reference](https://www.nesdev.org/wiki/CPU_unofficial_opcodes) — a
  well-maintained catalog of the quirks themselves, useful background on
  what's being left out here

### Interrupt Support

Emma65 implements the full 65C02 interrupt model:

- **RESET** — restores the CPU to its power-on state: every device on the
  bus is reset to its own power-on state, the stack pointer is set to
  `$FF`, the status register to `I` (interrupts disabled, every other flag
  clear), the cumulative cycle counter is zeroed, and any `STP`/`WAI`-halted
  state is cleared. The program counter is then loaded from the reset vector
  at `$FFFC`/`$FFFD`. Emma65 issues one automatically before running the
  first instruction of a session, and the debugger's CPU/Bus panel exposes
  it as an on-demand control.
- **NMI** — edge-triggered and latched: the first falling edge sets a pending
  flag that is consumed exactly once, with highest priority over simultaneous
  IRQ. Any device capable of signaling an NMI (for example a VIA's CA1 line)
  can trigger one.
- **IRQ** — level-triggered and multi-source: multiple devices can
  independently assert and release the IRQ line; the interrupt fires when any
  source is active and the I flag is clear. Each device's IRQ state is polled
  after every instruction.
- **BRK** — software interrupt; sets the B flag in the pushed status byte so
  interrupt handlers can distinguish a BRK from a hardware IRQ.

On interrupt entry the D flag is cleared, matching CMOS 65C02 hardware
behavior.

### Clock Speed Simulation

Free-running execution throttles to a configurable target clock frequency by
comparing accumulated emulated cycles against elapsed wall time, sleeping as
needed to match the target rate. Throttling is batched over roughly 1,000
instructions at a time, keeping sleep-syscall overhead negligible while
maintaining sub-millisecond timing granularity. This comfortably covers the
clock speeds of all historically common 6502-based systems, and headroom on
modern hardware goes well beyond that. In both cases below, accuracy held to
within 0.02% of target right up to the boundary shown, then fell off sharply
once the requested speed exceeded what the host could execute unthrottled —
so the boundary itself, not some margin below it, is the practical ceiling:

| device complement | measured accurate ceiling |
|---|---|
| bundled default (32K RAM, 32K ROM, VIA, two ACIAs, LFSR, console) | ~34 MHz |
| minimal (32K RAM, 32K ROM, console only) | ~85 MHz |

(release build, on a mid-range 2023 laptop CPU — AMD Ryzen 5 7530U). Every
polled device adds per-instruction overhead — it's given a chance to advance
its own state after every single instruction — so trimming the default
complement down to just RAM, ROM, and a console more than doubled the
ceiling here. Use these two points to
interpolate a rough expectation for your own configuration: more polled
devices pulls the ceiling down toward the low end, a bare-bones setup pushes
it toward the high end. The ceiling also depends on the host CPU and the
build profile — a debug build is roughly an order of magnitude slower than
release and hits its own, much lower ceiling — so treat it as "however fast
your configuration runs unthrottled on your machine in a `--release` build,"
not a fixed number.

The target clock speed is set with the `clock-speed-hz` TOML/CLI setting (see
[Running the Emulator](running-the-emulator.md)); some familiar reference
points:

| Setting | Speed |
|---|---|
| `clock-speed-hz = 1000000` | 1 MHz — Apple II speed |
| `clock-speed-hz = 1843200` | 1.8432 MHz — common UART baud-rate crystal |
| `clock-speed-hz = 2000000` | 2 MHz — BBC Micro speed |
| omitted | Maximum throughput; no throttling |

### Memory and Bus Configuration

The memory bus is organized around named address regions mapped into the
16-bit address space. Regions can be RAM, ROM (write-protected), or I/O device
windows, configured via TOML or CLI flags (see
[Running the Emulator](running-the-emulator.md)). The bus uses a
most-specific-wins overlap policy: a smaller region always shadows a larger
one at the same addresses, which makes it easy to place a device register
window inside a ROM region. Ambiguous overlaps (same-size regions at the same
addresses) and ROM size mismatches are caught when the configuration is
loaded, before the program ever runs, and reported as a startup error rather
than a silent misconfiguration.

Address resolution is a one-time cost paid when the configuration loads, so
reads and writes at runtime are effectively free regardless of how many
devices are configured — bus overhead stays out of the way of maximum
emulated CPU throughput. By default, accesses to addresses not covered by any
configured region are silently ignored (reads return `0xFF`, writes are
discarded), matching how unpopulated address space typically behaves on real
hardware. Set `unmapped-policy = "error"` (TOML) or `--unmapped-policy error`
(CLI) to instead treat these as bus errors — useful when tracking down a
program that's straying outside its intended memory map. Bus errors (this
setting, and ROM write violations) are reported back to whichever tool is
running the CPU (the `emma65` CLI, the debugger, or the tracer) so it can
decide how to respond — typically by halting and reporting the error.

### Memory-Mapped I/O Devices

The built-in `ram`, `rom`, `console`, and other device `type`s configurable
from TOML/CLI (see [Running the Emulator](running-the-emulator.md)) share one
configuration surface, so adding a new device type is a matter of plugging
into that same surface — see
[Adding a Custom Device Module](for-contributors.md#adding-a-custom-device-module)
under For Contributors for how to build one.

A device has a small set of capabilities beyond plain memory: it can advance
internal timers and counters in step with CPU time (a VIA's two timers and a
PTM's three are both built on this), assert or release the shared IRQ line
and signal an NMI, restore itself to a power-on state on reset, and — for
devices that talk to the outside world — begin closing down its connection
when the emulator shuts down. See [Interrupt Support](#interrupt-support)
above for how the CPU combines IRQ/NMI signals from every device on the bus.

### Execution Tracing

The CPU can record every register snapshot and bus read/write to a compact
[binary trace format](appendix-trace-format.md) as it executes — writing is
offloaded to a background thread so recording does not slow down execution.
Two tools consume these traces:

- The `emma65` binary writes a trace directly to a file with `--trace-file`
- The debugger's Trace window records and displays a scrolling, live view of
  recent execution without stopping the CPU
- The standalone `emma65-tracer` binary decodes a previously recorded trace
  file into a disassembly listing, optionally annotated with symbols from a
  VICE label file and per-instruction bus operation detail
