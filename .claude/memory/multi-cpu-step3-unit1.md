---
name: multi-cpu-step3-unit1
description: Multi-CPU step 3 Unit 1 (CPU description, register snapshot, trace rows; 6502 only) — PR #683 open 2026-11-28; what it built and where Units 2–3 extend it
metadata:
  type: project
---

Step 3's plan is `plan/multi-cpu-step3-debugger-plan.md`, merged in #682 on 2026-11-28. Its open decisions were all taken as recommended ([[multi-cpu-effort]]), so the plan's "Open decisions" section still reads as open, but it's settled. Unit 1 is implemented in PR #683 (branch `step3-unit1-cpu-description`), opened 2026-11-28 and awaiting the user's UAT. The touch-point survey below is kept for reference. Branch from `origin/main` ([[branch-from-origin-main]]), with a name like `step3-unit1-cpu-description`, and follow [[multi-unit-plan-workflow]].

**Unit 1's scope:** the 6502 only, with no visible behavior change. It adds a backend `CpuDescription`, makes `RegisterSnapshot` a `values` map, makes `set_register` take a register name, adds `TraceRowDto.registers`, and drives the frontend's Register panel, Trace panel and status-bar interrupt controls from the description. The stack window and byte order belong to Unit 2. `DebugCpu` and the 6809 belong to Unit 3.

**Backend touch points (`debugger/src-tauri/src/`), checked 2026-11-28:**
- `RegisterSnapshot` is built in **six** places, not the five the plan says. They are `registers.rs` (`set_register` ~96, `get_registers` halted ~127 and live ~149), `disassembly.rs` (`step_into` ~104, `finish_run` ~446) and `cpu_bus.rs` (`reset_cpu` ~112). Replace them all with one helper.
- `ChangedFlagsState(Mutex<u8>)` and the `p_before ^ regs.p.to_byte()` pattern repeat in `step_into`, `step_over`, `step_return` (`disassembly.rs`) and `set_register`. Widening it to `u32` is enough for Unit 1.
- `RegisterField` enum in `registers.rs` becomes a name string. Validate width per register from the description.
- `lib.rs:570` emits `session-loaded` with `()`. The Display, LED-matrix and LCD panels listen to it and ignore the payload, so adding the description to it is safe. Also add a `get_cpu_description` command, registered in lib.rs's `invoke_handler` list, for late-mounting panels.
- `trace.rs`: `TraceRowDto` is built ~277 with `a/x/y/s/p/pc` from `row.regs`.

**Frontend touch points (`debugger/frontend/src/`):**
- `RegisterPanel.tsx` exports `RegisterSnapshot` (fields `a,x,y,s,pc,p,changed_flags,…`). It has the `RegisterField` type and `FLAG_CHARS` (N V - B D I Z C). `ExecutionContext.tsx` and `StatusBar.tsx` import that type.
- `DisassemblyPanel.tsx:47` and `RunControlsContext.tsx:17` each declare their **own local copy** of `RegisterSnapshot`. Disassembly reads `snap.pc`. Consider keeping a top-level `pc` field next to `values`, since many consumers want only the PC. If you keep it, say so in the PR, because the plan lists only `values`.
- `TracePanel.tsx`: the header has `<th>A X Y S P Flags</th>` (~372–377). The cells are at ~414–419, with its own `FLAG_BITS`/`formatFlags` (~47–60).
- `StatusBar.tsx`: the NMI and IRQ controls are at ~154–253.
- Tests to update: `RegisterPanel.test.tsx`, `ExecutionContext.test.tsx`, `RunControlsContext.test.tsx`, `StatusBar.test.ts`, plus the backend modules' `#[cfg(test)]` blocks.

**Verification for the PR:** `cargo test -p emma6k-debugger`, `cargo clippy -p emma6k-debugger --all-targets -- -D warnings` ([[ci-clippy-deny-warnings]]) and `cargo fmt --all`. In `debugger/frontend` run `npm run lint`, `npm run format:check` and `npm test`, which is what CI runs. GUI UAT is the user's ([[gui-uat-is-the-users]]). The PR's UAT steps are the plan's Unit 1 list: register edits in each radix, the flags row, trace columns and the status bar all unchanged on a TaliForth profile. Update `doc/src/the-debugger.md` only if user-visible text changes ([[wiki-user-facing-writing]]).

**What #683 built, for Units 2–3:**
- Backend `debugger/src-tauri/src/cpu.rs`: `CpuDescription` and `describe_6502(variant)`; `CpuDescriptionState`, set in `lib.rs` just before `session-loaded`; plus `register_values`, `flags_value` and `set_register_value`, all on the 6502's `Registers`. Unit 3 turns these into `DebugCpu` methods. `ByteOrder` has only `Little`, since clippy denies an unused `Big`, so Unit 3 adds `Big`. `RegisterSnapshot::new(regs, changed, stopped, waiting, bp_hit)` lives in `registers.rs`.
- The snapshot kept a top-level `pc` beside `values`, and the PR says so.
- Frontend: `cpuDescription.ts` (types, `flagMask`, `isUnusedFlag`: an unused bit is the one labelled `-`), `CpuDescriptionContext.tsx` (provider inside `App.tsx`, around `ExecutionProvider`), `traceFormat.ts` (trace column helpers), and `test/cpuFixtures.ts` (`DESCRIPTION_6502`, `snapshot6502`). The status bar gates NMI and IRQ on `interrupt_lines`, and Unit 3 adds FIRQ the same way.
- Register panel grouping: data and index registers go in the data-radix table; pc, stack and flags go in the address table, in description order. The 6502 order is A X Y PC S P.

**Why:** a fresh session should start Unit 1 without re-surveying the debugger.
**How to apply:** if #683 gets review changes, work on its branch. Once it merges, move the "what #683 built" notes into [[multi-cpu-effort]], record the merge there, and delete this file.
