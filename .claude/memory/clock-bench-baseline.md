---
name: clock-bench-baseline
description: Clock-speed benchmark baselines per workstation (only the macOS i9 one is measured so far); compare refactors against the current host's numbers
metadata:
  type: project
---

Benchmark baselines are specific to each machine, so compare only against numbers from the same host. The figures documented in `doc/src/the-emulator-core.md` (~34 / ~85 MHz) came from an AMD Ryzen 5 7530U. It isn't recorded whether that is the user's Linux workstation, so ask before assuming.

**macOS workstation:** Intel Core i9-9880H @ 2.30GHz, 16 threads, release build, measured 2026-11-26 at commit `afba63f` (main), before the multi-CPU refactor ([[multi-cpu-effort]]).

| Device complement | Throttling accurate up to | Unthrottled top speed |
|---|---|---|
| Bundled default | ~26 MHz (0.00% error through 26; −0.17%/−0.19% at 27–28) | 27.4–28.1 MHz (3 runs) |
| Minimal (RAM, ROM, console) | ~50–54 MHz (noisy; ≤0.3% error through 54) | 53.8–55.8 MHz (2 runs) |

On that Mac, the skill's default minimal sweep range (70–100 MHz) sits entirely above the top speed. Re-bracket the copied `examples/clock_bench_minimal.rs` to roughly 30–56 MHz. Results near the top speed are noisy there (laptop turbo/thermal behavior), so compare the accurate-throttling numbers across at least two runs.

**Linux workstation:** no baseline recorded yet. Take one on `main` before benchmarking refactor PRs there, and add it here.

**Why:** the user asked for a baseline on the current hardware before the multi-CPU refactor, because the documented figures came from different hardware.
**How to apply:** run the `clock-speed-benchmark` skill on each refactor PR. Compare against this file's row for the current host; a consistent drop of more than about 5–10% is a regression. Don't edit the docs' baseline table without asking the user.
