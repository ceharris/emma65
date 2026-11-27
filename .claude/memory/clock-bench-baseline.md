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

**After multi-CPU step 1 (Linux, rustc 1.100.0, `balanced`/`powersave`, 2026-11-26).** The refactor made the 6502 faster. With the `emma6k-core` crate (PR 4) and its `#[inline]` hints, the minimal complement stays accurate to ≥100 MHz with a top speed of ~106 MHz, and the default complement to ≥40 MHz with a top speed of ~41 MHz (two A/B runs each). Use these, not the pre-refactor rows above, as the Linux baseline for step 2.

**Re-bracket after step 1 (Linux).** The skill's stock brackets top out at 40 and 100 MHz, below the post-step-1 ceilings, so they never show the falloff. For A/B runs, sed the copied `examples/clock_bench_minimal.rs` to `[34, 36, 38, 39, 40, 41, 42, 44]` (default) and `[90, 95, 100, 102, 104, 106, 108, 110]` (minimal). Build one binary per ref into the scratchpad, then alternate them; each run takes ~55 s. On 2026-11-27, same-day `main` read 40.1–41.0 MHz (default) and 98.9–102 MHz (minimal) for top speed, a bit under the 2026-11-26 figures. Compare against same-day `main`, not just this file.

**Code layout swings top speed by ±4% (Linux, 2026-11-27).** The same source change measured 4.5% slower in the clock-bench binary and 2% faster in a different probe binary; an unused 8-byte struct field alone moved speed ~2%. So a clock-bench drop under ~5% can be alignment luck. To tell layout from real work, build a small probe example (default complement, `clock_speed_hz = None`, fixed seconds, print `cpu.cycles()`) per ref, then compare `perf stat -e instructions:u,cycles:u,branch-misses:u,op_cache_hit_miss.op_cache_miss:u` normalized per emulated cycle. Same instructions per emulated cycle (~321 on the default complement) with different op-cache misses means layout. `perf` needs `kernel.perf_event_paranoid` ≤ 1; it's 4 by default on this host and resets on reboot, so give the user a script that runs `sudo sysctl -w kernel.perf_event_paranoid=1` ([[sudo-via-script]]).

**6809 baseline (Linux, Unit 2 of step 2, `balanced`/`powersave`, 2026-11-27).** A throwaway example (not committed) built RAM `$0000–$BFFF`, an 8K ROM at `$E000`, and a console at `$C000` through `build_machine`, running a loop of an indexed 128-byte copy (`LDA ,X+`/`ADDA #`/`STA ,U+`/`DECB`/`BNE`), 16-bit `LDD`/`ADDD`/`STD`, `BSR` with `PSHS`/`PULS PC`, and a console-latch poll (~2,500 cycles per pass). Top speed 126.3–127.4 MHz (4 readings); throttling exact to 100 MHz, mostly accurate to 122 MHz with occasional 1–2.6% dips, falling off from 124 MHz. It's a new baseline, not comparable with the 6502's TaliForth-idle workload. Once Unit 5's CamelForth template exists, re-measure with CamelForth idle for a like-for-like comparison.

**Power mode barely matters on Linux.** The same day, with the platform profile and governor set to `performance`, two more runs gave a default complement accurate to ~36–38 MHz (top speed 35.9–39.1 MHz) and a minimal complement accurate to ~88 MHz (top speed 92.9–93.1 MHz). That's within about 2% of the `powersave`/`balanced` numbers, so compare runs across either mode. Record the mode anyway.

**Why:** the user asked for a baseline on the current hardware before the multi-CPU refactor, because the documented figures came from different hardware (the Linux workstation).
**How to apply:** run the `clock-speed-benchmark` skill on each refactor PR. Compare against this file's row for the current host; a consistent drop of more than about 5–10% is a regression. Don't edit the docs' baseline table without asking the user.
