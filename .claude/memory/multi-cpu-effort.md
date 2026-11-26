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

**Where we left off (2026-11-26):** PRs 1–3 merged (#664–666). The user chose to do PR 4 now rather than defer it to step 2 (2026-11-26). PR 4 (the `emma6k-core` crate in `core/`) is open as #668 on branch `refactor/extract-core-crate`, awaiting the user's UAT and review. Once it merges, step 1 is complete. Step 2 is the 6809 crate for the CLI. Don't start step 2 until the user says so, and plan it first: there's no step 2 plan document yet.

How step 1 ended up, for step 2:
- **Crates.** `emma6k-core` (`core/`) has its modules at the crate root: `emma6k_core::{bus, config, device, exec, session, transport, watch, ...}`. `emma65` (root) is the 6502 plus a facade, `src/emulator.rs`, that does `pub use emma6k_core::*`, adds the 6502 re-exports, and has a local `config` module adding `Config`/`CpuVariantSpec`/`templates`. Everything outside the workspace's `emma65` code uses `emma65::emulator::*` paths.
- **A CPU crate's toolkit.** Each CPU builds on `MachineState`'s public `#[inline]` accessors (`bus`/`bus_mut`, `interrupts`/`interrupts_mut`, `breakpoints_mut`, `evaluator_mut`, `cycles`, `reset_cycles`, `resolve_vector`, `log_sender`) and its helpers (`check_halt` with a closure-built `WatchContext`, `finish_cycle`, `tick_idle`, `irq_recognized`). `config::build_machine` builds a `MachineState`. Core also exports `log_channel` and `log_msg!`. The fields stay private. `core/src/exec/toy.rs` is a complete minimal CPU.
- **Cross-crate inlining is the performance trap.** A non-generic core function called from a CPU crate inlines only if it's `#[inline]`. Without hints, PR 4 was 7.5% slower (`Bus::write`, `tick_devices`, and `InterruptController::bit_index`/`take_*`/`*_pending` went out-of-line). With hints, it's about 3% *faster* than PR 3. Keep the hot paths `#[inline]`, and compare `nm -C target/release/examples/clock_bench_minimal | grep -E '(bus::Bus|interrupt::InterruptController|cpu::Cpu|machine::MachineState)>::'` against `main` in every PR that touches them. `lto = "thin"` wasn't needed.
- **Tests.** 6502-driven tests of core code live in `src/mos6502/exec_tests.rs` and `device_tests.rs`. Core's own `exec` coverage is the toy-CPU tests.
- **Lockstep version.** `core/Cargo.toml`, the root `version`, and the root's `emma6k-core` dependency `version` must match. The release skill knows this.
- **Carried-over decisions:** `Config` lives in `mos6502::config` (step 2's `cpu = "..."` selector goes wherever the choice between CPU sessions is made). Generic `exec` types have 6502 aliases. `Cpu` keeps its inherent accessors. The `pic_finch` device has a local `IRQ_VECTOR`.
- **Toolchain:** rustc 1.100.0 on Linux since 2026-11-26, matching CI's stable. Its new `unused_dependencies` lint warns about a normal dependency used only by tests, so put those under `[dev-dependencies]`.

**CI doesn't build the debugger or the SDL crates for `src/`- or `core/`-only changes.** Their jobs are path-filtered to their own directories (`debugger/**` and so on) even though they depend on `emma65` and `emma6k-core`. In #664 they were skipped. For a refactor PR, run `cargo test --workspace` and the debugger's clippy command locally, and say in the PR that you did.

**Open offer (2026-11-26, not yet answered):** add `src/**`, `core/**` and the root `Cargo.toml` to the path filters of CI's debugger and SDL jobs, so changes to the libraries they depend on build them. Raise it again if the user doesn't. Do it as its own small PR, not inside a refactor PR.

**To resume in a new session:**
1. Run `gh pr view 668 --json state`. If it's still open, wait for the user's review feedback and address it on `refactor/extract-core-crate`.
2. If it merged, fast-forward `main`, delete the local branch, and update this file on `main` to say step 1 is complete (#664–668). There's no PR left, so commit it straight to `main` per [[git-workflow]].
3. Ask the user what's next: step 2 (the 6809), the CI path-filter fix above, issue #667, or the macOS port ([[macos-port-effort]]).
4. For step 2, write a plan document first (`plan/multi-cpu-step2-...md`), as was done for step 1. The step 1 plan's "Not in step 1" section lists its scope: a 6809 crate, the `cpu = "..."` selector, an enum choosing the session at startup, FIRQ/HALT interrupt lines, trace format v3, and a 6809 template. Check its unit split for compile dependencies ([[multi-unit-plan-workflow]]) and get the user's decisions before implementing.

**How to apply:** follow the resume steps above. A/B-benchmark every PR that touches a hot path against `main`, alternating builds, using [[clock-bench-baseline]]'s post-step-1 row for the current host. Re-check `nm` for `Cpu::execute` staying inlined into `step`: PR 1's ~14% gain came from that, and every later PR kept it.
