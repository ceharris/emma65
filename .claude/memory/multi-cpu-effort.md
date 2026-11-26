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

**Where we left off (2026-11-26):** PR 1 (`MachineState` extraction) is open as #664 on branch `refactor/extract-machine-state`, awaiting the user's UAT and review. Don't start PR 2 until the user says so ([[multi-unit-plan-workflow]]).

Notes from PR 1 for PR 2:
- **`check_halt` takes a closure**, `|bus| CpuWatchContext { regs, bus }`, not the plan's `&dyn WatchContext`. A context borrowing `machine.bus` can't coexist with `&mut self` on the same `MachineState`. The `Processor` trait should keep this shape.
- **PR 1 made the 6502 about 14% faster** on the minimal complement and about 4% on the default one (A/B on Linux, minimal top speed 90 → 103 MHz). `nm` showed why: `Cpu::execute` stopped being a separate function and was inlined into `step`. That's an LLVM size heuristic, so PR 2's generic `run_loop`/`step` could undo it. Compare PR 2 against PR 1's numbers, not `main`'s, and re-check with `nm -C target/release/examples/clock_bench_minimal | grep 'Cpu>::'`.
- `finish_cycle` stays out-of-line, both before and after the move, despite `#[inline]`. `bus_read`/`bus_write`/`push`/`pop` were out-of-line on `main` too.

**How to apply:** follow the notes above. Benchmark each PR against [[clock-bench-baseline]] for the current host. The step 1 plan document already uses `emma6k-core`.
