---
name: emma09-mutation-testing
description: The cargo-mutants pass over emma09 ran 2026-11-28 — results, the gaps it found, the equivalent mutants left, and how to rerun it without filling /tmp
metadata:
  type: project
---

On 2026-11-27, during step 2's Unit 2, the user asked whether to mutation-test the 6809, since W. Schwotzer's suite is far less widely used than Klaus Dormann's 6502 suites. They chose to set it up then and run it **later, before wrapping up the 6809 support** ([[multi-cpu-effort]]).

Set up on the Linux workstation: `cargo-mutants` 27.1.0 is installed (`cargo install --locked cargo-mutants`; check the Mac before relying on it there). `.cargo/mutants.toml` scopes it to `emma09/src/**/*.rs` and tests only the mutated package. Run `cargo mutants -p emma09`, optionally with `-j 4`. At Unit 2 it listed 893 mutants, 642 of them in `execute.rs`. Expect 20–40 minutes, so run it in the background.

**Why:** mutation testing shows which code changes no test catches. Schwotzer's expectations are trustworthy (validated on a real EF6809P), but its coverage is partial (no interrupts, SWI/RTI, CWAI/SYNC or cycle counts). It can't catch an error the code and a unit test share, such as a misread datasheet cycle count; CamelForth booting (Unit 5) is the independent check for that.

**Results (2026-11-28, Linux):** 1,111 mutants in 22 minutes with `-j 4`: 792 caught, 67 timeouts (these count as caught), 74 unviable, 178 missed. The new tests (#678, merged 2026-11-28) caught 92 of the 178. No CPU bug turned up; every new test passed against the code as written. The gaps were mostly flags an instruction should clear and Schwotzer's suite never checks (AND/OR/EOR/STA/LD/ST/TST/CLR/ASL/ASR/COM/SEX), plus EOR, STU, TFR from S/PC, `,U+`, branch conditions, DAA edges, indexed extra cycles, watch names, the builder, and the trace assembler. The 86 left are equivalent: `|` → `^` on disjoint bits (84), `Cpu::builder` → `Default` and `variant_to_byte` → 0.

**How to apply:** rerun after a large CPU change, not in CI. Set `TMPDIR` to a directory on disk (for example `target/mutants-tmp`) and use `--output target/mutants-out`: each job copies and builds the workspace in the temp directory, and `/tmp` is a 2.6 GB tmpfs on the Linux host. `--iterate` with the same output directory reruns only the mutants not yet caught. Expect the 86 equivalent survivors; anything else is new.
