# Multi-CPU support, step 1: a CPU-agnostic core behind a `Processor` trait

## Context

We want to add the Motorola 6809 (and possibly the 6309, 6800, 6801, and 6803) without forking
the project. The bus, the `IoDevice` model, the transports and relays, the device protocols,
the configuration and device registry, the watch pipeline, and the peripheral crates are
already CPU-agnostic. Most of that work exists to keep the core loop from being blocked by
peripheral activity, and every future CPU should share it.

The overall path has three steps:

1. **This document.** Do a refactor that changes no behavior and keeps the 6502 as the only CPU.
   It introduces a `Processor` trait, makes `exec` generic over it, separates core code from
   6502 code, and finishes with the core moved into its own crate. The result is proven by the
   existing test suite plus the clock-speed benchmark.
2. Add a 6809 crate for the CLI only. This adds a `cpu = "6809"` config key, a choice of
   session made once at startup, more interrupt lines (FIRQ/HALT), a per-variant trace
   register record (trace format v3), and a 6809 starter template.
3. Generalize the debugger. The backend sends register descriptions, and the register,
   stack, disassembly, and assembler panels are driven by the processor.

Step 1 is finished when all of the following hold:

- Every existing test passes unchanged, apart from mechanical import and path updates.
- The `emma65` CLI, `emma65-tracer`, and the debugger behave exactly as before.
- Clock-speed throughput matches the baseline below within run-to-run noise.
- The core crate compiles without any reference to 6502 types.

## Design constraints

- **No dynamic dispatch on the hot path.** `run_loop` becomes `run_loop<P: Processor>`, so each
  CPU gets its own compiled copy and `cpu.step()` stays a static call. Never put a
  `Box<dyn Processor>` (or a `dyn` hop per instruction) in the run loop. PR #643 showed that
  even a dynamic hop on memory access costs measurable throughput, and #644 had to reverse it.
- **`Bus` stays concrete.** The special-cased RAM/ROM fast path in `Bus::read`/`write`/`peek` is
  not touched. CPUs reach the bus as `&mut Bus`, not through a trait.
- **Batch-granular work stays batch-granular.** Interrupt-command draining (once per
  `BATCH_SIZE` batch), snapshot publishing, and throttling stay where they are in the loop. The
  trait adds no per-instruction calls beyond the existing `step()`.
- **No behavior change.** The only change to the public API's meaning is the explicit
  clean-stop variant described in PR 2, and it is equivalent for every current caller.

## Performance baseline (this machine)

These figures were measured on 2026-11-26 on an Intel Core i9-9880H, with a release build at
`afba63f`, using the `clock-speed-benchmark` skill:

| Device complement | Throttling accurate up to | Unthrottled top speed |
|---|---|---|
| Bundled default | ~26 MHz | 27.4–28.1 MHz |
| Minimal (RAM, ROM, console) | ~50–54 MHz | 53.8–55.8 MHz |

The skill's default minimal sweep range (70–100 MHz) sits entirely above this machine's top
speed. For the minimal sweep, re-bracket the copied `examples/clock_bench_minimal.rs` to roughly
30–56 MHz. Run each sweep at least twice: results near the top speed are noisy on this laptop.
A change counts as a regression only if it is consistently more than about 5–10% below these
figures.

## Where the 6502 is baked into shared code today

These are the things this step has to move or generalize:

| Location | 6502 assumption |
|---|---|
| `src/emulator/cpu/mod.rs` `Cpu` | One struct mixes shared machine state (bus, interrupt controller, watch evaluator, breakpoints, clock speed, cycles, trace callback, vector resolver, log sender) with 6502 state (registers, decode table, variant, invalid-opcode policy, WAI/STP flags). |
| `Cpu::step` (`cpu/mod.rs:295`) | The breakpoint/watch check before each instruction is shared logic, but it lives inside the 6502's `step`. |
| `src/emulator/exec/mod.rs` | `CpuLiveSnapshot.registers: Registers`; `stack_page` is hard-wired to `0x0100–0x01FF` (`build_live_snapshot`); step-over relies on `JSR_OPCODE`/`JSR_BYTE_LEN`; `step_return` uses an 8-bit wrapping comparison on `S`; a clean stop is reported as a fake `NOP` (`0xEA`, `Mnemonic::Nop`). |
| `StepResult` (`cpu/mod.rs:66`) | `Executed(DecodedOp)` carries the 6502's `Mnemonic`/`AddressingMode`. |
| `EmulatorSession` (`session.rs`) | `pub cpu: Cpu`. |
| `Config::build` (`config/emulator.rs`) | Builds the bus and the 6502 `Cpu` in one function; `CpuVariantSpec` only knows about 65C02 variants. |
| `map_register_name`/`map_flag_name`, `CpuWatchContext` | 6502 register and flag names, and little-endian `W[]`/`D[]` fetches. This is already correctly behind `WatchContext`, so it only needs to move. |
| `NMI_VECTOR`/`IRQ_VECTOR`/`RESET_VECTOR` | 6502 constants. The `VectorResolver` trait itself is already CPU-neutral, since it takes an address. |
| `cpu/trace.rs` | `TraceKind::Registers(Registers)`, the v2 record layout, and the header variant tag are all 6502-specific. |
| `src/disassembler`, `src/assembler` | 6502 ISA. |
| `config/templates/*` | 6502 ROM images. |

What is already CPU-agnostic and stays put: `bus` (apart from the vector constants),
`device`, `transport`, `config` (apart from CPU construction and the variant spec), `watch`,
`logging`, and `SymbolTable`.

The deliberate exception is trace. Making it generic would ripple into
`BinaryTraceWriter`/`Reader`, `ChannelTraceCallback`, `TraceDisassembler`, the tracer binary, and
the debugger's trace window. In step 1, trace stays a 6502 concern that moves along with the
6502 code, format v2 unchanged. A per-variant register record (v3) belongs in step 2, which is
the first time a second layout exists.

## The `Processor` trait

Here is a sketch. Exact names get settled in PR 2.

```rust
/// Machine state every CPU shares: everything `Cpu` owns today except the ISA's own registers,
/// decode tables, and halt states.
pub struct MachineState {
    pub(crate) bus: Bus,
    pub(crate) interrupts: InterruptController,
    pub(crate) evaluator: WatchEvaluator,
    pub(crate) breakpoints: HashSet<u16>,
    pub(crate) clock_speed: ClockSpeed,
    pub(crate) cycles: u64,
    pub(crate) vector_resolver: Box<dyn VectorResolver>,
    pub(crate) log_sender: LogSender,
}

impl MachineState {
    /// The breakpoint/watch check lifted out of `Cpu::step`. The CPU passes its own
    /// `WatchContext` (registers + bus), so register/flag/endianness semantics stay per-ISA.
    pub fn check_halt<Op>(
        &mut self,
        pc: u16,
        skip_pc: Option<u16>,
        ctx: &dyn WatchContext,
    ) -> Option<StepResult<Op>>;
}

/// The 6502's WAI/STP map onto these; the 6809's CWAI/SYNC will too.
pub enum HaltState { Running, Waiting, Stopped }

pub trait Processor: Send + 'static {
    /// Snapshot type for `CpuLiveSnapshot` and the debugger.
    type Registers: Copy + Send + Sync + 'static;
    /// The decoded instruction carried by `StepResult::Executed`.
    type Op: Copy + Send + 'static;
    /// Opaque stack-depth marker used by `step_return`.
    type StackMark: Copy + Send;

    fn machine(&self) -> &MachineState;
    fn machine_mut(&mut self) -> &mut MachineState;

    fn step(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult<Self::Op>;
    fn reset(&mut self) -> Result<(), ExecError>;
    fn registers(&self) -> &Self::Registers;
    fn pc(&self) -> u16;
    fn halt_state(&self) -> HaltState;
    fn flush_trace(&mut self);

    /// Step-over support: `Some(len)` if the instruction at `pc` is a subroutine call
    /// (6502: JSR = 3; 6809 later: JSR/BSR/LBSR with their lengths).
    fn subroutine_call_len(&self, pc: u16) -> Option<u16>;
    /// Step-out support (replaces the `S`-specific logic in `step_return`).
    fn stack_mark(&self) -> Self::StackMark;
    fn has_returned_from(&self, mark: Self::StackMark) -> bool;
    /// The memory window the live snapshot captures as "the stack"
    /// (6502: (0x0100, 256); a 6809 will center it on S).
    fn stack_window(&self) -> (u16, u16);

    /// Watch-expression name maps for `WatchCompiler::new`.
    fn map_register_name(name: &str) -> Option<Operand>;
    fn map_flag_name(name: &str) -> Option<Operand>;

    // Default methods forwarding to `machine()`: bus(), bus_mut(), interrupts(),
    // interrupts_mut(), evaluator(), evaluator_mut(), add/remove/clear_breakpoint(),
    // breakpoints(), cycles(), clock_speed(), is_waiting(), is_stopped().
}
```

Notes:

- **Default methods keep call sites unchanged.** Because the forwarding methods have default
  implementations, `cpu.bus()`, `cpu.breakpoints()`, and similar calls compile as they do now.
  Callers may need `use emma65::emulator::Processor` in scope.
- **`map_*_name` are associated functions, not methods.** `WatchCompiler::new` takes function
  values before any CPU exists (the debugger compiles watchpoints while no CPU is running). The
  free functions `map_register_name`/`map_flag_name` stay as 6502 re-exports so the debugger's
  `watchpoints.rs` doesn't change in step 1.
- **The trace callback is not on the trait.** It stays an inherent method on the 6502 `Cpu`
  (see "Where the 6502 is baked in" above).
- **`InterruptController` stays IRQ/NMI/RESET in step 1.** Adding FIRQ/HALT lines is step 2.
  Nothing here should make that harder: `InterruptCommand` and `apply_interrupt_command` already
  act only on `MachineState::interrupts`.

## Implementation: four PRs

Each PR must leave `cargo test --workspace`, the CI clippy command (`-D warnings`), and
`cargo fmt --all -- --check` green. Each PR also needs a rerun of the clock-speed benchmark,
compared against the baseline above and summarized in the PR description.

### PR 1: Extract `MachineState` from `Cpu` (internal only)

- Add `src/emulator/machine.rs` with `MachineState` and move the shared fields out of `Cpu`. The
  6502 `Cpu` embeds `machine: MachineState`.
- Update the 6502's internal helpers (`bus_read`, `bus_write`, `bus_reset`, `finish_cycle`,
  `service_interrupt`, `irq_recognized`, and so on) to go through `self.machine`. These edits
  are mechanical. Keep `bus_read`/`bus_write` as the only route to the bus, so trace recording
  stays in one place.
- Lift the breakpoint/watch block out of `Cpu::step` (`cpu/mod.rs:337–370`) into
  `MachineState::check_halt`, passing `&CpuWatchContext`.
- `CpuBuilder` builds a `MachineState` and then the `Cpu` around it. Its public API doesn't
  change.
- **The public API doesn't change.** `Cpu`'s existing inherent methods keep their signatures.
- Benchmark. This PR moves the most hot-path code, so if anything regresses it shows up here.
  Check that `bus_read`/`finish_cycle` still inline. Field access through `self.machine.bus` is
  free, but a helper that went from inlined to out-of-line is not.

### PR 2: Add `Processor` and make `exec` generic

- Add the `Processor` trait (location: `src/emulator/processor.rs`) and implement it for the
  6502 `Cpu`. The 6502 hooks:
  - `subroutine_call_len`: opcode `0x20` gives `Some(3)`.
  - `stack_mark`: returns `regs.s`.
  - `has_returned_from`: is `(s.wrapping_sub(mark) as i8) > 0`, copied exactly from
    `step_return`.
  - `stack_window`: returns `(0x0100, 256)`.
- `StepResult` becomes `StepResult<Op>`. The 6502 module exports
  `pub type StepResult = exec::StepResult<DecodedOp>` under the existing path, so callers and
  tests that name `StepResult` without parameters keep compiling.
- Replace the fake clean-stop `NOP` with an explicit `StepResult::StopRequested` variant. Audit
  every exhaustive `match` on `StepResult`:
  - `debugger/src-tauri/src/disassembly.rs` `flags_from_result` (line 403) only uses
    `matches!`, so the new variant already gets the same all-false flags as `Executed(_)`.
    Check the other `run_cpu` paths and any exhaustive matches the compiler reports.
  - No current caller looks inside the op of a clean-stop result. `tests/system_integration.rs:83`
    matches `Executed(op)` only on an ordinary step, which can't produce this variant.
- Make these generic over `P: Processor`: `run`, `run_from`, `run_loop`, `step_into`,
  `step_over_breakpoint`, `step_over_subroutine`, `step_return`, `RunHandle<P>`
  (`take_cpu() -> P`), `build_live_snapshot`, and `drain_interrupt_commands`. `RunStopper` and
  `InterruptCommand` don't depend on the CPU and stay non-generic.
- `CpuLiveSnapshot<R>`:
  - `registers: R`.
  - Rename `stack_page` to `stack: Vec<u8>` and add `stack_base: u16`, both filled from
    `P::stack_window()`.
  - Update `debugger/src-tauri/src/stack.rs:47` to use these fields. The frontend contract
    doesn't change, because for the 6502 the window is still page 1.
- Delete `JSR_OPCODE`/`JSR_BYTE_LEN` from `exec`.
- Benchmark. The run loop is now a generic function compiled once for the 6502. Throughput
  should be identical to PR 1.

### PR 3: Draw the core/6502 module boundary inside the crate

Move code so that one prefix holds everything 6502-specific, and nothing outside that prefix
names a 6502 type:

- Move `src/emulator/cpu/` (decoder, ALU, status, variant, vectors, trace) into a new
  `src/mos6502/` module tree. So that `bus` and `config` no longer need them, move the vector
  constants with it.
- Move `src/disassembler/` and `src/assembler/` under `src/mos6502/`.
- Move `config/templates/` there as well, since its ROMs are 6502 programs. Leave the template
  registry mechanism (`materialize`, the `TEMPLATES` type) in core, and have the 6502 provide
  the entries.
- Split `Config::build`:
  - A core half (`build_machine` or similar) runs the device modules and returns
    `MachineState`, `ErrorReceiver`, `DeviceIdAllocator`, and the vector resolver.
  - A 6502 half wraps that output in a `Cpu`.

  `Config::build` keeps its current signature and returns a 6502 session, so the CLI and
  debugger don't change. `CpuVariantSpec` moves to the 6502 module; config deserialization
  keeps accepting `cpu-variant = "65C02"` exactly as before.
- `EmulatorSession<P>` becomes generic. A type alias under the existing name keeps the
  debugger's `lib.rs:170` and the CLI unchanged.
- Keep every path under `emma65::emulator::*` working with re-exports, so the debugger, the
  peripheral crates, and the integration tests need only mechanical import updates, or none.
- Add a test-only toy `Processor` in core, for example a "CPU" whose opcode `0x00` is NOP, `0x01`
  is CALL, `0x02` is RET, and `0xFF` is STP, with a 2-byte stack. Port a few `exec` tests to it:
  - throttled run
  - breakpoint hit
  - step-over of a CALL
  - step-return
  - WAI/STP parking

  This proves `exec` works without the 6502, and it is the first real check that the trait is
  sufficient. The existing 6502-based `exec` tests stay where they are as regression coverage.
- Update `CLAUDE.md`'s crate-structure section. It is already stale: it lists
  `src/emulator/disasm/`, but the code lives in `src/disassembler/`.

### PR 4: Extract the core crate

**Timing is deferred:** decide whether PR 4 lands in step 1 or waits for the 6809 crate once
PRs 1–3 have been reviewed and merged.

- Create a new workspace member, `emma6k-core`, and move into
  it: `bus`, `device`, `transport`, `config` (the framework and device modules), `exec`,
  `machine`, `processor`, `session`, `logging`, `watch`, and the shared `location`/`text`
  helpers.
- `emma65` becomes the 6502 crate plus the `emma65` and `emma65-tracer` binaries. It depends on
  the core crate and re-exports it, so `emma65::emulator::*` stays a working path for the
  debugger and peripheral crates.

  Moving the peripheral crates (`display`, `led-matrix`, `lcd-display`) to depend on the core
  crate directly is optional. It's a reasonable follow-up because they only use frame and
  geometry types.
- The compiler now enforces the boundary: the core crate can't name a 6502 type.
- Release and build touchpoints:
  - Add `build-info` as a build-dependency of the new crate's `build.rs`.
  - `emma6k-core` is versioned in lockstep with `emma65`: every `emma65` release bumps both to
    the same version, and `emma6k-core` is never released on its own. Teach the `release` skill
    this, so it updates both version files, writes one changelog entry, and uses one tag.
  - Packaging (`package.metadata.deb`/`generate-rpm`) needs no change, because the binary set
    is the same.
- Benchmark. Crossing a crate boundary can stop small functions from being inlined. If the
  numbers drop, mark the hot accessors on `Bus`/`MachineState` `#[inline]`, or turn on
  `lto = "thin"` in the release profile, and re-measure.

## Verification for each PR

1. `cargo test --workspace`. This includes `klaus_dormann_65c02`, which exercises the whole ISA
   and is the strongest guard on the 6502 move, plus `exec_integration`,
   `system_integration`, `config_integration`, `emulator_binary`, and `assembler_execution`.
2. Run CI's exact clippy command with `-D warnings`, and `cargo fmt --all -- --check`.
3. Run the clock-speed benchmark: both sweeps, at least two runs, compared against the baseline
   table above.
4. Manual smoke test of the CLI: `emma65` with no config (the TaliForth default), then
   `--profile` for one other template. Also record a trace and decode it with `emma65-tracer`.
5. Manual smoke test of the debugger: Run/Stop, Step, Step Over a JSR, and Step Out. Also check
   the stack panel while running, the watchpoints panel, the memory variables panel, the trace
   window, and a UI-driven IRQ/NMI/reset during a run (this covers the `StopRequested` and
   `Reset` paths).

## Not in step 1

- Any 6809 code, the `cpu = "..."` family selector, and the enum that picks a session at
  startup (step 2).
- FIRQ/HALT interrupt lines and `IoDevice::firq_active` or a generalized interrupt-line mask
  (step 2).
- Trace format v3 and generic trace and disassembly-from-trace (step 2).
- Register-description-driven debugger panels and per-ISA assembler and disassembler panels
  (step 3).
- Renaming the project to emma6k (see "Decisions" below). This was done separately, before step 1.

## Decisions

Decided 2026-11-26:

1. **The core crate is named `emma6k-core`.** It was first chosen as `emma6k-core`, then changed
   to follow the rename's rule that crates which aren't 6502-specific use the `emma6k-` prefix.
2. **The project is renamed to emma6k, before step 1 starts.** This is done on branch
   `chore/rename-emma6k`. Crates that aren't 6502-specific got the `emma6k-` prefix
   (`emma6k-debugger`, `emma6k-display`, `emma6k-led-matrix`, `emma6k-lcd-display`,
   `emma6k-build-info`). The 6502 crate and its binaries keep `emma65`/`emma65-tracer`, and the
   apt/rpm package is `emma6k`. See `.claude/memory/emma6k-rename.md` for status and the
   remaining steps.
3. **The clean stop is reported as an explicit `StepResult::StopRequested` variant** (PR 2), not
   as a synthesized idle instruction.
4. **`emma6k-core` is versioned in lockstep with `emma65`** (PR 4).
5. **PR 4's timing is decided after PRs 1–3 are reviewed and merged.**
