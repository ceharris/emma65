# Changelog

All notable changes to `emma65` (the 6502 emulator library) are documented
here, together with `emma6k-core`, the CPU-neutral core crate (`core/`) that
`emma65` builds on, and the crates versioned with them: `emma09` and the
`emma6k` command line (`emma6k`, `emma65`, `emma09` and `emma6k-tracer`). The two are versioned
in lockstep and released together; `emma6k-core` has no changelog of its own.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
— see `.claude/skills/release/reference.md` for this crate's specific
breaking/additive/fix-only definitions and pre-1.0 policy.

## [Unreleased]

### Added

- The `exit` device: a program writes `EXIT` and a status byte to it to stop
  the CPU, and `emma6k` (or `emma65`/`emma09`) exits with that status. It works
  on either CPU. CamelForth's new `BYE` word uses it. Like the other devices,
  it logs a message when it's reset.

### Removed

- `emma65-tracer`. Its replacement, `emma6k-tracer`, decodes 6502 and 6809
  traces and reads older trace files too. The packages install `emma6k-tracer`
  instead.

### Changed

- `IoDevice::take_reset() -> bool` is now `take_request() -> Option<CpuRequest>`
  (a reset or an exit), and the bus asks for it right after each write to the
  device rather than every step. Devices can also declare themselves passive
  (`is_passive`), which leaves them out of the per-step tick and poll.
- Trace files are format version 3, which records the CPU family. The 6502's
  records are unchanged, and version 2 files still read.

### Fixed

- A run with `--log-file` no longer hangs after the CPU stops (STP, or the
  `exit` device) or on Ctrl+C: the emulator now exits once the log is written.

## [0.1.0] - 2026-09-07

Initial release.
