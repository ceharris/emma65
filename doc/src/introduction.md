# Introduction

Emma6k is an emulator and debugger toolkit for classic 8-bit microprocessors.
It emulates the 65C02 family and the Motorola 6809. The `emma6k` emulator
runs either; `emma65` and `emma09` are shorter commands for one family each.
The debugger and tracer support the 65C02 so far. Tools that work with any
emulated processor use the `emma6k` name or prefix, while the 65C02-specific
tracer keeps the `emma65` name. It provides a complete execution environment suitable for running and
debugging programs written for classic 65C02-based systems, with support for
flexible memory configuration, a rich set of virtual I/O devices, and
expression-based watchpoints. The project ships six tools built on the same
emulator core:

- **`emma6k`** — a command-line emulator for running programs directly, with
  `emma65` (65C02) and `emma09` (6809) as shortcuts for one CPU family
- **`emma6k-debugger`** — a graphical debugger (registers, disassembly,
  memory, stack, watchpoints, and a live execution trace, in a native desktop
  app) for interactively developing and troubleshooting programs
- **`emma65-tracer`** — a utility that decodes a recorded binary execution
  trace into a human-readable, symbol-annotated disassembly listing
- **`emma6k-display`** — an SDL2 peripheral process that renders the
  character display device (`display`) in its own window when running
  `emma65` standalone (no debugger)
- **`emma6k-led-matrix`** — an SDL2 peripheral process that renders the RGB
  LED matrix device (`display/matrix`) in its own window when running
  `emma65` standalone (no debugger)
- **`emma6k-lcd-display`** — an SDL2 peripheral process that renders the
  character LCD device (`display/lcd`) in its own window when running
  `emma65` standalone (no debugger)

Together they form a foundation for building retro-computing tools,
educational simulators, and hardware-in-the-loop test rigs.
