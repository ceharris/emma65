# emma65

[![CI](https://github.com/ceharris/emma65/actions/workflows/ci.yml/badge.svg)](https://github.com/ceharris/emma65/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Emma65 is a software emulator for the 65C02-family of 8-bit microprocessors.
It provides a complete execution environment suitable for running and
debugging programs written for classic 65C02-based systems, with support for
flexible memory configuration, a rich set of virtual I/O devices, and
expression-based watchpoints. The project ships five tools built on the same
emulator core:

- **`emma65`** — a command-line emulator for running programs directly
- **`emma65-debugger`** — a graphical debugger (registers, disassembly,
  memory, stack, watchpoints, and a live execution trace, in a native desktop
  app) for interactively developing and troubleshooting programs
- **`emma65-tracer`** — a utility that decodes a recorded binary execution
  trace into a human-readable, symbol-annotated disassembly listing
- **`emma65-display`** — an SDL2 peripheral process that renders the
  character display device (`display`) in its own window when running
  `emma65` standalone (no debugger)
- **`emma65-led-matrix`** — an SDL2 peripheral process that renders the RGB
  LED matrix device (`display/matrix`) in its own window when running
  `emma65` standalone (no debugger)

Together they form a foundation for building retro-computing tools,
educational simulators, and hardware-in-the-loop test rigs.

## Documentation

The full documentation — installation, the debugger, the emulator core,
I/O devices, running the emulator and its peripherals, and a guide for
contributors adding new device types — is published as a wiki at
https://ceharris.github.io/emma65/. Its source lives in this
repository under `doc/`, built with [mdBook](https://rust-lang.github.io/mdBook/):

```bash
cargo install mdbook
mdbook build doc      # renders to doc/book/
mdbook serve doc      # or serve it locally with live reload
```
