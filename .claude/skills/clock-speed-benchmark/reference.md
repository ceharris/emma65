# Reference: clock-speed benchmark

## What's being measured

`emulator::exec::run()`'s free-running loop throttles emulated execution to a target clock
frequency by comparing accumulated emulated cycles against elapsed wall time, batching the
comparison over ~1,000 instructions to keep sleep-syscall overhead negligible (see
`src/emulator/exec/mod.rs`). Two distinct numbers come out of a benchmark run:

- **Accurate-throttling ceiling** — the highest target clock speed at which measured throughput
  still tracks the target to within a few hundredths of a percent. Below this, the throttle is
  doing real work (sleeping between batches); above it, the CPU can't execute fast enough to hit
  the target even unthrottled, so the error grows monotonically worse as the target climbs further
  out of reach.
- **Unthrottled ceiling** — raw throughput with `clock_speed_hz = None` (no throttling at all).
  This is the hard ceiling; the accurate-throttling ceiling sits at or just below it.

Every polled device costs per-instruction overhead (`IoDevice::tick()` is called for every device
on every instruction), so a smaller device complement raises both ceilings. The published wiki
figures (`doc/src/the-emulator-core.md`, "Clock Speed Simulation" section) bracket this with two
points: the bundled default complement (32K RAM, TaliForth ROM, VIA, two ACIAs, LFSR, console) and
a minimal complement (RAM, ROM, console only).

## Baseline figures

| device complement | accurate ceiling (as documented) | last re-validated |
|---|---|---|
| bundled default | ~34 MHz | 2026-09-09 (PR #644, RAM/ROM bus fast-path restore) |
| minimal (RAM, ROM, console) | ~85 MHz | 2026-09-09 (PR #644, RAM/ROM bus fast-path restore) |

Both were originally measured 2026-09-07 (wiki commit `3d6c3ad`, "docs: correct clock speed
simulation accuracy claim in wiki") on a mid-range 2023 laptop CPU (AMD Ryzen 5 7530U), release
build. Re-measured 2026-09-09 after PR #644 restored `Bus`'s special-cased RAM/ROM fast path
(reverting the `Box<dyn IoDevice>` dynamic dispatch #643 had introduced for all memory access,
which had regressed these numbers) — results matched the original figures within normal run
variance, confirming the fast path was restored.

Treat these as reference points to interpolate from, not universal constants — they depend on
host CPU, build profile (a debug build is roughly an order of magnitude slower and hits a much
lower ceiling; only ever benchmark `--release`), and machine load at the time. Two runs of the
same benchmark on the same machine can plausibly differ by 5-10%; a difference within that range
between an old baseline and a fresh run is noise, not a regression.

## Why these two example programs, and not a `cargo bench`/criterion setup

This measures wall-clock throttling accuracy over multi-second windows (the whole point is
verifying the sleep/wake batching holds a target rate over real time), which doesn't fit
criterion's iteration-count-based statistical model well. A tiny `#[tokio::main]` binary that
spins up a session, runs it for a fixed number of seconds, and compares cycles executed against
elapsed wall time is simpler and matches how the original 2026-09-07 measurement was done. These
programs are deliberately kept as throwaway `examples/` files copied in for the run and deleted
afterward (via this skill's `scripts/setup.sh`/`scripts/cleanup.sh`) rather than committed
permanently — they're a diagnostic tool for occasional revalidation, not something that needs to
build on every `cargo build --workspace` or be kept in sync with API changes between uses.

## Adjusting the target-speed brackets

`assets/clock_bench.rs` and `assets/clock_bench_minimal.rs` bracket their `targets_mhz` sweeps
around the baseline figures above. If a run shows the accurate ceiling clearly outside the
bracketed range (error is still ~0.00% at the top of the range, or already well into double-digit
error at the bottom), the actual ceiling has shifted enough that the results are inconclusive —
edit the copied `examples/clock_bench*.rs` file's `targets_mhz` list to bracket wider or shift the
range, rebuild (`cargo build --release --example clock_bench`), and rerun before drawing
conclusions. Don't extrapolate from a sweep that didn't actually observe the falloff.

## Updating the wiki

If a revalidation run lands meaningfully outside the baseline figures above on comparable
hardware (not just normal run-to-run noise, and not explained by a documented change to the
device-polling hot path), that's a real signal worth investigating before touching the docs —
check for hot-path regressions the way PR #644 fixed one for RAM/ROM. Only update
`doc/src/the-emulator-core.md`'s table and this file's baseline table if the user confirms the
new numbers should replace the documented ones; don't do it unilaterally from a single run.
