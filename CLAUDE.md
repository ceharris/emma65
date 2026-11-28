# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Agent memory

Keep all persistent agent memory for this project in `.claude/memory/` in this repository. It is
versioned with the code, so it travels between the user's workstations (Linux is the primary
development host; there's also a macOS workstation). Its index is imported here:

@.claude/memory/MEMORY.md

- **Never write project memory to the per-user auto-memory directory**
  (`~/.claude/projects/<path>/memory/`). If the harness's built-in memory instructions say to save
  there, save to `.claude/memory/` instead. If that per-user directory holds memories about this
  project, reconcile them into `.claude/memory/`: merge duplicates, keep the newer or more accurate
  fact, and record conflicts for the user rather than silently choosing one. Then leave only a
  pointer back to this directory there.
- **Format.** Store one fact per file, with frontmatter `name` (kebab-case slug, matching the file
  name), `description` (one line, used to judge relevance), and `metadata.type` (`user`,
  `feedback`, `project`, or `reference`). For `feedback` and `project` memories, follow the fact
  with **Why:** and **How to apply:** lines. Link related memories with `[[name]]`. Add a one-line
  pointer to `.claude/memory/MEMORY.md` for every file.
- **Maintenance.** Update an existing file rather than creating a duplicate, and delete memories
  that turn out to be wrong. Use absolute dates. Don't record what the code, git history, or this
  file already say.
- **Host-neutral by default.** When a fact only holds on one machine (benchmark numbers, platform
  gaps, local paths), say which host it applies to.
- **Commit memory with the work.** Include memory changes in the commits for the work they
  describe, so the other workstation picks them up. Uncommitted memory doesn't travel.
- **This repository is public.** Never store secrets, credentials, or personal details beyond what
  the repository already contains.

## Commands

```bash
cargo build              # build the default members: the emma65 library, and the emulator CLI
                          # (emma6k, its emma65/emma09 wrappers, and emma6k-tracer) from cli/
cargo build --workspace  # also build the debugger crate (emma6k-debugger), the display crate
                          # (emma6k-display), the led-matrix crate (emma6k-led-matrix), and the
                          # lcd-display crate (emma6k-lcd-display)
cargo build -p emma6k-display     # build just the SDL2 display peripheral (needs libsdl2-dev)
cargo build -p emma6k-led-matrix  # build just the SDL2 LED matrix peripheral (needs libsdl2-dev)
cargo build -p emma6k-lcd-display # build just the SDL2 LCD display peripheral (needs libsdl2-dev)
cargo test                # run the default members' tests: emma65's (library, tests/) and the CLI's
                          # (incl. cli/tests/emulator_binary.rs) — NOT emma6k-core's or emma09's
cargo test -p emma6k-core # run the core crate's tests
cargo test -p emma09      # run the 6809 crate's tests (unit tests, exec tests, Schwotzer's CPU test suite)
EMMA09_BLESS=1 cargo test -p emma09 every_opcode  # rewrite the 6809 disassembler's golden table,
                          # then check it: python3 emma09/src/disassembler/lwasm_check.py (needs lwasm)
cargo test --workspace    # every crate's tests: emma65, emma6k-core, emma09, emma6k, debugger, display, led-matrix, lcd-display
cargo test <name>         # run a single test by name (partial match)
cargo clippy              # lint (covers every workspace member, including emma6k-core)
cargo fmt --all           # format all Rust code in the workspace
```

Run `cargo fmt --all` before pushing a branch or opening a PR that touches any Rust code — CI runs
`cargo fmt --all -- --check` and fails the build on unformatted code.

The debugger's frontend (`debugger/frontend/`) is a separate React/TypeScript/Vite project.
Tauri invokes `npm run build` there automatically as part of `cargo tauri build` /
`cargo tauri dev`; it is not built by plain `cargo build`.

## Architecture

This repository is the emma6k project: a Cargo workspace (Rust 2024 edition) with nine members.
Crates that aren't specific to the 6502 use the `emma6k-` prefix; the 6502 emulator crate and its
binaries keep the `emma65` name.

- **`.`** (crate `emma65`) — the 6502 crate: the 65C02 CPU and everything specific to it, the
  `emma65::emulator` facade over the core
- **`cli`** (package `emma6k`) — the emulator command line: the `emma6k` binary, which chooses the
  CPU from `cpu-variant` (or its synonym `cpu`) and runs it, and the `emma65` and `emma09`
  wrappers, which exec `emma6k` with a hidden `--family` flag fixing the CPU family; and
  `emma6k-tracer`, which decodes either CPU's trace files (it replaced `emma65-tracer`). Versioned
  in lockstep with `emma65`
- **`core`** (crate `emma6k-core`) — the CPU-neutral emulator core every CPU family shares: bus,
  devices, transports, configuration framework, execution control behind the `Processor` trait,
  the trace file format (generic over a CPU's register record), and the watch pipeline. It never names a CPU-specific type, and the compiler enforces that.
  Versioned in lockstep with `emma65` and released only as part of an `emma65` release (see the
  release skill)
- **`emma09`** (crate `emma09`) — the Motorola 6809, built on `emma6k-core` alone: registers,
  opcode tables, indexed addressing, ALU, interrupts (NMI, FIRQ, IRQ, CWAI, SYNC), its
  `Processor` impl, tracing, the disassembler, and its `Config` and device registry (the built-ins
  minus `pic/finch`). `emma09`/`emma6k --cpu 6809` run it, with the GPL-3 CamelForth template as
  its default; `plan/multi-cpu-step2-mc6809-plan.md` built it. Versioned in lockstep with `emma65`, like `emma6k-core`
- **`build-info`** (crate `emma6k-build-info`) — a `build.rs`-only helper, shared as a
  build-dependency by the `build.rs` of each member with binaries (`cli`, `debugger/src-tauri`,
  and the three SDL2 crates; the library-only `emma65`, `emma6k-core` and `emma09` have none), that captures each crate's git commit
  hash, build timestamp, and (for a tagged release commit) version number into `BUILD_VERSION`/
  `BUILD_INFO_LINE` env vars consumed via `env!` — see `build-info/src/lib.rs`'s doc comment and
  issue #623
- **`debugger/src-tauri`** (crate `emma6k-debugger`) — a Tauri 2 desktop app that hosts the
  emulator and exposes a full-featured debugger UI
- **`display`** (crate `emma6k-display`) — an SDL2 peripheral binary that renders the
  `display` device's composited output for the plain `emma65` CLI (the debugger renders
  it in-process instead); spawned by the emulator as a child process over a `pipe:` transport,
  per `plan/char-display-external-protocol.md`
- **`led-matrix`** (crate `emma6k-led-matrix`) — an SDL2 peripheral binary that renders the
  `display/matrix` device's per-matrix composited output for the plain `emma65` CLI (the
  debugger renders it in-process instead); spawned by the emulator as a child process over a
  `pipe:` transport, per `plan/led-matrix-external-protocol.md`
- **`lcd-display`** (crate `emma6k-lcd-display`) — an SDL2 peripheral binary that renders the
  `display/lcd` device's composited dot-matrix output for the plain `emma65` CLI (the debugger
  renders it in-process instead); spawned by the emulator as a child process over a `pipe:`
  transport, per `plan/lcd-display-external-protocol.md`

`emma6k-core`'s modules sit at its crate root: `emma6k_core::{bus, config, device, error, exec,
session, transport, watch, location, text}`, plus root re-exports (`Bus`, `Processor`,
`MachineState`, `log_channel`, the `log_msg!` macro, …).

The `emma65` library exposes three top-level public modules:

- **`emulator`** (`src/emulator.rs`) — a facade: `pub use emma6k_core::*`, plus the 6502's public
  types under their pre-split paths (`emma65::emulator::Cpu`, `emma65::emulator::cpu::StepResult`,
  `emma65::emulator::Config`, `emma65::emulator::config::templates`, …). The debugger, the
  peripheral crates, the binaries, and `tests/` all use these paths. New code inside `emma65` should
  name `emma6k_core` and `crate::mos6502` directly.
- **`mos6502`** — everything specific to the 65C02: the CPU and its `Processor` impl, the binary
  execution trace, the assembler and disassembler, the top-level `Config` schema, and the bundled
  templates. `emma65::assembler` and `emma65::disassembler` are re-exports of its submodules. Its
  `exec_tests` and `device_tests` modules hold the 6502-driven tests of core code.
- **`watch`** — a re-export of `emma6k_core::watch`, the watchpoint expression pipeline

A CPU implemented in another crate builds on `MachineState`'s public accessors and per-step helpers
(`check_halt`, `finish_cycle`, `tick_idle`, `irq_recognized`); `emma09` is the real example, and
`emma6k_core::exec::toy` (test-only) a minimal one. **Cross-crate inlining:** the 6502 calls into core on every instruction, and
a non-generic core function only inlines across the crate boundary if it is `#[inline]`. Keep the
hot `Bus`, `InterruptController`, `MachineState`, `LogSender::set_cycles`, and
`WatchEvaluator::is_empty` paths `#[inline]`; removing one cost ~7% in the PR 4 benchmark.

### Crate structure

```
src/                        — crate emma65
  lib.rs                    — pub mod emulator, mos6502; re-exports watch, assembler, disassembler
  emulator.rs               — the emma65::emulator facade (emma6k_core + 6502 re-exports)
  mos6502/                  — everything specific to the 65C02
    cpu/                    — Cpu, CpuBuilder, Registers, Processor impl, opcode decode, ALU,
                               status register, variant, bus-access trace recording (cpu/trace.rs:
                               the Registers record codec and aliases of core's trace types);
                               StepResult/RunHandle/CpuLiveSnapshot aliases
    config.rs               — Config (top-level TOML/CLI schema), CpuVariantSpec, Config::build
    templates/              — the bundled starter-profile templates (see below)
    disassembler/           — Disassembler, and disassembler/trace (reconstructs disassembly
                               lines from a recorded trace stream)
    assembler/              — 65C02 assembler
    exec_tests.rs, device_tests.rs — 6502-driven tests of core exec and devices
core/src/                   — crate emma6k-core, the CPU-neutral core
  lib.rs                    — module declarations and the root re-export surface
  processor.rs              — Processor trait, HaltState
  machine.rs                — MachineState: bus, interrupts, watch evaluator, breakpoints, clock
                               speed, cycles, vector resolver, log sender; accessors and per-step
                               helpers
  bus/                      — Bus, BusConfig, address regions, DeviceIdAllocator, symbol
                               table (VICE label loading), bus loader, Interrupt controller,
                               IrqSource, VectorResolver (bus/vector.rs)
  device/                   — IoDevice trait, built-in devices, device/protocol (VIA and PTM
                               peer-communication message encoders/decoders)
  error.rs                  — BusConfigError, BusError, CpuBuildError, ExecError
  exec/                     — generic over P: Processor: StepResult<Op>, CpuLiveSnapshot<R>,
                               RunHandle<P>, RunStopper, ClockSpeed, run(), run_from(),
                               step_into(), step_over_subroutine(), step_over_breakpoint(),
                               step_return(); exec/toy.rs is a test-only toy Processor
  transport/                — Transport trait, ChannelRelay, TransportRelay, TransportReporter,
                               InternalPipeTransport, PipeTransport, TcpSocketTransport,
                               UnixSocketTransport, PtyTransport
  session.rs                — EmulatorSession<P> (owns the CPU, ErrorReceiver, DeviceIdAllocator)
  config/                   — device spec/module registry, build_machine, template mechanism
                               (see below)
  logging.rs                — LogSender, log writer/collector threads, log_channel, log_msg!
  trace.rs                  — the trace file format (v3 header, fixed-length records; reads v2),
                               TraceRegisters (a CPU's record codec), TraceRecord/TraceKind,
                               TraceCallback, BinaryTraceWriter/Reader, spawn_trace_writer,
                               TraceState — all generic over the CPU's Registers
  watch/                    — watchpoint expression pipeline (see below)
  location.rs, text.rs      — source-text helpers shared by watch and CPU assemblers; text.rs also
                               has immediate_comment, the disassemblers' 8-bit operand comment
cli/                        — package emma6k, the emulator command line
  src/bin/emma6k/main.rs    — main, run_6502/run_6809, and run_session<P: Processor> (the console,
                               logging, tracing and run loop, compiled once per CPU)
  src/bin/emma6k/config.rs  — CliArgs, AppConfig (CPU-neutral keys), per-source cpu/cpu-variant
                               checks, LoadedConfig::resolve → RunPlan (family, variant, template)
  src/bin/emma6k/family.rs  — Family (6502, 6809): variant → family, wrapper names, templates
  src/bin/emma6k/tty.rs     — terminal raw-mode helpers for the CLI console
  src/bin/emma65.rs, src/bin/emma09.rs, src/wrapper.rs — the wrappers (exec emma6k --family …)
  src/bin/emma6k-tracer/    — emma6k-tracer: main.rs reads the header's family and dispatches to
                               mos6502.rs or mc6809.rs (each family's decode loop and column
                               format); columns.rs is the shared fixed-column row helper
  tests/emulator_binary.rs  — subprocess tests of emma6k and the wrappers
emma09/                     — crate emma09, the Motorola 6809
  src/cpu/                  — Cpu, CpuBuilder, Registers, Processor impl, interrupt servicing,
                               big-endian watch context; opcodes.rs (per-page opcode tables with
                               base cycles), indexed.rs (postbyte forms), execute.rs (semantics,
                               ALU, stacking), cc.rs (condition codes), trace.rs (the 24-byte
                               Registers record, family 1). The bus-touching methods take a
                               `const TRACE: bool`, so the untraced step has no trace checks
  src/disassembler/         — Disassembler (Motorola syntax), instruction_len, and trace/ (the
                               TraceRowAssembler; interrupt rows have no line, see vector());
                               every_form.txt is the golden every-opcode table, lwasm_check.py its
                               round-trip check
  src/config.rs             — Config (cpu-variant "6809"), CpuVariantSpec, INTERRUPT_LINES (IRQ,
                               FIRQ, NMI), device_registry(), Config::build → EmulatorSession
  src/templates/            — TEMPLATES and DEFAULT_TEMPLATE; camelforth/ holds the GPL-3.0-or-later
                               CamelForth ROM (program.bin/.lbl, emulator-template.toml), its Forth
                               source as a text master (forth/chromium_ans.txt), README.md (license
                               and changes), and build.sh + tools.py (needs GForth 0.7.0–0.7.3)
  tests/camelforth.rs       — boots CamelForth via Config::build and drives its console
  src/exec_tests.rs         — exec (run, step-over, step-out) driving the 6809
  tests/schwotzer_6809.rs   — W. Schwotzer's MC6809 CPU test suite; its GPL-2.0 source, binary,
                               FLEX shim and license are in tests/roms/ (test-only, never shipped)
debugger/
  src-tauri/                — emma6k-debugger: Tauri commands/state per UI panel (see below)
  frontend/                 — React/TypeScript/Vite UI (panels, styles, keybindings)
```

`util/via_sr_peripheral.py` is a standalone Python script that emulates a VIA shift-register
peripheral over a transport, useful for manually exercising `Via6522`'s shift-register modes.

---

### `emulator::config` module (`core/src/config/`)

Multi-source configuration (TOML < `EMMA6K_*` env vars < CLI args) via `figment` + `clap`, in
`emma6k` (`cli/src/bin/emma6k/config.rs`) and the debugger. There is no `EMMA65_` fallback.

Building is split in two. The core's `config::build_machine(&registry, MachineSpec, context)`
iterates the devices, dispatches each to its `DeviceModule`, builds the `Bus`, and returns a
`BuiltMachine` (a `MachineState` plus the `ErrorReceiver` and `DeviceIdAllocator`). The 6502's
`Config::build(&registry)` (`src/mos6502/config.rs`; also at `emma65::emulator::Config`) calls
it, wraps the `MachineState` in a `Cpu` via `CpuBuilder::machine`, and returns an
`EmulatorSession`. `Config` lives on the 6502 side because its `cpu-variant` field is a 6502
type.

`DeviceSpec::from_str` format: `type@address[,key=value,...]`
- Address: decimal, `0x`/`0o`/`0b` prefix
- Size: bytes or `K`/`k` suffix
- Transport: `tcp:PORT`, `tcp:IP:PORT`, `unix:PATH`, `pty`, `pty:SYMLINK`

**Templates.** The CPU-neutral mechanism (`Template`, `TemplateError`, `find_in`,
`materialize_from`, and the asset writer) is `emulator::config::template`
(`core/src/config/template/`). The 6502's entries are **`mos6502::templates`
(`src/mos6502/templates/`)**, also reachable as `emma65::emulator::config::templates` — a
registry of bundled "starter profile" templates, each its own subdirectory (`taliforth/`, `msbasic/`,
`ehbasic/`, `rain/`, `lcd/`, `snake/`) holding a checked-in device-layout template
(`emulator-template.toml`), a ROM image (`program.bin`), and its VICE labels (`program.lbl`),
embedded via `include_bytes!`/`include_str!`. `TEMPLATES` lists all of them (id, display name,
description); `materialize(id, dest)` renders one (filling in its `{{ROM_IMAGE}}`/`{{LABELS}}`
placeholders) into `dest`, returning the path to the written `emulator.toml`.
`materialize_default(dest)` is shorthand for the `"taliforth"` template, the registry's
`DEFAULT_TEMPLATE` — the single source of truth for the emulator's zero-config layout: 32K
zero-filled RAM at `0x0000`, the TaliForth ROM at `0x8000`, a VIA on a Unix-socket transport, an
R6551 ACIA on `~/.emma/dev/ttyS0` and an MC6850 ACIA on `~/.emma/dev/ttyS1` (both PTYs), an LFSR,
and a console with `Ctrl+C` as its break key — WDC65C02 at 1.8432 MHz. `emma6k` merges every
family's template list (`cli/src/bin/emma6k/family.rs`); a template's `cpu-variant` chooses its
family. It materializes a family's default template into a tempdir when no devices are configured,
or any named template via `--profile <id>` (`LoadedConfig::resolve` and `apply_template` in
`cli/src/bin/emma6k/config.rs`, loaded through the normal `Toml::file()` path); the 6809's default
is `camelforth`. The debugger materializes the default into `~/.emma/debugger/profiles/default/` the first
time that profile directory is created, and offers the full template list in its New Profile
dialog (see `profile::ensure_profile_dir`/`list_templates` below).

---

### `emulator::bus`

`BusConfig` is a builder. Regions are added with `.ram()`, `.rom()`, `.device()`, then
`.build()` produces a `Bus`. Most-specific-wins overlap: smaller regions shadow larger
ones at the same addresses. Ambiguous same-size overlaps are caught at build time.

`DeviceIdAllocator` hands out `DeviceId`s that don't collide with configured devices;
`EmulatorSession` exposes it post-configuration so hosts (e.g. the debugger UI) can
allocate additional IDs for UI-driven controls (like an IRQ toggle) safely.

`symbol::load_vice_labels` loads a VICE-format label file into a `SymbolTable`, used by
the disassembler, trace tooling, and the debugger to resolve addresses to names.

---

### `mos6502::cpu` (also `emulator::cpu`)

`Cpu::builder(variant)` → `CpuBuilder` → `Cpu`. Two variants: `Cmos65C02` and
`Wdc65C02`. The builder accepts a `ClockSpeed` and a `Bus`, or a ready-made `MachineState` via
`.machine()`.

`Cpu` implements `Processor` and embeds a `MachineState`. It keeps inherent accessors (`bus()`,
`breakpoints()`, …) that duplicate the trait's defaults, so concrete callers don't need
`Processor` in scope. `Cpu::step()` returns `StepResult` (an alias for
`exec::StepResult<DecodedOp>`). `exec::run()` and friends (below) drive execution.

`cpu::trace` is bus-access tracing. The machinery is core's `emma6k_core::trace`, generic over a
CPU's `TraceRegisters`; `cpu::trace` supplies the 6502's 16-byte `Registers` record (family 0) and
aliases (`TraceKind`, `TraceRecord`, `BinaryTraceWriter<W>`, …) under their old paths.
`TraceCallback<Registers>` receives `TraceRecord`s (`TraceKind::Registers | Read | Write | Cycles`)
as the CPU executes; `BinaryTraceWriter`/`BinaryTraceReader` persist them to/from a versioned
binary format (magic `E65T`, format version 3: the header carries the variant, the CPU family and
the record length; version 2 files still read, and the 6502's records are the same in both);
`spawn_trace_writer` runs the writer on a background thread fed by a channel, with a configurable
`OverflowPolicy`. This is the backbone for both `emma6k-tracer` and the debugger's live trace
window.

---

### `emulator::device`

All built-in devices implement `IoDevice`:

```rust
fn read(&mut self, offset: u16) -> u8;
fn write(&mut self, offset: u16, value: u8);
fn peek(&self, offset: u16) -> u8;   // side-effect-free (watchpoints, disassembler)
// optional: tick(), irq_active(), take_nmi(), take_request(), is_passive(), name()
```

**Device requests and passive devices.** `take_request()` returns a `CpuRequest` (`Reset`, or
`Exit(status)`). The bus asks for it right after each write (or patch) to the device, never in
the per-step poll, and keeps it for `Bus::take_written_request`, which `MachineState`'s per-step
poll latches into the `InterruptController`. Each CPU takes the pending request once per step
(`take_request`): `Exit` stops it as STP does and records `MachineState::exit_status`, which
`emma6k` uses as its process exit code. A device whose `is_passive()` is true never ticks or
interrupts; `BusConfig::build` moves passive devices behind the polled ones, so the per-step
tick and poll skip them at no cost. The `exit` device (`device::ExitDevice`: write `EXIT`, then
the status) is the one built-in passive device. Keep both rules: polling requests per device and
ticking a do-nothing device each cost measurable speed.

Devices that need byte-stream I/O hold a `TransportRelay` connected to
an `Option<Box<dyn Transport>>`; the VIA and MC6840 additionally support a structured
peer-communication protocol (ASCII or binary framing) implemented in `device::protocol`
(`device::protocol::via`, `device::protocol::ptm`) for exchanging port/pin state with real or
emulated peripherals over that transport.

---

### `emulator::exec`

Everything here is generic over `P: Processor`, and the run loop is compiled once per CPU, so
`step()` stays a static call: never put a `dyn Processor` on the hot path.

`run()`/`run_from()` drive a free-running loop (optionally clock-throttled) on a background
thread, returning a `RunHandle`/`RunStopper` pair for stopping it and reading a live
`CpuLiveSnapshot` (registers, the stack window `Processor::stack_window` reports, cycle counts,
IRQ/NMI/STP/WAI status, a memory page) without pausing the CPU. A run stopped on request ends with
`StepResult::StopRequested`. `step_into()`, `step_over_subroutine()`, `step_over_breakpoint()`, and
`step_return()` implement the debugger's single-step, step-over, and step-out commands.

---

### `emulator::transport`

`Transport` is the byte-stream abstraction; implementations: `PipeTransport`,
`InternalPipeTransport`, `TcpSocketTransport`, `UnixSocketTransport`, `PtyTransport`. TCP and
Unix socket transports listen for incoming connections; PTY creates a pseudoterminal.
`TransportRelay`/`ChannelRelay` decouple a device's synchronous `tick()` from the transport's
async I/O task; `TransportReporter` surfaces connect/disconnect/error events as `DeviceEvent`s
(consumed via `device_event_channel()`).

---

### `watch` module (`core/src/watch/`)

A self-contained pipeline for evaluating watchpoint expressions against live machine state.

```
source &str → Scanner → Vec<Token> → Parser → Expr tree → Compiler → Vec<OpCode> → Evaluator → Operand (u32)
```

`WatchCompiler::new(map_register, map_flag, map_symbol)` — owns a `Parser`.
`compiler.compile(source, evaluator)` → `Watchpoint` (stores `Vec<OpCode>`).
`WatchEvaluator::new()` — owns watchpoints, `Variables`, and variable runtime storage.
`evaluator.evaluate_all(context)` → `Ok(Some(index))` | `Ok(None)` | `Err((index, err))`.

#### Submodules

- **`text`** — zero-copy cursor over `&str`; `consume()` returns `[start..current]`
- **`location`** — source position tracking used by tokens and diagnostics
- **`scanner`** — tokenizes source; handles `0x`/`$`/`0o`/`0q`/`0b`/decimal literals
- **`token`** — `Token<'a>` with `TokenType` and an `&'a str` text slice
- **`expr`** — `Expr<'a>` AST: leaf nodes (Number, Register, Flag, Variable), Assign (walrus), UnaryOperator (includes Fetch), BinaryOperator; `signed: bool` field
- **`variables`** — `Variables` maps names to stable `Operand` IDs via `get_or_create`
- **`parser`** — recursive descent; precedence: `:=` → `||` → `&&` → `|` → `^` → `&` → `==` → relational → shift → `+/-` → `*/` → unary → primary
- **`compiler`** — depth-first `Expr` traversal → flat `Vec<OpCode>`; signedness selects opcode variant
- **`evaluator`** — stack VM over `&[OpCode]` against `&dyn WatchContext` and `&mut [Operand]`
- **`context`** — `WatchContext` trait: `read_register_u32/i32`, `read_flag`, `read_mem_u32/i32`
- **`session`** — high-level `WatchCompiler` + `WatchEvaluator` API

#### Domain-specific operators

- `B[addr]`, `W[addr]`, `D[addr]` — byte/word/dword memory fetch; leading `+`/`-` controls signedness
- `` `flagname `` — reads a named CPU status flag
- `:=` — walrus: assigns RHS to a named variable and yields its value; variables persist across `evaluate_all` calls
- `$hex` — hexadecimal literal shorthand

#### Lifetime threading

`Token<'a>` and `Expr<'a>` borrow from the source `&'a str`. After `compiler::compile`
consumes the tree, the resulting `Vec<OpCode>` (stored in `Watchpoint`) has no lifetime
parameters and can be stored freely.

---

### Peripheral crates

The `debugger`, `display`, `led-matrix`, and `lcd-display` crates each have their own CLAUDE.md
(`debugger/src-tauri/CLAUDE.md`, `display/CLAUDE.md`, `led-matrix/CLAUDE.md`,
`lcd-display/CLAUDE.md`) with crate-specific detail, loaded automatically when working under
those directories.
