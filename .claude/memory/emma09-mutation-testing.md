---
name: emma09-mutation-testing
description: A cargo-mutants pass over the 6809 CPU is set up but deliberately deferred until just before the 6809 work (step 2) wraps up
metadata:
  type: project
---

On 2026-11-27, during step 2's Unit 2, the user asked whether to mutation-test the 6809, since W. Schwotzer's suite is far less widely used than Klaus Dormann's 6502 suites. They chose to set it up then and run it **later, before wrapping up the 6809 support** ([[multi-cpu-effort]]).

Set up on the Linux workstation: `cargo-mutants` 27.1.0 is installed (`cargo install --locked cargo-mutants`; check the Mac before relying on it there). `.cargo/mutants.toml` scopes it to `emma09/src/**/*.rs` and tests only the mutated package. Run `cargo mutants -p emma09`, optionally with `-j 4`. At Unit 2 it listed 893 mutants, 642 of them in `execute.rs`. Expect 20–40 minutes, so run it in the background.

**Why:** mutation testing shows which code changes no test catches. Schwotzer's expectations are trustworthy (validated on a real EF6809P), but its coverage is partial (no interrupts, SWI/RTI, CWAI/SYNC or cycle counts). It can't catch an error the code and a unit test share, such as a misread datasheet cycle count; CamelForth booting (Unit 5) is the independent check for that.

**How to apply:** when step 2's last unit is done, or the user says to wrap up the 6809, run the pass. Triage each surviving mutant into a new test, or a note on why it's equivalent. Report the results in that PR. It's not a CI job.
