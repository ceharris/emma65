---
name: headless-emulator-runs
description: To run the emulator CLI headless (checks, UAT, benchmarks), use a small WDC65C02 ROM ending in STP so it exits by itself; never trace an unthrottled, unbounded run
metadata:
  type: feedback
---

When running `emma6k`/`emma65` from a shell or test to check something (a trace, an exit code, a timing), give it a tiny WDC 65C02 ROM that does its work and then executes `STP` ($DB): the run ends on its own and the process exits 0. Build it with a few lines of Python (32K of $DB, the program at offset 0, the reset vector at $7FFC/$7FFD → $8000) and run with `--cpu-variant WDC65C02 --device ram@0x0000,size=32768 rom@0x8000,size=32768,image=<rom>`. A loop count in the ROM makes a fixed-length workload for end-to-end timing (`perf stat` on the binary).

**Why:** on 2026-11-27 I traced the default TaliForth profile with `--trace-file` and stopped it with `timeout -s INT`. Unthrottled, the trace filled the temp volume's quota in about a second, the run hung waiting for the trace writer, and the session's tools failed until the file was deleted. The user suggested the STP approach. (Also, `pkill -f <pattern>` matched and killed my own shell, because the pattern was in its command line; use `pkill -x <name>`.)

**How to apply:** prefer an STP ROM over signals, `timeout`, or killing the process. If a long-running profile is unavoidable, throttle it with `--clock-speed-hz` and never combine it with `--trace-file`. Keep stdin open (a pipe), not `/dev/null`, when checking console output: the console's stdio transport stops output when stdin hits end of file. Relates to [[gui-uat-is-the-users]] (the GUI is still the user's to drive).
