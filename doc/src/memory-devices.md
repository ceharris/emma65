# Memory Devices

Programs need somewhere to live and somewhere to work — a plain `ram` region
and a plain `rom` region are usually enough for that. Some real
single-board-computer designs go further, though, using a bank-switching MMU
to give a 64 KB 6502 address space access to a much larger pool of physical
memory; the Finch, Phoebe, and Vireo devices emulate three such designs.

Every memory device is placed on the bus with a TOML `[[devices]]` table —
`type` selects the device (shown in parentheses in each heading below) — or
the equivalent `--device type@address,key=value,...` CLI flag; see
[Running the Emulator](running-the-emulator.md) for the general TOML/CLI/env
conventions shared by every device type.

## RAM (`ram`)

A plain block of read/write memory mapped into any address range on the
bus.

```toml
[[devices]]
type = "ram"
address = 0x0000
size = 32768               # or the quoted string "32K"
```

- `size` (required) — how much address space the region occupies, either a
  plain integer number of bytes or a quoted string with a `K`/`k` suffix for
  kibibytes (e.g. `"32K"`). The suffixed form must be a TOML string —
  `size = 32K` without quotes is invalid TOML, not a valid size.
- `image` (optional, path) — a binary, Intel Hex, or Motorola S-Record file
  loaded at `offset` (default `0`) within the region at startup; see
  [Running the Emulator](running-the-emulator.md) for the recognized file
  extensions. Bytes the image doesn't cover fall back to `fill`.
- `fill` (optional, byte) — value used to initialize memory the image
  doesn't cover; omitted entirely (and no `image` given), the region starts
  with random contents, mimicking real RAM's power-on state.
- `offset` (optional, signed integer, default `0`) — byte offset within
  `image` (or, if negative, before it) at which loading begins.
- `labels` (optional, path) — a VICE-format label file, for symbol
  resolution in the debugger and tracer.

## ROM (`rom`)

A block of read-only memory mapped into any address range on the bus.
Writes are silently discarded.

```toml
[[devices]]
type = "rom"
address = 0x8000
size = 32768
image = "~/roms/my.bin"    # .bin, .rom, .hex, .ihx, .ihex, .s19, .srec
```

- `size` (required) — how much address space the region occupies, either a
  plain integer number of bytes or a quoted string with a `K`/`k` suffix for
  kibibytes (e.g. `"32K"`). The suffixed form must be a TOML string —
  `size = 32K` without quotes is invalid TOML, not a valid size.
- `image` (required, path) — a binary, Intel Hex, or Motorola S-Record file
  loaded at `offset` (default `0`) within the region at startup; see
  [Running the Emulator](running-the-emulator.md) for the recognized file
  extensions.
- `fill` (optional, byte) — value used to initialize any bytes the image
  doesn't cover.
- `offset` (optional, signed integer, default `0`) — byte offset within
  `image` (or, if negative, before it) at which loading begins.
- `labels` (optional, path) — a VICE-format label file, for symbol
  resolution in the debugger and tracer.

## Bank-Switched Memory Modules

Finch, Phoebe, and Vireo are complete memory subsystems — RAM, ROM, and a
bank-switching MMU — rather than plain `ram`/`rom` regions, each modeled on a
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

```text
                                    6 5 0 2   A d d r e s s   B u s
                    A15 A14 A13 A12 A11 A10  A9  A8  A7  A6  A5  A4  A3  A2  A1  A0
                      │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │
  ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓  │   │   │   │   │   │   │   │   │   │   │   │
  ┃   MMU Bank Registers (0..15)   ┃  │   │   │   │   │   │   │   │   │   │   │   │
  ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛  │   │   │   │   │   │   │   │   │   │   │   │
      │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │
     B7  B6  B5  B4  B3  B2  B1  B0   │   │   │   │   │   │   │   │   │   │   │   │
      │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │   │
    M19 M18 M17 M16 M15 M14 M13 M12 M11 M10  M9  M8  M7  M6  M5  M4  M3  M2  M1  M0
                      E f f e c t i v e   M e m o r y   A d d r e s s
```

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
