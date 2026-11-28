# Multi-CPU support, step 3: the 6809 in the debugger

## Context

Step 1 (`plan/multi-cpu-step1-core-refactor-plan.md`) put a CPU-neutral core behind the
`Processor` trait. Step 2 (`plan/multi-cpu-step2-mc6809-plan.md`) added the `emma09` crate
and the 6809 for the command line. The debugger (`emma6k-debugger`) is still 6502-only: a
profile whose `cpu-variant` is `6809` fails to load, with an error saying the debugger runs
only the 6502 (`src/mos6502/config.rs`).

Step 3 makes the debugger run either CPU, chosen by the profile. When it's done:

- A profile with `cpu-variant = "6809"` (or `cpu = "6809"`) loads in the debugger, and every
  panel works for it: registers, disassembly and stepping, breakpoints, watchpoints, the
  stack, memory, memory variables, symbols, trace, the CPU/bus status bar, and the assembler.
- The New Profile dialog offers every family's templates, so a CamelForth profile is one
  click away.
- The panels learn what a CPU looks like (registers, flags, interrupt lines, stack, byte
  order) from a description the backend sends, not from 6502 constants in the frontend.
- `emma09` has a 6809 assembler, and the Assembler panel uses it for a 6809 profile.
- Every 6502 profile behaves exactly as it does today.

## Decisions already made

From the step 1 planning (2026-11-26):

1. **Register descriptions come from the backend**, and the stack, disassembly, and
   assembler panels are driven by the processor, not hard-coded for the 6502.
2. **No `dyn Processor` on the hot path.** The run loop stays `exec::run_from::<P>`,
   compiled once per CPU.

From the user, 2026-11-28:

3. **The Assembler panel gets a real 6809 assembler**, written for `emma09` the way
   `emma65` has its own, as part of step 3.
4. **The 6809 Stack panel shows S only**: a window of memory around S, with full 16-bit
   addresses. U is visible in the Register panel and the Memory panel.
5. **Memory Variables follow the CPU's byte order**: little-endian on the 6502, big-endian
   on the 6809. There is no per-variable setting.

## Design

### Choosing the CPU when a profile loads

`load_session` (`debugger/src-tauri/src/lib.rs`) extracts `emma65::emulator::Config` straight
from the profile's figment (TOML < `EMMA6K_` env). Step 3 reads the CPU first, the same way
`emma6k` does: take `cpu-variant` (or `cpu`) from the merged figment, map it to a family, and
then extract that family's `Config` (`emma65::emulator::Config` or `emma09::Config`) from the
same figment and build its session. A profile that names no CPU is a 6502 profile, as today,
so every existing profile keeps working. (`emma6k` has no default CPU, but the debugger has
always defaulted to the 6502, and its existing profiles rely on that.)

The CLI's mapping from variant to family is `Family` in `cli/src/bin/emma6k/family.rs`. The
debugger needs the same mapping and the same merged template list. Rather than copy it, move
`family.rs` into a small library target of the `cli` package (crate `emma6k`), keep the
binaries using it from there, and have the debugger depend on it. See open decision 1.

Neither `Config` needs to change: both already have `build_with_context`, so the debugger
passes either one the same `InstantiationContext` it builds today. It builds a 6809 session
with `emma09::device_registry()`, which already leaves out `pic/finch`.

### The debugger's CPU: an enum, with generic helpers

Every backend module holds or borrows a concrete `emma65::emulator::Cpu` today: `CpuState`,
`LiveSnapshotRx` (a `CpuLiveSnapshot` of 6502 registers), `DisassemblerState`, the trace
writer and reader, and each module's tests. Step 3 introduces a `cpu` module in the
debugger with:

```rust
pub enum DebugCpu {
    Mos6502(emma65::emulator::Cpu),
    Mc6809(emma09::Cpu),
}
```

and `CpuState(Mutex<Option<DebugCpu>>)`. Commands that only need what `Processor` already
provides (bus, breakpoints, interrupts, reset, cycles, halt state) call a small
`with_processor!` macro that matches the enum and runs the same generic code on either arm.
Code that differs per CPU (register snapshots and edits, disassembly, trace rows,
watchpoint evaluation, assembling) is an `impl DebugCpu` method that matches and calls the
family's own type. Free-running is the one place performance matters: `run_cpu` matches
once and spawns `exec::run_from::<emma65::emulator::Cpu>` or `run_from::<emma09::Cpu>`, so
the loop stays monomorphic, as decision 2 requires. A `dyn`-trait design was considered and
rejected: it would push the run loop's spawning and the per-family `Registers`/`Op` types
behind erased interfaces for no gain, and two families don't need open-ended extension.

`LiveSnapshotRx` becomes an enum of the two `watch::Receiver<Option<CpuLiveSnapshot<R>>>`
types, and the debugger's disassembler state an enum of the two disassemblers.

### The CPU description

On every session load the backend publishes a `CpuDescription` (a new `get_cpu_description`
command, plus a field on the existing `session-loaded` event):

```text
family            "6502" | "6809"
variant           "65C02" | "WDC65C02" | "6809"
registers         [{ name, width_bits, role }]      role: data | index | stack | pc | flags
flags             [{ label, bit, name }]             for the flags register, high bit first
flags_register    "P" | "CC"
interrupt_lines   ["IRQ", "NMI"] | ["IRQ", "FIRQ", "NMI"]
stack             { pointer: "S", page_fixed: bool } (the 6502's lives in page 1)
byte_order        "little" | "big"
max_instruction_len  3 | 5
assembler         true                              (false only until Unit 6)
```

The frontend keeps it in a `CpuDescriptionContext` and every panel that today hard-codes
the 6502 reads it instead. The 6502's description reproduces today's layout exactly (A, X,
Y, S, PC, P; flags N V - B D I Z C), so its panels don't change. The 6809's is A, B, D (a
derived view of A:B), DP, X, Y, U, S, PC, CC, with flags E F H I N Z V C.

### Register snapshots and edits

`RegisterSnapshot` (`registers.rs`) has fixed `a`/`x`/`y`/`s`/`pc`/`p` fields, built in five
places (`registers.rs`, `disassembly.rs` twice, `cpu_bus.rs`, the run-completion handler).
It becomes:

```text
values          { name → u32 }      every register in the description, D included
changed_flags   u32                 flag bits that changed on the last step
cpu_stopped, cpu_waiting, breakpoint_hit   (unchanged)
```

built by one `DebugCpu::register_snapshot` helper. `set_register` takes the register's name
instead of the `RegisterField` enum; editing D writes A and B. `ChangedFlagsState` compares
the flags register before and after a step, whichever CPU it is. The frontend
`RegisterPanel` renders rows from the description: widths come from `width_bits` (8 or 16),
and the flags row from `flags`. Its existing radix, edit and ASCII behavior carries over per
row.

### Stack panel

`get_stack` reads page 1 and returns `s: u8` plus 256 bytes. Step 3 returns
`{ pointer: u16, base: u16, bytes }`, where the window is `Processor::stack_window()` (the
6502's is page 1, as today; the 6809's is S minus `STACK_WINDOW_BELOW` to S plus
`STACK_WINDOW_ABOVE`, already used for live snapshots). The halted path reads the same window
with `peek_range`, so running and halted views match. `StackPanel.tsx` stops adding `0x0100`
to page offsets and shows the real addresses; for a page-fixed stack it keeps today's
wrap-around word pairs. Word pairs show in the CPU's byte order.

### Memory variables and the memory panel

`memory_variables.rs` reads and writes words and dwords little-endian
(`read_value`, and `toLittleEndianBytes` in `MemoryVariablesPanel.tsx`). Both take the byte
order from the description (decision 5). Existing 6502 profiles see no change. The Memory
panel shows bytes only, so it needs nothing.

### Disassembly, stepping, and breakpoints

`get_disassembly` calls the family's `Disassembler::disassemble_range`; both return the same
line shape (`addr`, `raw_bytes`, `labels`, `mnemonic`, `operand_text`, `comment_text`,
`is_valid`), so `DisassembledRow` doesn't change. The 6809's instructions are up to five
bytes long, so `DisassemblyPanel.tsx`'s bytes column sizes itself from
`max_instruction_len`, and its scroll-back heuristic (stepping back to find an instruction
boundary) must allow for five-byte instructions. Step into, step over (via
`Processor::subroutine_call_len`, which covers BSR, LBSR and JSR), step out, run, stop and
breakpoints are generic already and only need the enum dispatch.

### Watchpoints

`watchpoints.rs` compiles with the 6502's `map_register_name`/`map_flag_name` and evaluates
on demand with the 6502's inherent `Cpu::evaluate_watchpoints`. Step 3 compiles with
`P::map_register_name`/`P::map_flag_name` for the session's CPU, and adds
`emma09::Cpu::evaluate_watchpoints`, built on the 6809's existing big-endian watch context,
so `W[addr]` reads a 6809 word the way the 6809 does. A watchpoint file written for one CPU
and loaded with the other reports its unknown-register errors through the existing
compile-error path; nothing new is needed.

### The CPU/bus status bar and FIRQ

The status bar has NMI and IRQ controls. For a CPU whose description lists FIRQ, it gets a
third control, an assert/release toggle like IRQ's. The backend reserves a second debugger
interrupt source, `DEBUGGER_FIRQ = MAX_IRQ_SOURCES - 2`, allocated like `DEBUGGER_IRQ`, and
adds its bit to the interrupt controller's FIRQ mask
(`set_firq_mask(firq_mask() | bit)`) after the session is built. `CpuBusSnapshot` gains
`firq_active` (`InterruptController::firq_active()`). Reset clears it the way it clears the
UI's IRQ. While the CPU runs, the IRQ control sends a one-shot pulse through
`RunStopper::assert_irq_pulse`; the FIRQ control does the same with its own source, which the
FIRQ mask routes to FIRQ. `cpu_stopped`/`cpu_waiting` already cover SYNC and CWAI, which
report as `Waiting`.

### Trace

`trace.rs` writes with the 6502's `BinaryTraceWriter` and reconstructs rows with the 6502's
`TraceRowAssembler`. Both are generic over a CPU's register record since step 2 Unit 6; the
debugger picks the family's types. `TraceRowDto`'s fixed `a`/`x`/`y`/`s`/`p` fields become
`registers: { name → u32 }`, filled from the record, and `TracePanel.tsx` builds its register
columns from the description. A serviced 6809 interrupt (a row with no instruction line) is
shown as an interrupt row named from its vector, as `emma6k-tracer` does.

### Profiles and templates

`profile::list_templates` and `create_profile_dir_from_template` use the 6502's
`templates::TEMPLATES`. They switch to the merged list from `Family` (see open decision 1), and the New Profile dialog shows each template's CPU next to its name. The
default profile stays TaliForth.

### The 6809 assembler

`emma65`'s assembler (`src/mos6502/assembler/`, about 4,500 lines; see
`plan/assembler-plan.md`) splits into a CPU-neutral front end (scanner, tokens, expression
tree and evaluator, the statement parser's shared parts, errors, about 1,700 lines) and the
6502-specific instruction table, operand forms and driver. Step 3 moves the CPU-neutral part
into `emma6k-core` as `emma6k_core::asm`, as `location.rs` and `text.rs` were shared before,
with no change to what the 6502 assembler accepts. `emma09::assembler` is then built on it:

- **Syntax:** Motorola mnemonics and operands, the syntax `emma09`'s disassembler emits, so
  anything the Disassembly panel shows can be pasted back into the Assembler panel.
- **Directives:** the 6502 assembler's `.org`, `.byte`, `.word`, `.res`, plus the Motorola
  forms `ORG`, `FCB`, `FDB`, `FCC`, `RMB`, `EQU` and `SETDP`, so as9- and lwasm-style
  snippets work. See open decision 2.
- **Addressing:** inherent, immediate (8- and 16-bit by instruction), direct, extended,
  every indexed form (constant offsets with the shortest encoding, accumulator offsets,
  auto-increment/decrement, PC-relative, and each of these indirect), and relative (short
  and long branches). Direct versus extended is chosen from `SETDP`'s assumed direct page,
  with `<` and `>` forcing either, as in as9. Constant-offset size and short-versus-long
  branches are shrunk by the driver's existing forward-reference passes.
- **Register lists and pairs:** PSHS/PULS/PSHU/PULU register lists, and TFR/EXG register
  pairs with the 6809's size rules.
- **Verification:** the disassembler's golden every-opcode table (`every_form.txt`) round-
  trips: assembling each line reproduces its bytes. `lwasm_check.py` already cross-checks
  that table against lwasm.

The Assembler panel's backend (`assembler.rs`) calls the family's `assemble`, and its output
(`AssembledProgram`: segments and symbols) has the same shape for both. The panel itself
only needs its placeholder text and help to follow the CPU.

### What the wiki needs

`doc/src/the-debugger.md` describes the panels with 6502 register names, page 1, and the
two interrupt controls. Each unit updates the parts it changes, and Unit 6 adds a "Using a
6809 profile" section. Per `.claude/memory/wiki-user-facing-writing.md`, no Rust internals.

## Implementation: seven units

Each unit is a branch and a PR, reviewed and merged before the next starts. Units 1 and 2
keep the debugger 6502-only but move every panel onto the description, so they can be
reviewed and tested against today's behavior. Unit 3 adds the 6809. Units 4 to 6 build the
assembler. Unit 7 covers the cross-CPU polish.

### Unit 1: the CPU description, registers, and trace rows (6502 only)

- Backend: `CpuDescription` (6502 only), `get_cpu_description`, the `session-loaded`
  payload; `RegisterSnapshot` as `values`/`changed_flags`; one snapshot helper replacing the
  five constructions; `set_register` by name; `TraceRowDto.registers`.
- Frontend: `CpuDescriptionContext`; `RegisterPanel` and `TracePanel` render from it; the
  status bar's interrupt controls come from `interrupt_lines` (IRQ and NMI for the 6502).
- Tests: the existing register, trace, and status-bar tests, rewritten for the new shapes.
- UAT: every 6502 panel looks and behaves as before, including register edits in each
  radix, the flags row, and the trace columns.

### Unit 2: stack window and byte order (6502 only)

- Backend: `get_stack` returns `{ pointer, base, bytes }` from `stack_window()`;
  `memory_variables.rs` takes its byte order from the description.
- Frontend: `StackPanel` shows real addresses and handles a stack that isn't page-fixed;
  `MemoryVariablesPanel` writes in the description's byte order.
- UAT: the 6502 Stack panel and Memory Variables panel behave as before.

### Unit 3: the 6809 session

- Move `Family` into the `cli` package's library target (open decision 1); the debugger
  depends on `emma09` and it.
- Backend: `DebugCpu` and `with_processor!`; `load_session` picks the family; every module
  that names `Cpu` switches to `DebugCpu` (assembler, breakpoints, cpu_bus, disassembly,
  memory, memory_variables, registers, stack, symbols, trace, watchpoints, lib); the 6809's
  `CpuDescription`; `emma09::Cpu::evaluate_watchpoints`; `DEBUGGER_FIRQ` and `firq_active`;
  6809 trace recording and rows; the merged template list. The Assembler panel reports
  "no 6809 assembler yet" for a 6809 profile until Unit 6 (`assembler: false`).
- Frontend: FIRQ control; `DisassemblyPanel` sized by `max_instruction_len`; the New Profile
  dialog shows each template's CPU; the Assembler panel disabled when `assembler` is false.
- `src/mos6502/config.rs` keeps rejecting `6809` (the 6502's `Config` still isn't the 6809's),
  but its message no longer mentions the debugger.
- Tests: the backend modules' 6502 tests keep passing through `DebugCpu`; new 6809 tests
  for register snapshots and edits (including D), the stack window, watchpoints with
  6809 register and flag names, FIRQ assert/release, and trace rows with an interrupt row.
- UAT: create a CamelForth profile from New Profile; it boots in the terminal; step, step
  over a JSR, step out, set a breakpoint on a word's code, edit U and D, watch `B[...]`
  and a CC flag, assert FIRQ (no effect on CamelForth, but the indicator shows it), record
  and browse a trace; then switch back to a TaliForth profile and repeat a quick check.

### Unit 4: the shared assembler front end in core

- Move the CPU-neutral assembler modules into `emma6k_core::asm`, with `emma65`'s assembler
  built on them. No change to what it accepts or produces; its existing tests are the check.
- The split between shared and 6502-specific code is settled by reading the modules, not
  by line counts: anything that names a 6502 mnemonic, operand form, or `CpuVariant` stays
  in `emma65`.

### Unit 5: the 6809 assembler

- `emma09::assembler`: instruction table from `emma09`'s opcode tables, operand forms,
  directives (open decision 2), `SETDP` and `<`/`>`, register lists and pairs, the driver's
  size shrinking for indexed offsets and branches.
- Tests: unit tests per addressing form and directive; the `every_form.txt` round trip;
  diagnostics for out-of-range branches, bad register pairs, and unknown mnemonics.

### Unit 6: the Assembler panel for the 6809

- `assembler.rs` dispatches to the family's assembler; `assembler: true` in the 6809's
  description; panel placeholder and help follow the CPU.
- Wiki: the debugger page's assembler section, and a "Using a 6809 profile" section.
- UAT: assemble a small 6809 program into RAM in a CamelForth profile, see its symbols in
  the Symbols panel, step through it.

### Unit 7: cross-CPU checks

- Switching profiles between CPUs: every panel picks up the new description without a
  restart (Open Recent, New Profile, Open Profile), and per-profile files written for the
  other CPU (watchpoints, memory variables) load with errors where they don't fit rather
  than failing the profile.
- Frontend tests for the description-driven panels with the 6809 description (registers,
  stack, status bar, trace columns), alongside the 6502's.

### Compile dependencies between units

Unit 1 changes `RegisterSnapshot` and `TraceRowDto` and every builder of them in one PR, so
it compiles alone. Unit 2 touches only the stack and memory-variables paths. Unit 3 is the
large one: introducing `DebugCpu` changes every backend module at once, because `CpuState`
is shared state; it can't be split without a temporary single-variant enum, which isn't worth
a review cycle (see open decision 3). Unit 4 moves code within the workspace with `emma65`
as its only user, and Unit 5 adds `emma09`'s use; Unit 6 depends on both and on Unit 3.
Unit 7 depends on Unit 3 and can go before or after Units 4 to 6.

## Not in step 3

- The 6309, which would be a new variant inside `emma09`.
- A second, U-stack window in the Stack panel (decision 4).
- Per-variable byte order (decision 5).
- 6809 syntax highlighting in the Assembler panel (there's no 6502 highlighter either).
- Performance work on the 6809 (the out-of-line `Bus::read`/`find_region_mut` lead in
  `.claude/memory/multi-cpu-effort.md`).

## Open decisions

1. **Where `Family` lives.** Recommended: a library target in the `cli` package (crate
   `emma6k`), used by its binaries and the debugger. The alternative is a copy in the
   debugger, which risks the two lists of variants drifting apart. It can't go in
   `emma6k-core`, which never names a CPU.
2. **6809 assembler directives.** Recommended: accept both the 6502 assembler's dotted
   directives and the Motorola forms (`ORG`, `FCB`, `FDB`, `FCC`, `RMB`, `EQU`, `SETDP`).
   The alternative is Motorola forms only, which matches as9 and lwasm but not the 6502
   panel's help.
3. **Splitting Unit 3.** Recommended: keep it one PR. The alternative is a 3a that introduces
   `DebugCpu` with only the 6502 arm (a pure refactor) and a 3b that adds the 6809 arm and
   everything that uses it; smaller reviews, one more cycle.
4. **A profile with no CPU.** Recommended: the debugger keeps treating it as a 6502 profile,
   as today. The alternative, matching `emma6k`'s "no default CPU", would break every existing
   debugger profile that doesn't set `cpu-variant`.

## Host notes

The debugger is built and tested on the Linux workstation. CI's debugger job runs for changes
under `debugger/`, so Units 1 to 3, 6 and 7 get it; Units 4 and 5 touch only `src/`, `core/`
and `emma09/`, which CI's debugger job skips on purpose
(`.claude/memory/ci-path-filters-deliberate.md`), so those PRs build and test the debugger
locally and say so.
