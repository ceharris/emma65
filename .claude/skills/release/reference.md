# Release reference

Detailed lookup material for the `release` skill. `SKILL.md` links here rather than inlining
this — load it when you actually need a path or a classification call.

## Artifact table

| Crate | Directory | `Cargo.toml` path(s) | Changelog | Tag prefix |
|---|---|---|---|---|
| `emma65` | repo root | `Cargo.toml` | `CHANGELOG.md` | `emma65-v` |
| `emma65-display` | `display/` | `display/Cargo.toml` | `display/CHANGELOG.md` | `emma65-display-v` |
| `emma65-led-matrix` | `led-matrix/` | `led-matrix/Cargo.toml` | `led-matrix/CHANGELOG.md` | `emma65-led-matrix-v` |
| `emma65-lcd-display` | `lcd-display/` | `lcd-display/Cargo.toml` | `lcd-display/CHANGELOG.md` | `emma65-lcd-display-v` |
| `emma65-debugger` | `debugger/src-tauri/` | `debugger/src-tauri/Cargo.toml`, `debugger/src-tauri/tauri.conf.json` (`.version`), `debugger/frontend/package.json` (`.version`) — all three must match | `debugger/src-tauri/CHANGELOG.md` | `emma65-debugger-v` |

## Tag and branch naming

- Release tag: `<crate>-vX.Y.Z` (e.g. `emma65-lcd-display-v0.2.0`), annotated, pointing at the
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
| `emma65` | Remove/rename a public `emulator`/`watch` API item; change a CLI flag's name or required-ness; change/remove a TOML config key or its accepted value shape; change the binary trace format so older `emma65-tracer` builds can't decode newer trace files | New public API item with no existing signature touched; new optional CLI flag; new optional TOML config key with a backward-compatible default; new trace record kind that older readers can skip | Bug fixes, perf improvements, doc-comment fixes with no API/CLI/config/trace-format change |
| `emma65-display` | Remove/rename a CLI flag; incompatible change to the wire protocol in `plan/char-display-external-protocol.md` (framing, field meaning/order) | New optional CLI flag; backward-compatible protocol extension (new message type an older peer can ignore) | Rendering/crash bug fixes with no CLI/protocol change |
| `emma65-led-matrix` | Remove/rename a CLI flag; incompatible change to `plan/led-matrix-external-protocol.md` | New optional CLI flag; backward-compatible protocol extension | Rendering/timing bug fixes with no CLI/protocol change |
| `emma65-lcd-display` | Remove/rename a CLI flag; incompatible change to `plan/lcd-display-external-protocol.md` (including font/geometry negotiation semantics) | New optional CLI flag; backward-compatible protocol extension | Rendering bug fixes with no CLI/protocol change |
| `emma65-debugger` | Change to the profile/session file format that breaks loading files saved by a prior version; change to the keybindings config schema/keys that breaks existing user configs; removing/renaming a user-visible exported-file behavior | New optional field in the profile/session or keybindings format with a safe default; new panel/feature with no change to existing file formats | UI/crash/layout bug fixes with no format/schema change |

`cargo semver-checks -p emma65` (see SKILL.md step 5) only applies to the `emma65` crate — it's
the only workspace member with a real, consumed `lib` target. The four other crates are bin-only,
and `emma65-debugger`'s `emma65_debugger_lib` is an internal Tauri staticlib/cdylib, not a
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
