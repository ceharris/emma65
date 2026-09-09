---
name: clock-speed-benchmark
description: Revalidate emma65's clock-speed throttling accuracy and ceiling — the numbers documented in doc/src/the-emulator-core.md's "Clock Speed Simulation" section (~34 MHz bundled default, ~85 MHz minimal RAM+ROM+console). Use when the user asks to benchmark/validate/revalidate emulator performance or clock-throttling accuracy, or after a change that could affect the CPU's hot memory-access or per-instruction device-polling path (Bus, IoDevice::tick, exec::run) and its performance impact needs checking.
---

# Clock-speed benchmark

Measures how accurately `emulator::exec::run()`'s free-running loop holds a target clock speed,
and where that accuracy falls off, using two throwaway example binaries against a release build.
This is the exact methodology behind the numbers published in `doc/src/the-emulator-core.md`
("Clock Speed Simulation" section) — reuse it rather than inventing a new benchmark, so results
stay comparable across runs.

Background on what's being measured, the current baseline figures, and when (if ever) to update
the wiki from a new result: see `reference.md`. Load it before step 4 (interpreting results) or if
you need to widen the target-speed brackets.

## Runbook

**1. Confirm a clean starting point.** Run `git status --porcelain`. If there are unrelated
uncommitted changes, tell the user before proceeding — this skill adds and then removes files
under `examples/`, and you don't want to clean up someone else's in-progress work by mistake.

**2. Set up.**

```bash
.claude/skills/clock-speed-benchmark/scripts/setup.sh
```

Copies `assets/clock_bench.rs` and `assets/clock_bench_minimal.rs` into `examples/` and builds
both in release mode. These files are not meant to be committed or added to `Cargo.toml` — they
exist only for this run. If the copied example fails to compile because the public API it uses
has moved (`Config`, `DeviceRegistry`, `templates::materialize_default`,
`emma65::emulator::run`, `RunHandle::take_cpu`, `Cpu::cycles`), fix the copy under `examples/`
to match current signatures — don't just work around the failure — but leave
`assets/*.rs` as-is unless the user asks you to update the skill itself (see final step).

**3. Run both benchmarks**, each printing a target-vs-actual MHz table:

```bash
./target/release/examples/clock_bench
./target/release/examples/clock_bench_minimal
```

`clock_bench` sweeps the bundled default device complement (32K RAM, TaliForth ROM, VIA, two
ACIAs, LFSR, console) across ~26 target speeds bracketing the ~34 MHz baseline, then reports the
unthrottled ceiling. `clock_bench_minimal` runs a shorter sweep of the same full complement
alongside a RAM+ROM+console-only complement bracketing the ~85 MHz baseline, for a same-run
comparison of how device count affects the ceiling. Each target speed runs for 3 simulated
seconds, so expect roughly 90 seconds for `clock_bench` and 60 seconds for
`clock_bench_minimal` — run them with a timeout comfortably longer than that rather than polling.

**4. Interpret the results.** Read `reference.md` if you haven't yet. For each sweep: the
"accurate ceiling" is the highest target where error stays near 0.00%; above it, error should grow
monotonically worse as the target climbs. Compare both the accurate ceiling and the unthrottled
ceiling against the baseline table in `reference.md`. A run within ~5-10% of baseline is normal
noise, not a regression. If a sweep didn't actually observe the falloff (error is still ~0.00% at
the top of the bracketed range), see reference.md's "Adjusting the target-speed brackets" section
before concluding anything.

**5. Clean up.**

```bash
.claude/skills/clock-speed-benchmark/scripts/cleanup.sh
```

Removes the copied example files and fails loudly if the working tree isn't clean afterward —
don't skip this or leave throwaway benchmark code sitting in the tree.

**6. Report** the two ceiling figures (accurate + unthrottled) for each device complement, how
they compare to the documented baseline, and your conclusion (confirms current performance /
flags a regression worth investigating). Only propose updating `doc/src/the-emulator-core.md` or
this skill's `reference.md` baseline table if results land meaningfully and repeatably outside
baseline on comparable hardware — and even then, ask the user first rather than editing the docs
unilaterally from a single run.
