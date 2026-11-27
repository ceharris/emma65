# Multi-CPU support, step 2: the 6809 for the CLI

## Context

Step 1 (`plan/multi-cpu-step1-core-refactor-plan.md`, PRs #664–666 and #668) put a CPU-neutral
core behind the `Processor` trait and moved it into the `emma6k-core` crate. The 6502 is still
the only CPU.

Step 2 adds the Motorola 6809 for the command line. When it's done:

- A new `emma09` crate holds the 6809: CPU, `Processor` impl, configuration, starter template,
  disassembler, and trace record.
- A new CPU-neutral `emma6k` binary picks the CPU from the configuration (`cpu-variant =
  "6809"`, or its synonym `cpu`) or the command line (`--cpu 6809`), and builds that CPU's
  session once at startup.
- `emma65` and `emma09` are thin wrapper binaries that run `emma6k` with the CPU fixed.
- The 6809 has a CamelForth starter template, built from the GPL-3 CamelForth in
  [nealcrook/multicomp6809](https://github.com/nealcrook/multicomp6809), with its console I/O
  rewritten for emma's `console` device.
- Trace format v3 records either CPU's registers, and the tracer decodes 6809 traces.

The debugger stays 6502-only until step 3. In step 2 its only change is a clear error when a
profile asks for a CPU other than the 6502.

Step 2 is finished when:

- `emma09` with no configuration boots CamelForth on the console, and `emma6k --cpu 6809
  --profile camelforth` does the same.
- `emma65`'s behavior is unchanged, including its zero-config TaliForth default, `--profile`,
  `--trace-file`, and every existing test.
- A 6809 trace recorded with `--trace-file` decodes with the tracer.
- 6502 throughput matches the post-step-1 baseline in `.claude/memory/clock-bench-baseline.md`
  within run-to-run noise, and the 6809's throughput is measured and recorded.

## Decisions already made

These were decided with the user on 2026-11-27:

1. **The command line.** A CPU-neutral `emma6k` binary chooses the CPU from configuration or
   the command line. `emma65` and `emma09` are thin wrappers that run `emma6k` with the right
   CPU.
2. **One CPU key.** `cpu` is a synonym for `cpu-variant`. Its values name a variant (`65c02`,
   `wdc65c02`, `6809`, `6309`, …), and the variant implies the family. `emma6k` has no default;
   the wrappers take care of it.
3. **The 6809 crate is `emma09`.** The 6309, a 6809 superset, would later be a `cpu-variant`
   inside it. A 6800/6801/6803 family would be a separate crate, because its instruction set
   isn't compatible with the 6809's.
4. **Trace is in step 2, as the last unit.** The earlier units don't depend on it. Until it
   lands, `--trace-file` with a 6809 is a startup error.
5. **The tracer is `emma6k-tracer`**, a CPU-neutral replacement for `emma65-tracer`.
6. **`emma6k` and `emma09` are versioned in lockstep with `emma65`**, like `emma6k-core`.
7. **Environment variables switch to `EMMA6K_`**, with no fallback to `EMMA65_`.
8. **The starter template is CamelForth** from `nealcrook/multicomp6809`, rebuilt with GForth
   following the procedure in its wiki page
   [CamelForthRebuildANS](https://github.com/nealcrook/multicomp6809/wiki/CamelForthRebuildANS).
   Its basic I/O is modified for emma's console device. Shipping it under GPL-3 is fine, as
   long as the template includes the license details.
9. **ISA test suites to consider:** the MC6809 CPU Test Suite by W. Schwotzer, and Sean
   Riddle's Williams Emulation Test Suite. After looking at both, Unit 2 uses Schwotzer's and
   explains why Riddle's doesn't fit.

## Design

### Interrupt lines: FIRQ by device routing, not a new device method

The 6809 has IRQ, FIRQ, and NMI inputs. Which line a peripheral drives is board wiring, not a
property of the chip: an MC6850's IRQ output is wired to either IRQ or FIRQ on a 6809 board.
So, rather than adding `IoDevice::firq_active`:

- Every device spec accepts a common `interrupt = "irq" | "firq"` attribute. It defaults to
  `irq`, and the configuration framework handles it in `build_machine`, so individual device
  modules don't parse it. (`deny_unknown_fields` on each `*Attributes` struct stays: the
  framework removes the key before the module sees the attributes.)
- `InterruptController` gains a `firq_mask: u64` of device bits fixed at build time. A device's
  `irq_active()` still sets its bit in `irq_sources`, exactly as today. Then:
  - `irq_active()` is `irq_sources & !firq_mask != 0`.
  - `firq_active()` is `irq_sources & firq_mask != 0`.

  `poll_devices` doesn't change, so the per-device poll costs nothing extra. The 6502 path gains
  one AND with a mask that is always zero, and Unit 1 benchmarks it.
- The CPU tells `build_machine` which lines it has (`MachineSpec` gains the list). A 6502
  configuration that routes a device to `firq` fails at startup with a clear error.
- `VectorResolver::irq_mask` keeps gating IRQ only. FIRQ isn't masked by a resolver.
- A UI-driven FIRQ, an `InterruptCommand` variant, waits for step 3, because the debugger is the
  only thing that issues those commands.

**HALT** (the 6809's DMA bus-request pin) is out of scope. No emulated device masters the bus,
and adding the pin would put a check on the hot path with nothing to drive it.

### The `emma09` crate

The crate lives at `emma09/` as a workspace member and mirrors `src/mos6502/`'s layout:

- `cpu/`:
  - `Registers`: A, B (D as a view), X, Y, U, S, PC, DP, CC.
  - Decode of pages 0, 0x10 and 0x11.
  - The indexed-addressing postbyte, including indirect modes.
  - ALU and condition codes.
  - Interrupts: IRQ, FIRQ, NMI, SWI/SWI2/SWI3, RESET.
  - CWAI and SYNC.
  - `Processor` impl.
  - `CpuBuilder`.
  - An invalid-opcode policy like the 6502's.
- `config.rs`:
  - `Config` (`cpu-variant = "6809"`, `clock-speed-hz`, `devices`, `unmapped-policy`), built
    through `build_machine` the way `mos6502::config` does it.
  - The `EmulatorSession` alias.
- `templates/`: the CamelForth template (Unit 5).
- `disassembler/` and the 6809 trace record (Unit 6).

The CPU uses only core's public API, `MachineState`'s accessors, and its per-step helpers
(`check_halt`, `finish_cycle`, `tick_idle`, `irq_recognized`), as `core/src/exec/toy.rs` does.
Like the 6502, it adds cycles per instruction through `finish_cycle`, not per bus cycle.

How the 6809 implements the `Processor` hooks:

| Hook | 6809 |
|---|---|
| `subroutine_call_len` | `BSR` (0x8D) is 2 bytes. `LBSR` (0x17) is 3. `JSR` is 2 when direct (0x9D), 3 when extended (0xBD), and 2 plus the indexed postbyte's extra bytes when indexed (0xAD), all peeked at `pc`. |
| `stack_mark` / `has_returned_from` | The mark is `S`, a `u16`. The CPU has returned once `S` is above the mark, since the stack grows down. Use the same wrap-tolerant signed comparison the 6502 uses, at 16 bits. |
| `stack_window` | A window around `S` (for example `S - 32 .. S + 96`) that is clamped to the address space. Only the debugger consumes it, in step 3. |
| `halt_state` | CWAI and SYNC give `Waiting`. The 6809 has no STP, so it never reports `Stopped`. |
| `map_register_name` / `map_flag_name` | Registers A B D X Y U S PC DP CC. Flags E F H I N Z V C. |

The 6809's `WatchContext` reads `W[]` and `D[]` **big-endian**. `WatchContext` is per-CPU, so
the watch pipeline doesn't change.

Details that the tests must pin down:

- NMI is disarmed after reset until the first load of S.
- Entire-state (E=1) and fast (E=0) interrupt stacking.
- CWAI pushes the state before it waits.
- SYNC resumes without servicing the interrupt if it's masked.
- The reset vector is at `FFFE`, and the other vectors are at `FFF2–FFFC`.
- The documented cycle counts, including the extra cycles of indexed modes.

**Built-in devices.** Audit each device module for 6502 assumptions. One is already known:
`pic_finch` resolves only the 6502's IRQ vector (`FFFE`). The 6809's IRQ vector is `FFF8`, so
the 6809 session uses a device registry without the 6502-specific modules, and asking for one is
an "unknown device type" error.

### The `emma6k` binary and its wrappers

Add a new workspace member at `cli/`, a package named `emma6k`, with three binaries:

- **`emma6k`** is the real CLI. `src/bin/emulator/{main,config,tty}.rs` moves here.
  - **One key chooses the CPU:** `cpu-variant`, with `cpu` as a synonym, and `--cpu-variant`
    or `--cpu` on the command line. Its values name a variant, and the variant implies the
    family. Values are case-insensitive:

    | Value | Family | Notes |
    |---|---|---|
    | `65c02` (also `c02`) | 6502 | |
    | `wdc65c02` (also `wdc02`) | 6502 | |
    | `6809` | 6809 | |
    | `6309` | 6809 | Reserved: the parser knows it but reports "not supported yet" until the 6309 variant exists |

    Setting both `cpu` and `cpu-variant` in the same source is an error. Across sources, the
    usual precedence applies (TOML < env < command line), whichever spelling each source uses.
  - **`emma6k` has no default CPU.** If no source names a variant, it exits with an error
    saying to set `cpu-variant` or `--cpu`, or to use the `emma65`/`emma09` wrapper.
  - The family's crate parses the variant (`emma65`'s `CpuVariantSpec`, `emma09`'s
    `CpuVariantSpec`). `emma6k` only maps each value to its family.
  - `main` resolves the family, then calls a `run_session<P: Processor>` function that holds
    today's `main` body (console stdio, log file, trace file, raw mode, `run_from`, the event
    loop). It is compiled once per CPU, so the hot path is never dynamic.
  - **Environment variables use the `EMMA6K_` prefix** (for example `EMMA6K_CPU_VARIANT`), with
    no fallback to `EMMA65_`. The debugger switches to `EMMA6K_` in the same unit, so the two
    agree.
- **`emma65` and `emma09`** are wrappers.
  - Each finds `emma6k`: first next to `std::env::current_exe()`, then on `PATH`.
  - Each `exec`s it with its arguments, using `CommandExt::exec`, so signals, the terminal and
    the exit status are `emma6k`'s own.
  - A wrapper fixes the **family** but not the variant, because a configuration may name either
    6502 variant. It passes a hidden argument (for example `--family 6502`) that does two
    things:
    - It supplies the family's default variant when nothing else names one: `65c02` for
      `emma65`, today's default, and `6809` for `emma09`.
    - It makes a variant from another family an error. So `emma65 --config six809.toml` fails
      clearly rather than silently running a 6502.
- **Templates and profiles.** `emma6k` merges each CPU crate's `TEMPLATES` into one list. Every
  template's TOML sets `cpu-variant` (TaliForth's already says `WDC65C02`).
  - `--profile camelforth` selects the 6809 by itself. Combining it with a 6502 variant, or
    running it through `emma65`, is an error.
  - With no devices configured, each family uses its own default template: `taliforth` for the
    6502, and `camelforth` for the 6809 once Unit 5 adds it. Until then, `emma09` with no
    configuration exits with a message saying to pass `--config` or `--profile`. Bare `emma6k`
    with no configuration at all hits the "no CPU" error first.

The 6502's `Config`, which the debugger uses, accepts `cpu` as a serde and clap alias for
`cpu-variant`. Its parser recognizes the 6809 values and rejects them with "the debugger
supports only the 6502 (65c02, wdc65c02)", rather than today's generic "Invalid CPU variant". The
library's `Config::build` keeps its `65c02` default for the debugger's existing profiles. The
"no default" rule belongs to `emma6k`.

Moving the CLI also moves its touchpoints:

- `tests/emulator_binary.rs` moves to `cli/tests/`, because `CARGO_BIN_EXE_*` only names a test's
  own package's binaries. It gains wrapper tests.
- The `[package.metadata.deb]` and `generate-rpm` asset lists gain `emma6k` and `emma09`.
- CI's core-emulator job path filters gain `cli/**` and `emma09/**`. The debugger and SDL jobs
  stay filtered to their own directories, on purpose (see
  `.claude/memory/ci-path-filters-deliberate.md`).
- The release skill learns the new packages. `emma6k` and `emma09` are versioned in lockstep
  with `emma65`, like `emma6k-core`.
- `EMMA65_` becomes `EMMA6K_` in the CLI, the debugger (`debugger/src-tauri/src/lib.rs`), the
  binary tests, the wiki's "Running the emulator" page, `CLAUDE.md`, and the `build-info`
  doc comment that explains why its own variables are unprefixed.
- The wiki's CLI and configuration pages are updated.

### CamelForth template

Per the rebuild page, CamelForth's build is `gforth gforth_shim.fs` over `chromium_ans.scr`. It
produces `6809M.HEX`, an Intel HEX file of 8 KB at `E000–FFFF` with the vectors compiled in.
GForth's 6809 metacompiler is self-contained, so no external assembler is needed. The multicomp
image also assumes:

- a VDU console at `FFD0/FFD1`
- an SD card at `FFD8/FFD9`
- an MMU and ROM-paging register at `FFDE`
- user area `0600` and block buffers at `D000`

The work:

- Vendor the needed sources under `emma09/src/templates/camelforth/forth/`:
  - `chromium_ans.scr` and `gforth_shim.fs`
  - the GPL-3 `copying` file and a README giving where they came from and what changed
- Rewrite `KEY`, `KEY?` and `EMIT` for emma's `console` device (data register at offset 0, latch
  register at offset 1). Remove or stub the MMU, ROM-paging and SD-card initialization and words.
- Add a `build.sh` that:
  - runs GForth
  - converts the Intel HEX file to `program.bin`
  - converts CamelForth's diagnostic symbol dump (`EVERYONE = 1`) to a VICE `program.lbl`

  Both outputs are checked in, as for the 6502 templates, so normal builds don't need GForth.
  The `.scr` block files are binary, so edit them with the repo's `forth_block2ascii`-style
  tools, or keep a text master and generate the blocks from it; decide which in the unit.
- `emulator-template.toml`:
  - `cpu-variant = "6809"`
  - RAM at `0000–DFFF`
  - the ROM at `E000–FFFF`
  - the `console` device shadowing the old VDU address range inside the ROM (most-specific-wins
    lets a small device region sit over the ROM)
  - optionally an MC6850, since it's native to the 6809
- An integration test boots the template, sends `2 3 + .` to the console, and expects `5 ok`.
  It also defines and runs a word. This doubles as the 6809's broadest end-to-end ISA test.

**License.** CamelForth is GPL-3.0-or-later, and emma6k is MIT. The user has approved shipping
it, embedded like the other templates, on condition that the template carries the license
details. The template directory holds:

- the full GPL-3 text (`copying`)
- the modified source
- a `LICENSE.md` or README section with:
  - CamelForth's copyright (Bradford J. Rodriguez, 1995, for both CamelForth for the 6809 and
    the Chromium 2 metacompiler)
  - the upstream URL and commit
  - a summary of emma's changes
  - a note that only this template is GPL-3, not the rest of emma6k

The template's description in `TEMPLATES` and its wiki entry name the license too.

### Trace format v3

The v2 record is 16 bytes: an 8-byte instruction ID, a tag, and 7 bytes of payload. A 6809
register snapshot is 14 bytes, so it doesn't fit.

- **v3 header.** It keeps the `E65T` magic and gains a CPU-family byte next to the variant byte.
  It also declares the file's record length. The 6502 keeps 16 bytes, and the 6809 uses 24: an
  8-byte instruction ID, a tag, and up to 15 bytes of payload.
- **Read and Write records** keep their layout. Only the register record is per family.
- **The 6502 writes v3** with family 0 and 16-byte records, so the record bytes are the same as
  v2's. The reader accepts both v2 and v3, so the debugger's trace window and old trace files
  keep working.
- **Where the code goes.** The CPU-neutral framing (header, fixed-length records, writer thread,
  `OverflowPolicy`, `ChannelTraceCallback`) moves to `emma6k-core`, generic over a per-CPU
  register-record codec. Each CPU crate supplies its codec. The 6502's `TraceKind` and
  `TraceDisassembler` keep their public paths.
- **The 6809 disassembler** goes in `emma09::disassembler`, with trace reconstruction
  (`TraceRowAssembler`) like the 6502's.
- **The tracer** becomes `emma6k-tracer`, in the `cli/` package, and dispatches on the
  header's CPU family. `emma65-tracer` is retired: it's removed from the deb/rpm asset lists,
  and the changelog and wiki say so.

## Implementation: six units

Each unit is its own branch and PR, following the multi-unit workflow. Each must leave
`cargo test --workspace`, CI's clippy commands with `-D warnings`, and `cargo fmt --all --
--check` green. The debugger and SDL crates aren't built by CI for these paths, so each PR also
runs their builds and clippy locally and says so in the PR.

### Unit 1: FIRQ routing in core

- Add the `interrupt` device attribute, handled in `build_machine`.
- Add the `InterruptController` `firq_mask` and `firq_active()`, and make `irq_active()` exclude
  FIRQ-routed devices.
- `MachineSpec` gains the CPU's supported lines. The 6502 declares IRQ and NMI and rejects
  `firq`.
- Tests:
  - routing: a FIRQ-routed device doesn't raise IRQ
  - mask: a FIRQ-routed device is invisible to `irq_recognized`
  - config: a 6502 rejects `firq`, and an unknown line name is rejected
- Benchmark the 6502 against `main` (both sweeps, alternating builds). Check with `nm` that
  `InterruptController::irq_active` and `MachineState::irq_recognized` are still inlined.

### Unit 2: the `emma09` crate: registers, the full instruction set, `Processor`

- Add the workspace member and its `build.rs` (`emma6k-build-info`).
- Registers, decoder, addressing modes including all indexed forms, ALU and condition codes,
  and every documented instruction with its cycle count.
- Implement `Processor`, with interrupt servicing limited to RESET. That's enough for `exec` to
  run and step it.
- Big-endian `WatchContext`, and the register and flag name maps.
- **Integrate W. Schwotzer's MC6809 CPU Test Suite**, as `tests/klaus_dormann_65c02.rs` does
  for the 6502. What the suite is (checked 2026-11-27):
  - Source: `src/tools/cputest.txt` in [aladur/flexemu](https://github.com/aladur/flexemu)
    (last changed upstream in commit `ef85d17`, 2016-06-12), linked from
    [adumont/emu6809](https://github.com/adumont/emu6809).
  - It is one FLEX-assembler source file of about 1,800 lines, `ORG $8100`, which must end below
    `$BFFF`. It uses a scratch area at `EXTREG = $8000` and direct page `$80`.
  - It covers every addressing mode, including all indexed and indirect forms, and the
    instructions with their condition codes: NEG, COM, DEC, INC, CLR, ADD/ADDD/ADC, MUL, SEX,
    SUB/SUBD/SBC, DAA, CMP/CMPD, TST, BIT, the shifts and rotates, the loads, stores and LEAs,
    TFR, and EXG.
  - It does **not** cover interrupts, SWI, RTI, CWAI, SYNC, or cycle counts. Unit 3's own tests
    cover those.
  - It reports results through FLEX monitor entry points: `PSTRNG` (`$CD1E`, prints an
    EOT-terminated string at X), `PUTCHR` (`$CD18`), `PCRLF` (`$CD24`), `OUTDEC` (`$CD39`), and it
    exits by jumping to `WARMS` (`$CD03`). It prints `Failed Test: <mnemonic>` for each failure
    and `All Tests succeded` (sic) if none fail.

  The integration:
  - A small shim ROM at `$CD00` implements those entry points on emma's console device.
  - The test runs until the PC reaches `WARMS`, then asserts that the console shows `All Tests
    succeded` and no `Failed Test:` line.
  - Assemble the suite with `lwasm`. Its FLEX syntax (`IF`/`ENDIF`/`ERR`, `FCC` with `EOT`) may
    need small edits, which are recorded in the vendored copy's header.
  - Check in the built binary next to its source, as `tests/roms/` does for Klaus Dormann's
    suites, so `cargo test` doesn't need `lwasm`.
  - **License:** flexemu is GPL-2.0, and the file has no header of its own. The source and
    binary are test-only and never ship in a package. They go in `emma09/tests/roms/` with a
    note naming the author, the upstream URL and commit, the GPL-2.0 license and its text, and
    our edits. The user approved this on 2026-11-27.
- **Sean Riddle's Williams Emulation Test Suite isn't used.** It is a ROM for Williams arcade
  hardware (PIAs, blitter, watchdog, video beam counter), and its 6809 part is an adapted copy of
  Schwotzer's suite. Its other tests are blitter tests and timing loops judged on screen, which
  emma has no hardware for. It also states no license. Its source is
  [willyemutest04.asm](https://seanriddle.com/willyemutest04.asm).
- Unit tests: opcode-group tests built from byte arrays (no in-tree 6809 assembler yet), checking
  results, flags and cycle counts against the MC6809 programming manual. Add `exec` tests: run,
  breakpoint, step-over of `JSR`/`BSR`/`LBSR`, and step-out.
- Benchmark the 6809 on a minimal RAM + ROM + console machine and record the result, as a new
  baseline rather than a regression gate.

### Unit 3: 6809 interrupts and configuration

- IRQ, FIRQ, NMI (including disarmed-until-S), SWI/SWI2/SWI3, entire and fast stacking, CWAI,
  SYNC, and the halt-state mapping. Device-driven interrupts arrive through Unit 1's routing.
- `emma09::Config`, `CpuVariantSpec` (`"6809"`), the `EmulatorSession` alias, and the 6809's
  device registry (without the 6502-only modules).
- Tests:
  - each interrupt kind's stacking and vectors
  - FIRQ preempting IRQ
  - CWAI and SYNC waking
  - an MC6850 routed to FIRQ driving the CPU end to end through `Config::build`

### Unit 4: the `emma6k` binary and the `emma65`/`emma09` wrappers

- Add the `cli/` package. Move the CLI into it.
- Add the `cpu`/`cpu-variant` key with its family mapping, the "no CPU" error, the `EMMA6K_`
  prefix, and the `run_session<P>` dispatch.
- Merge the template list, with per-CPU defaults and profile/CPU conflict errors.
- Add the wrappers.
- Add the 6502 `Config`'s `cpu` alias and its error for 6809 values, and switch the debugger
  to `EMMA6K_`.
- `--trace-file` with the 6809 is an error (until Unit 6).
- Move `tests/emulator_binary.rs` and add tests for:
  - the wrappers
  - the "no CPU" error
  - `cpu` and `cpu-variant` both set
  - a wrapper rejecting another family's variant or profile
  - `EMMA6K_CPU_VARIANT` taking effect, and `EMMA65_CPU_VARIANT` being ignored
  - a 6809 run from a small inline ROM
- Update packaging, CI path filters, the release skill, `CLAUDE.md`, and the wiki.
- Benchmark the 6502 against `main` to confirm the dispatch adds nothing per instruction.
- Manual UAT: `emma65` with no configuration (TaliForth), `emma65 --profile snake`, and
  `emma65 --trace-file` decoded by the tracer. Also check that `emma09` with no configuration
  prints its "no default profile yet" message.

### Unit 5: the CamelForth template

- Vendor the source, rewrite the console I/O, add `build.sh`, the template TOML, and the
  checked-in `program.bin`/`program.lbl`.
- Register `camelforth` as the 6809's default template.
- Add the boot-and-evaluate integration test. Add the license notice and the wiki's template
  entry.
- Manual UAT: `emma09` with no configuration reaches the `ok` prompt. A few Forth words work,
  and Ctrl+C behaves as the template documents.

### Unit 6: trace format v3, the 6809 disassembler, the tracer

- Move the generic trace framing into core and add the v3 header. Keep v2 readable, and have
  the 6502 write v3 with unchanged record bytes.
- Add the 6809 register-record codec, `emma09::disassembler`, and trace row reconstruction.
- Rename the tracer to `emma6k-tracer` in the `cli/` package and retire `emma65-tracer`. Add
  dispatch on the header's CPU family, and `--trace-file` support for the 6809 in `emma6k`.
- Tests:
  - v2 files still decode
  - a 6502 v3 round trip
  - a 6809 v3 round trip
  - the disassembler against a table of every opcode and addressing form
- Locally: the debugger's trace window still records and displays a 6502 trace.
- Manual UAT: record a CamelForth session with `emma09 --trace-file` and decode it.

### Compile dependencies between units

- Unit 1 depends on nothing.
- Unit 2 uses only core.
- Unit 3 needs Unit 1's FIRQ API and Unit 2's crate.
- Unit 4 needs Unit 3's `emma09::Config`.
- Unit 5 needs Unit 4 only for `--profile` and the default template. Its integration test uses
  `emma09::Config::build` directly.
- Unit 6 touches the trace code in `src/mos6502/cpu/trace.rs`, which no earlier unit changes.

Check this again against the actual file lists before starting each unit.

## Not in step 2

- Any debugger support for the 6809:
  - register descriptions
  - stack, disassembly and assembler panels
  - a UI FIRQ control
  - 6809 profiles in the New Profile dialog

  All of this is step 3. In step 2 the debugger only rejects non-6502 profiles clearly.
- A 6809 assembler.
- The 6309 variant, and the 6800/6801/6803 family.
- The HALT/DMA line.
- Splitting the apt/rpm package by CPU.

## Open decisions

None. The last two were settled on 2026-11-27:

- **Wrapper semantics** are as described under "The `emma6k` binary and its wrappers". A wrapper
  fixes the family, supplies that family's default variant only when nothing names one, and
  rejects another family's variant or profile.
- **Vendoring W. Schwotzer's GPL-2.0 test suite** is approved: test-only, never packaged, with
  the license notice described in Unit 2.

## Host notes

- On the Linux workstation, `gforth` (snap) and `lwasm` are installed (checked 2026-11-27). Unit 2
  needs `lwasm` to rebuild the test suite's binary, and Unit 5's `build.sh` needs GForth. Normal
  builds and `cargo test` need neither, because both outputs are checked in. The macOS workstation hasn't been checked.
