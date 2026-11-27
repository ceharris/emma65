---
name: multi-cpu-effort
description: Plan and decisions for adding 6809 (and other 6800-family) support in this repo behind a Processor trait; step 1 plan is in plan/multi-cpu-step1-core-refactor-plan.md
metadata:
  type: project
---

**Goal:** add the Motorola 6809, and possibly the 6309, 6800, 6801 and 6803, to this project without forking it. The user especially wants to reuse the non-blocking peripheral architecture (bus, `IoDevice`, transports and relays, device protocols, config registry, watch pipeline, SDL peripheral crates). The MC6850 and MC6840 devices are native 6800-family parts.

**Agreed approach (2026-11-26):**
- Stay in one repository.
- Use neither Cargo features nor a runtime switch inside the existing `Cpu`.
- Put a CPU-agnostic core behind a `Processor` trait, with one crate per CPU family.
- The run loop is `run_loop<P: Processor>`, compiled separately for each CPU. Choose the CPU once at session creation (an enum of sessions). Never `Box<dyn Processor>` on the hot path.
- `Bus` stays concrete: PR #643 regressed throughput with a dynamic hop on memory access, and #644 had to restore the fast path.

**Steps:**
1. A refactor with the 6502 as the only CPU and no behavior change. The plan is `plan/multi-cpu-step1-core-refactor-plan.md`, merged to `main` with #661 on 2026-11-26. It has four PRs:
   - PR 1: `MachineState` extraction.
   - PR 2: `Processor` trait plus generic `exec`.
   - PR 3: core/6502 module boundary inside the crate, plus a toy test `Processor`.
   - PR 4: extract the core crate.
2. A 6809 crate for the CLI. Adds `cpu = "6809"`, FIRQ/HALT interrupt lines, trace format v3 with a per-variant register record, and a 6809 template.
3. Generalize the debugger: register descriptions sent from the backend, and processor-driven stack/disassembly/assembler panels.

**User decisions (2026-11-26):**
- Core crate name `emma6k-core`. It was first chosen as `emma-core`, then changed on 2026-11-26 to follow the rename's prefix rule.
- Rename the project to emma6k before the refactor. Done in #661, merged 2026-11-26; see [[emma6k-rename]].
- The clean stop is reported as an explicit `StepResult::StopRequested` variant, not a synthesized NOP.
- The core crate is versioned in lockstep with `emma65`.
- PR 4's timing is decided only after PRs 1–3 are reviewed and merged.

**Where we left off (2026-11-27):** step 1 is complete. PRs 1–3 merged as #664–666, and PR 4 (the `emma6k-core` crate in `core/`) merged as #668 on 2026-11-27. The step 2 plan, `plan/multi-cpu-step2-mc6809-plan.md` (six units, no open decisions), was approved and merged in #669 on 2026-11-27. Unit 1 (FIRQ routing in core) merged as #670 on 2026-11-27. Unit 2 (the `emma09` crate) was implemented on 2026-11-27 on branch `multi-cpu-step2-unit2-emma09` and is in review as a PR. Next is Unit 3 (6809 interrupts and configuration); start it only when the user says so, and only after Unit 2 merges. Before the 6809 work wraps up, run the deferred mutation-testing pass ([[emma09-mutation-testing]]).

**What Unit 1 built, for Units 2–3:** `emma6k_core::InterruptLine { Irq, Firq, Nmi }`. `MachineSpec.interrupt_lines` lists the CPU's inputs (the 6502's are `mos6502::config::INTERRUPT_LINES`). `build_machine` strips the `interrupt` device attribute (`config::INTERRUPT_ATTRIBUTE`) and sets `InterruptController::set_firq_mask` from the IRQ-capable device IDs (< `MAX_IRQ_SOURCES`) each FIRQ-routed spec added, found with the new `BusConfig::device_ids()`. `irq_active()` and `active_sources_mask()` (so also `irq_recognized` and `pic_finch`) leave FIRQ-routed sources out; `firq_active()` sees only those; `consume_irq_pulses` leaves FIRQ-routed pulses pending. The 6809 will need its own FIRQ check (`interrupts().firq_active()` and its F flag) next to `irq_recognized`. FIRQ-routing a device with no interrupt output (RAM, ROM, a display without a keyboard) is an error. **The wiki doesn't document `interrupt` yet**, on purpose: on the 6502 only `irq` is valid. Add it to `doc/src/io-devices.md` in Unit 3, when the 6809 makes `firq` usable.

**Unit 1 benchmark finding (Linux, 2026-11-27):** in the clock-bench binary the default complement's top speed dropped ~4.5% against same-day `main` (38.8–39.4 vs 40.1–41.0 MHz, 4 interleaved pairs); the minimal complement didn't change measurably. `perf stat` on a separate unthrottled probe binary showed it's code layout, not work: host instructions per emulated cycle rose only ~0.15% (320.6 → 321.1), branch misses were unchanged, and in that binary the branch was ~2% *faster* than `main`. Speed differences tracked op-cache misses and frontend stalls. `nm` symbol sets were identical and `irq_active`/`irq_recognized`/`Cpu::execute` stayed inlined. All of this is in PR #670.

**What Unit 2 built, for Unit 3:** `emma09/` is a library crate (no `build.rs`: its binary lives in `cli/` from Unit 4, so nothing would read the build-info variables). `Cpu::step` handles a `Waiting` halt (CWAI/SYNC) by `tick_idle` and returns `Waiting` unless a reset is pending; Unit 3 adds the wake conditions and the NMI/FIRQ/IRQ checks right after `take_reset` in `step`. CWAI already sets E and pushes the entire state before waiting, so its interrupt service must skip the push. Every explicit load of S (LDS, LEAS, TFR/EXG to S, PULU S) goes through `Cpu::load_s` in `execute.rs`, the one place to arm NMI. Vector constants (`RESET_VECTOR` … `SWI3_VECTOR`) and `software_interrupt()` (entire push, optional I|F mask, vector through `resolve_vector`) are in place. Undocumented opcodes and undefined indexed postbytes follow `InvalidOpcodePolicy` (default `Nop`). CI's core job now builds, lints and tests `-p emma09`, and the pages workflow documents it; Unit 4 still adds `cli/**`. The release skill already bumps `emma09/Cargo.toml` with `emma65`. **Test suite assembler:** lwasm for now (the user's choice); the user's own ca69 assembler can't yet, because its `as9` and `a09` dialects reject `LEAY [n16]` (extended-indirect LEA) though native ca69 accepts it, a translator bug filed as ceharris/ca69#79 (2026-11-27). Switch `emma09/tests/roms/build.sh` to `ca69 --dialect as9` once it's fixed (the 3-line `IF/ERR/ENDIF` guard would also need deleting).

**6809 baseline (Linux, 2026-11-27):** see [[clock-bench-baseline]].

**Step 2 user decisions (2026-11-27):**
- A CPU-neutral `emma6k` binary picks the CPU. `emma65` and `emma09` are thin wrappers that exec `emma6k` with the CPU fixed.
- `cpu` is a synonym for `cpu-variant`. Its values name a variant (`65c02`, `wdc65c02`, `6809`, `6309`), and the variant implies the family. `emma6k` has no default CPU; the wrappers supply it.
- The tracer becomes `emma6k-tracer`. `emma6k` and `emma09` are versioned in lockstep with `emma65`. Env vars switch to `EMMA6K_` with no `EMMA65_` fallback, in the debugger too. These reverse two names the rename kept ([[emma6k-rename]]).
- CamelForth's GPL-3 is fine if the template carries the license details.
- ISA test suite: W. Schwotzer's MC6809 CPU Test Suite, `src/tools/cputest.txt` in github.com/aladur/flexemu (GPL-2.0; found via github.com/adumont/emu6809). It covers no interrupts and reports through FLEX monitor entry points at `$CDxx`, which a shim must provide. Sean Riddle's suite (seanriddle.com/willyemutest04.asm) was rejected: it targets Williams arcade hardware, embeds a copy of Schwotzer's tests, and states no license.
- The 6809 crate is `emma09`; the 6309 would be a variant inside it.
- Trace v3 is in step 2, as its last unit.
- The 6809 template is CamelForth (GPL-3) from github.com/nealcrook/multicomp6809, rebuilt with GForth per its wiki's CamelForthRebuildANS page, with `KEY`/`KEY?`/`EMIT` rewritten for emma's console device. The user has done this rebuild before.

How step 1 ended up, for step 2:
- **Crates.** `emma6k-core` (`core/`) has its modules at the crate root: `emma6k_core::{bus, config, device, exec, session, transport, watch, ...}`. `emma65` (root) is the 6502 plus a facade, `src/emulator.rs`, that does `pub use emma6k_core::*`, adds the 6502 re-exports, and has a local `config` module adding `Config`/`CpuVariantSpec`/`templates`. Everything outside the workspace's `emma65` code uses `emma65::emulator::*` paths.
- **A CPU crate's toolkit.** Each CPU builds on `MachineState`'s public `#[inline]` accessors (`bus`/`bus_mut`, `interrupts`/`interrupts_mut`, `breakpoints_mut`, `evaluator_mut`, `cycles`, `reset_cycles`, `resolve_vector`, `log_sender`) and its helpers (`check_halt` with a closure-built `WatchContext`, `finish_cycle`, `tick_idle`, `irq_recognized`). `config::build_machine` builds a `MachineState`. Core also exports `log_channel` and `log_msg!`. The fields stay private. `core/src/exec/toy.rs` is a complete minimal CPU.
- **Cross-crate inlining is the performance trap.** A non-generic core function called from a CPU crate inlines only if it's `#[inline]`. Without hints, PR 4 was 7.5% slower (`Bus::write`, `tick_devices`, and `InterruptController::bit_index`/`take_*`/`*_pending` went out-of-line). With hints, it's about 3% *faster* than PR 3. Keep the hot paths `#[inline]`, and compare `nm -C target/release/examples/clock_bench_minimal | grep -E '(bus::Bus|interrupt::InterruptController|cpu::Cpu|machine::MachineState)>::'` against `main` in every PR that touches them. `lto = "thin"` wasn't needed.
- **Tests.** 6502-driven tests of core code live in `src/mos6502/exec_tests.rs` and `device_tests.rs`. Core's own `exec` coverage is the toy-CPU tests.
- **Lockstep version.** `core/Cargo.toml`, the root `version`, and the root's `emma6k-core` dependency `version` must match. The release skill knows this.
- **Carried-over decisions:** `Config` lives in `mos6502::config` (step 2's `cpu = "..."` selector goes wherever the choice between CPU sessions is made). Generic `exec` types have 6502 aliases. `Cpu` keeps its inherent accessors. The `pic_finch` device has a local `IRQ_VECTOR`.
- **Toolchain:** rustc 1.100.0 on Linux since 2026-11-26, matching CI's stable. Its new `unused_dependencies` lint warns about a normal dependency used only by tests, so put those under `[dev-dependencies]`.

**CI doesn't build the debugger or the SDL crates for `src/`- or `core/`-only changes**, and that's deliberate; see [[ci-path-filters-deliberate]]. For a refactor PR, verify them locally and say so in the PR.

**To resume in a new session:**
1. Implement one unit per branch and PR ([[multi-unit-plan-workflow]]), re-checking compile dependencies against the actual file lists before each unit.

**How to apply:** follow the resume steps above. A/B-benchmark every PR that touches a hot path against `main`, alternating builds, using [[clock-bench-baseline]]'s post-step-1 row for the current host. Re-check `nm` for `Cpu::execute` staying inlined into `step`: PR 1's ~14% gain came from that, and every later PR kept it.
