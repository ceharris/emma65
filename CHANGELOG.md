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

### Removed

- `emma65-tracer`. Its replacement, `emma6k-tracer`, decodes 6502 and 6809
  traces and reads older trace files too. The packages install `emma6k-tracer`
  instead.

### Changed

- Trace files are format version 3, which records the CPU family. The 6502's
  records are unchanged, and version 2 files still read.

## [0.1.0] - 2026-09-07

Initial release.
