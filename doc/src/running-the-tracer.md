# Running the Tracer

`emma6k-tracer` decodes a binary trace file into a human-readable,
disassembled instruction listing. You record one with `--trace-file <path>` on
`emma65`, `emma09` or `emma6k`, or with the debugger's Trace window. The trace
file says which CPU recorded it, so the tracer decodes 6502 and 6809 traces
alike.

```
emma6k-tracer [--output <path>] [--symbol-file <path>]... [--verbose] [<input>]
```

- `<input>` — path to the trace file; reads from stdin if omitted
- `--output <path>` — path to write decoded output; writes to stdout if omitted
- `--symbol-file <path>` — a VICE-format label file to resolve addresses to
  symbol names; may be repeated to load labels from multiple files
- `--verbose` — additionally print the bus reads and writes performed by each
  instruction

`emma6k-tracer` replaces `emma65-tracer`, which earlier releases shipped. It
reads their trace files too.

## Example

Record a CamelForth session, then decode it with CamelForth's labels:

```
emma09 --trace-file forth.trace
emma6k-tracer --symbol-file program.lbl forth.trace | less
```

The label file is the template's `program.lbl`: create a profile from the
`camelforth` template to get a copy. A trace grows quickly, by tens of
megabytes a second at full speed, so keep recordings short.

## Output

Each row shows the step's sequence number, its cycle count, the registers
just before it ran, and the instruction. For the 6502 the registers are A, X,
Y, S and P, with P's flags spelled out (`NV-BDIZC`):

```
   Seq#    Cyc A  X  Y  S  P  NV-BDIZC Addr   Code    Instruction / Operation
---------- --- -- -- -- -- -- -------- ---- -------- -------------------------
         1   2 00 00 00 FF 24      I   F000 78        SEI
         2   2 00 00 00 FF 24      I   F001 A2 00     LDX #$00    ; 0 ^@ (NUL)
```

For the 6809 they're A, B, X, Y, U, S, DP and CC, with CC's flags spelled out
(`EFHINZVC`):

```
   Seq#    Cyc A  B  X    Y    U    S    DP CC EFHINZVC Addr Code           Instruction / Operation
---------- --- -- -- ---- ---- ---- ---- -- -- -------- ---- -------------- -------------------------
         1   2 00 00 0000 0000 0000 0000 00 50  F I     F5CC 86 DE          LDA   #$DE      ; 222 (-34)
         2   6 DE 00 0000 0000 0000 0000 00 58  F IN    F5CE 1F 8B          TFR   A,DP
```

A label line, `NAME:`, precedes an instruction at a labeled address. On the
6809, a step that serviced an interrupt instead of running an instruction
shows the interrupt and the vector the CPU read, for example
`<IRQ> [$FFF8]`, at the address it interrupted.
