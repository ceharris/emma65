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

**Where we left off (2026-11-26, end of session):** the rename is done ([[emma6k-rename]]). After the #661 merge (`6ee4814`), `main` has only memory commits. The working tree is clean, and no branch or PR is open. No step 1 code has been written. To resume:
1. **On Linux,** re-take the benchmark baseline on current `main` before writing any code ([[clock-bench-baseline]]). Its Linux row dates from 2026-09. The plan's "Performance baseline" table holds the macOS numbers, and the plan is frozen ([[plan-frozen-doc-current]]), so record new numbers in the memory file, not the plan.
2. **On macOS,** do the checkout follow-up from [[emma6k-rename]] first. Its baseline at `afba63f` still stands, because the rename didn't touch the hot path.
3. Cut a branch from a freshly fetched `origin/main` ([[branch-from-origin-main]]) and implement PR 1 (`MachineState` extraction) following the plan's "PR 1" section. Then do the plan's "Verification for each PR" checklist. Before starting, check that each of the four PRs compiles on its own ([[multi-unit-plan-workflow]]).
4. Open PR 1, then stop for the user's review before starting PR 2.

**How to apply:** follow the steps above. Benchmark each PR against [[clock-bench-baseline]] for the current host. The step 1 plan document already uses `emma6k-core`.
