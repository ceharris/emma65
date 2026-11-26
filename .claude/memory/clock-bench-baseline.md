---
name: clock-bench-baseline
description: Clock-speed benchmark baselines per workstation (Linux Ryzen 5 7530U from 2026-09, macOS i9 from 2026-11-26); compare refactors against the current host's numbers
metadata:
  type: project
---

Benchmark baselines are specific to each machine, so compare only against numbers from the same host. The figures documented in `doc/src/the-emulator-core.md` (~34 / ~85 MHz) came from the Linux workstation (confirmed 2026-11-26: its CPU is the AMD Ryzen 5 7530U).

**macOS workstation:** Intel Core i9-9880H @ 2.30GHz, 16 threads, release build, measured 2026-11-26 at commit `afba63f` (main), before the multi-CPU refactor ([[multi-cpu-effort]]).

| Device complement | Throttling accurate up to | Unthrottled top speed |
|---|---|---|
| Bundled default | ~26 MHz (0.00% error through 26; −0.17%/−0.19% at 27–28) | 27.4–28.1 MHz (3 runs) |
| Minimal (RAM, ROM, console) | ~50–54 MHz (noisy; ≤0.3% error through 54) | 53.8–55.8 MHz (2 runs) |

On that Mac, the skill's default minimal sweep range (70–100 MHz) sits entirely above the top speed. Re-bracket the copied `examples/clock_bench_minimal.rs` to roughly 30–56 MHz. Results near the top speed are noisy there (laptop turbo/thermal behavior), so compare the accurate-throttling numbers across at least two runs.

**Linux workstation:** AMD Ryzen 5 7530U, 12 threads, release build. These numbers are older and were measured before the latest changes to main:

| Device complement | Throttling accurate up to | Unthrottled top speed | Measured |
|---|---|---|---|
| Bundled default | ~34 MHz (~0.00% error) | 34.15 MHz | 2026-09-09, PR #644 branch |
| Minimal (RAM, ROM, console) | ~80–85 MHz | 87.1 MHz | 2026-09-09, PR #644 branch |

The documented figures were first measured on 2026-09-07. They were re-measured on 2026-09-09 to confirm that PR #644 fixed #643's dynamic-dispatch regression. The skill's default sweep ranges fit this host. Before benchmarking refactor PRs here, re-take the baseline on current `main` (`afba63f` or later) and replace this row.

**Why:** the user asked for a baseline on the current hardware before the multi-CPU refactor, because the documented figures came from different hardware (the Linux workstation).
**How to apply:** run the `clock-speed-benchmark` skill on each refactor PR. Compare against this file's row for the current host; a consistent drop of more than about 5–10% is a regression. Don't edit the docs' baseline table without asking the user.
