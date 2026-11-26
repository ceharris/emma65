---
name: clock-bench-baseline
description: Clock-speed benchmark baselines per workstation (Linux Ryzen 5 7530U and macOS i9, both re-taken 2026-11-26 before the multi-CPU refactor); compare refactors against the current host's numbers
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

**Linux workstation:** AMD Ryzen 5 7530U, 12 threads, release build. Measured 2026-11-26 at commit `37cf4a2` (main, after the emma6k rename and before the multi-CPU refactor). Conditions: Ubuntu 26.04, on AC power, `powersave` governor, `balanced` platform profile. Two runs:

| Device complement | Throttling accurate up to | Unthrottled top speed |
|---|---|---|
| Bundled default | ~35.5–36 MHz (≤0.03% error; falloff from 36–38) | 36.4–38.6 MHz (4 readings) |
| Minimal (RAM, ROM, console) | ~86–87 MHz (≤0.06% error; falloff from 87–88) | 92.3–92.5 MHz (2 runs) |

These are about 5–6% above the 2026-09-09 figures (~34 / 34.15 MHz and ~80–85 / 87.1 MHz, PR #644 branch, Ubuntu 24.04), which is within run-to-run noise or slightly better. The docs' ~34 / ~85 MHz figures still hold as conservative values. The skill's default sweep ranges fit this host. Expect single noisy points near the ceiling: in one run, 36 MHz read −4.64% while 37 MHz read −2.64%.

**Power mode barely matters on Linux.** The same day, with the platform profile and governor set to `performance`, two more runs gave a default complement accurate to ~36–38 MHz (top speed 35.9–39.1 MHz) and a minimal complement accurate to ~88 MHz (top speed 92.9–93.1 MHz). That's within about 2% of the `powersave`/`balanced` numbers, so compare runs across either mode. Record the mode anyway.

**Why:** the user asked for a baseline on the current hardware before the multi-CPU refactor, because the documented figures came from different hardware (the Linux workstation).
**How to apply:** run the `clock-speed-benchmark` skill on each refactor PR. Compare against this file's row for the current host; a consistent drop of more than about 5–10% is a regression. Don't edit the docs' baseline table without asking the user.
