# Trace File Format

The binary format the emulator writes, and the debugger and tracer read, when
[execution tracing](the-emulator-core.md#execution-tracing) is active. It's
reference material for writing an independent trace file reader (e.g. a
script or alternate viewer) — the bundled `emma6k-tracer` binary already
decodes it into disassembly listings (see [Running the Tracer](running-the-tracer.md)),
and the debugger's Trace window reads it live without needing to touch this
format directly.

A trace file is a fixed 8-byte header followed by a sequence of fixed-length
records, all little-endian, with no trailer. The header says which CPU family
recorded the file and how long its records are: 16 bytes for the 6502, 24
bytes for the 6809.

## Header

| Offset | Length | Field | Description |
|--------|--------|-------|--------------|
| 0 | 4 | magic | ASCII `E65T` |
| 4 | 1 | format version | Currently `3`. Readers also accept `2` (see [Version 2](#version-2)). |
| 5 | 1 | CPU variant | Numbered within the family (below) |
| 6 | 1 | CPU family | `0` = 6502, `1` = 6809 |
| 7 | 1 | record length | Bytes per record: `16` for the 6502, `24` for the 6809 |

The CPU variants:

| Family | Variant byte | Variant |
|--------|--------------|---------|
| 6502 (`0`) | `0` | `Cmos65C02` |
| 6502 (`0`) | `1` | `Wdc65C02` |
| 6809 (`1`) | `0` | MC6809 |

The 6502 variant matters to a decoder because `Wdc65C02` adds 34 opcodes
(`STP`, `WAI`, `BBR0`–`7`, `BBS0`–`7`, `RMB0`–`7`, `SMB0`–`7`) beyond the base
`Cmos65C02` set.

## Records

Every record has the same frame; only its length varies by family:

| Offset | Length | Field | Description |
|--------|--------|-------|--------------|
| 0 | 8 | `instr_id` | u64. See [Instruction correlation](#instruction-correlation) below. |
| 8 | 1 | tag | `0` = Registers, `1` = Read, `2` = Write, `3` = Cycles |
| 9 | record length − 9 | payload | Tag-dependent, zero-padded (7 bytes for the 6502, 15 for the 6809) |

A reader determines a record's meaning from the tag byte alone; the payload
layout is fixed per tag (and, for Registers, per family), not length-prefixed.

### Registers (tag 0)

A snapshot of all CPU registers taken immediately before the instruction
identified by `instr_id` began executing. Emitted once per instruction, as
the first record for that `instr_id`. A step that never touches the bus (one
stopped at a breakpoint, say) has no records at all.

For the **6502**:

| Payload offset | Length | Field |
|-----------------|--------|-------|
| 0 | 1 | `A` |
| 1 | 1 | `X` |
| 2 | 1 | `Y` |
| 3 | 1 | `S` |
| 4 | 2 | `PC` (u16) |
| 6 | 1 | `P` (status register, see below) |

The `P` byte is the eight processor status flags packed as
`N V - B D I Z C` (bit 7 down to bit 0), where `-` is the always-set UNUSED
bit — the same encoding pushed to the stack on real hardware:

| Bit | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|-----|---|---|---|---|---|---|---|---|
| Flag | N | V | UNUSED | B | D | I | Z | C |

For the **6809**:

| Payload offset | Length | Field |
|-----------------|--------|-------|
| 0 | 1 | `A` |
| 1 | 1 | `B` |
| 2 | 1 | `DP` |
| 3 | 1 | `CC` (condition codes, see below) |
| 4 | 2 | `X` (u16) |
| 6 | 2 | `Y` (u16) |
| 8 | 2 | `U` (u16) |
| 10 | 2 | `S` (u16) |
| 12 | 2 | `PC` (u16) |

The 16-bit registers are little-endian like the rest of the file, even though
the 6809 itself is big-endian. `CC` is the condition-code register as the CPU
stacks it:

| Bit | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|-----|---|---|---|---|---|---|---|---|
| Flag | E | F | H | I | N | Z | V | C |

### Read (tag 1) / Write (tag 2)

A single bus access performed while executing the instruction identified by
`instr_id`. A record is emitted for every bus read and write the CPU
performs — operand fetches, opcode fetches, stack pushes/pops, and each
device access — never for a device's side-effect-free `peek`.

| Payload offset | Length | Field |
|-----------------|--------|-------|
| 0 | 2 | `addr` (u16) |
| 2 | 1 | `value` |
| 3 | rest | (padding) |

### Cycles (tag 3)

The total clock cycle count for the instruction identified by `instr_id`
(base cycles plus any addressing-mode or branch-taken extra cycles), known
only once the instruction has finished executing. Emitted once per
instruction, as the last record for that `instr_id` — after every
`Registers`, `Read`, and `Write` record sharing that id.

| Payload offset | Length | Field |
|-----------------|--------|-------|
| 0 | 1 | cycle count |
| 1 | rest | (padding) |

## Instruction correlation

`instr_id` is a monotonically increasing counter (wrapping on overflow,
which in practice never happens) assigned once per CPU step. Every
record belonging to the same step — its `Registers` snapshot, each
bus `Read`/`Write` it performs, and its final `Cycles` total — shares the
same `instr_id`, in that relative order, though other steps' records
never interleave with them since the CPU executes one instruction to
completion before the next begins. A reader reconstructing a
per-instruction view (as `emma6k-tracer` does) can therefore group records
by run of equal `instr_id`.

The instruction's own bytes are its first reads, in address order from the
`PC` in its Registers record. On the 6809, a step that services an interrupt
fetches no instruction: its records are the stacking writes, then the two
reads of the interrupt's vector (for example `FFF8`/`FFF9` for IRQ).

## Seeking

Because every record has the header's record length, the byte offset of record
`n` (0-based, counting from the first record after the header) is
`8 + n * record length`. A reader with a seekable source can jump directly to
any record without scanning from the start — this is how the debugger's Trace
window serves windowed reads over a large live trace file.

## Version 2

Version 2 files predate the 6809. They're 6502 traces: their header bytes 6
and 7 are zero, and their records are 16 bytes, laid out exactly as version
3's 6502 records. A reader treats a version 2 file as family `0` with 16-byte
records.
