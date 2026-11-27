# Release reference

Detailed lookup material for the `release` skill. `SKILL.md` links here rather than inlining
this — load it when you actually need a path or a classification call.

## Artifact table

Only `emma65`'s tag triggers a GitHub Actions build (`.github/workflows/release.yml`'s `on: push:
tags:` pattern is `emma65-v[0-9]+.[0-9]+.[0-9]+` only). The other four crates' tags are
bookkeeping-only per step 11 of `SKILL.md` — they never dispatch a build; their current state at
whatever commit is on `main` gets packaged into the next `emma65-v*` combined release.

| Crate | Directory | `Cargo.toml` path(s) | Changelog | Tag prefix | Triggers build |
|---|---|---|---|---|---|
| `emma65` (with `emma6k-core` and `emma09`) | repo root, plus `core/` and `emma09/` | `Cargo.toml` (`version`, and the `emma6k-core` dependency's `version`), `core/Cargo.toml`, `emma09/Cargo.toml` (`version`, and its `emma6k-core` dependency's `version`) — all five must match | `CHANGELOG.md` | `emma65-v` | **Yes** — combined `.deb`/`.rpm` |
| `emma6k-display` | `display/` | `display/Cargo.toml` | `display/CHANGELOG.md` | `emma6k-display-v` | No |
| `emma6k-led-matrix` | `led-matrix/` | `led-matrix/Cargo.toml` | `led-matrix/CHANGELOG.md` | `emma6k-led-matrix-v` | No |
| `emma6k-lcd-display` | `lcd-display/` | `lcd-display/Cargo.toml` | `lcd-display/CHANGELOG.md` | `emma6k-lcd-display-v` | No |
| `emma6k-debugger` | `debugger/src-tauri/` | `debugger/src-tauri/Cargo.toml`, `debugger/src-tauri/tauri.conf.json` (`.version`), `debugger/frontend/package.json` (`.version`) — all three must match | `debugger/src-tauri/CHANGELOG.md` | `emma6k-debugger-v` | No |

`emma6k-core` and `emma09` have no rows of their own: they are versioned in lockstep with `emma65`
and released only as part of an `emma65` release (see `SKILL.md`).

The four `emma6k-*` crates were named `emma65-*` through 0.1.0, so their tags up to and including
0.1.0 use the legacy prefixes `emma65-display-v`, `emma65-led-matrix-v`, `emma65-lcd-display-v`,
and `emma65-debugger-v`. New tags use the `emma6k-` prefixes in the table above. When looking for
a crate's last release, check both (see `SKILL.md` step 2).

## Release artifacts

An `emma65-v*` release publishes exactly two assets, both built from
`[package.metadata.deb]`/`[package.metadata.generate-rpm]` in the root `Cargo.toml` (package name
`emma6k` for both): one combined `.deb` and one combined `.rpm`, each containing every workspace
binary — `emma65`, `emma65-tracer`, `emma6k-display`, `emma6k-led-matrix`, `emma6k-lcd-display`,
`emma6k-debugger` — plus the debugger's desktop entry and icons (`packaging/emma6k-debugger.desktop`,
`debugger/src-tauri/icons/`). Runtime library dependencies (SDL2, SDL2_gfx, webkit2gtk, gtk3) are
auto-detected from the packaged binaries (`depends = "$auto"` / `auto-req = "auto"`), not
hand-listed — there's nothing to update here when a dependency changes.

## Tag and branch naming

- Release tag: `<crate>-vX.Y.Z` (e.g. `emma6k-lcd-display-v0.2.0`), annotated, pointing at the
  merge commit on `main` that carried the version bump.
- Release branch: `release/<crate>-vX.Y.Z`.
- Commit message for the bump: `release: <crate> X.Y.Z`.

## Semver policy

All five artifacts are currently `0.y.z`. **Pre-1.0 policy**: while an artifact is at `0.y.z`, a
change that would be MAJOR under normal SemVer instead bumps MINOR (`0.y.z → 0.(y+1).0`) rather
than MAJOR, until the maintainer deliberately decides to cut `1.0.0` for that specific artifact.
A fix-only change still bumps PATCH (`0.y.z → 0.y.(z+1)`). Always name which class (breaking /
additive / fix-only) a change belongs to and say so explicitly in the PR body — the numeric bump
alone hides that distinction pre-1.0, and the human reviewer needs to see the reasoning to
override it if they disagree.

| Artifact | Breaking | Additive | Fix-only |
|---|---|---|---|
| `emma65` (and `emma6k-core`) | Remove/rename a public `emulator`/`watch` API item, or a public `emma6k-core` item (the `emulator` module re-exports it); change a CLI flag's name or required-ness; change/remove a TOML config key or its accepted value shape; change the binary trace format so older `emma65-tracer` builds can't decode newer trace files | New public API item with no existing signature touched; new optional CLI flag; new optional TOML config key with a backward-compatible default; new trace record kind that older readers can skip | Bug fixes, perf improvements, doc-comment fixes with no API/CLI/config/trace-format change |
| `emma6k-display` | Remove/rename a CLI flag; incompatible change to the wire protocol in `plan/char-display-external-protocol.md` (framing, field meaning/order) | New optional CLI flag; backward-compatible protocol extension (new message type an older peer can ignore) | Rendering/crash bug fixes with no CLI/protocol change |
| `emma6k-led-matrix` | Remove/rename a CLI flag; incompatible change to `plan/led-matrix-external-protocol.md` | New optional CLI flag; backward-compatible protocol extension | Rendering/timing bug fixes with no CLI/protocol change |
| `emma6k-lcd-display` | Remove/rename a CLI flag; incompatible change to `plan/lcd-display-external-protocol.md` (including font/geometry negotiation semantics) | New optional CLI flag; backward-compatible protocol extension | Rendering bug fixes with no CLI/protocol change |
| `emma6k-debugger` | Change to the profile/session file format that breaks loading files saved by a prior version; change to the keybindings config schema/keys that breaks existing user configs; removing/renaming a user-visible exported-file behavior | New optional field in the profile/session or keybindings format with a safe default; new panel/feature with no change to existing file formats | UI/crash/layout bug fixes with no format/schema change |

`cargo semver-checks -p emma65` (see SKILL.md step 5) only applies to the `emma65` crate — it's
the only workspace member with a real, consumed `lib` target. The four other crates are bin-only,
and `emma6k-debugger`'s `emma6k_debugger_lib` is an internal Tauri staticlib/cdylib, not a
published API surface — semver-checks has nothing meaningful to check there.

## Changelog format

Each `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/):

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Removed
- ...
```

Omit empty subsections. New entries are prepended directly below the file's header (most recent
first). A first release for an artifact gets a single entry with no subsections:

```markdown
## [0.1.0] - YYYY-MM-DD

Initial release.
```
