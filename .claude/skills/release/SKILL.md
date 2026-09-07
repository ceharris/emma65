---
name: release
description: Cut a release for one of emma65's five independently-versioned crates (emma65, emma65-display, emma65-led-matrix, emma65-lcd-display, emma65-debugger) — classify the semver bump, update version files, write the changelog entry, open a PR, and after the user confirms it's merged, tag and push. Use when the user asks to cut/prepare/start a release, bump a crate's version, or says things like "release emma65-lcd-display 0.2.0" or "publish a new debugger build". Only tagging `emma65` itself triggers a GitHub Actions build — see step 11.
---

# Release

Cuts a release for one artifact in the `emma65` workspace. This skill handles only the
human-judgment half of a release — classifying the version bump, editing files, and opening a PR
for review. It never builds or publishes anything itself.

Each of the five crates keeps its own independent version and `CHANGELOG.md` for bookkeeping, but
only `emma65` — the root/umbrella crate — has a real GitHub Actions build behind its tag. Pushing
an `emma65-vX.Y.Z` tag and dispatching `.github/workflows/release.yml` builds **every** workspace
binary (the emulator CLI, the three SDL2 peripheral binaries, and the Tauri debugger GUI) into one
combined `emma65` `.deb` and `.rpm`, published as a single GitHub Release. Tagging one of the
other four crates (step 11) is tag-and-push only — it records that crate's version/changelog for
history, but does not trigger a build; that crate's current state simply gets folded into
whichever `emma65-v*` release comes next.

Detailed lookup tables (artifact → paths, the full semver policy, changelog/tag format specs)
live in `reference.md` — load it when you reach a step that needs it, not up front.

## Runbook

**0. Preflight.** Run `gh auth status`. If it fails, stop and tell the user before touching any
files — don't discover this partway through with version files already edited.

**1. Identify the target artifact.** One of: `emma65`, `emma65-display`, `emma65-led-matrix`,
`emma65-lcd-display`, `emma65-debugger`. Ask if the user didn't name one explicitly. Look up its
directory, `Cargo.toml` path(s), and changelog path in `reference.md`'s artifact table.

**2. Find the last release tag** for this artifact:

```bash
git tag -l '<tag-prefix>*' --sort=-v:refname | head -1
```

(tag prefix from `reference.md`, e.g. `emma65-lcd-display-v`)

- **If none exists** — true for all five artifacts today — this is that artifact's first
  release. Skip straight to step 6 with the version already in `Cargo.toml` (currently `0.1.0`
  for all of them) and a changelog entry that just says "Initial release." Don't invent a bump
  rationale for work that predates any release process.
- Otherwise continue to step 3.

**3. Gather the raw change list** since that tag:

```bash
.claude/skills/release/scripts/gather-changes.sh <last-tag> <artifact-path>...
```

For the four crates with a single directory, pass that directory (e.g. `display`). For `emma65`
itself, pass `src`, `Cargo.toml`, and `Cargo.lock` (the paths that belong to the core crate, not
the peripheral directories). This output is raw material for drafting the changelog by hand in
step 7 — do not try to auto-generate changelog prose from it; this repo's commit messages aren't
consistently structured enough to categorize reliably on their own.

**4. Classify the bump.** Compare the gathered changes against `reference.md`'s semver policy
table for this artifact: breaking / additive / fix-only. Apply the pre-1.0 policy stated there
(while `0.y.z`, a breaking change bumps MINOR, not MAJOR). State your reasoning plainly — it goes
into the PR body in step 8 so the human reviewer can see and override it.

**5. Corroborate with `cargo semver-checks`, only for `emma65` itself:**

```bash
cargo semver-checks check-release -p emma65 --baseline-rev <last-tag>
```

Not applicable to the other four crates (bin-only, or an internal Tauri lib) — skip this step
for them entirely. If the tool isn't installed, tell the user
(`cargo install cargo-semver-checks --locked`) and proceed on manual judgment if they decline. If
its verdict disagrees with your step-4 classification, surface that to the user rather than
silently picking one.

**6. Update version field(s).**

- Single-file crates (`emma65`, `emma65-display`, `emma65-led-matrix`, `emma65-lcd-display`):
  edit the `version = "X.Y.Z"` line directly in that crate's `Cargo.toml`.
- `emma65-debugger`:
  ```bash
  .claude/skills/release/scripts/bump-debugger-version.sh <new-version>
  ```
  This updates all three of `debugger/src-tauri/Cargo.toml`,
  `debugger/src-tauri/tauri.conf.json`, and `debugger/frontend/package.json` and verifies they
  agree before exiting 0 — it fails loudly, naming the file, if any edit didn't take.
- Then run `cargo build -p <crate>` (or `cargo check --workspace` for the debugger) so
  `Cargo.lock` picks up the bump. Commit the updated lockfile alongside.

**7. Write the changelog entry.** Prepend a new section to the artifact's `CHANGELOG.md`
(directly below the file header) following the format in `reference.md` — `## [X.Y.Z] -
YYYY-MM-DD` with `### Added`/`### Changed`/`### Fixed`/`### Removed` subsections as applicable
(omit empty ones), drafted from step 3's raw change list. First release for an artifact gets a
single unstructured "Initial release." line instead.

**8. Open the PR.**

```bash
git checkout -b release/<crate>-vX.Y.Z
git add <version files> Cargo.lock <changelog>
git commit -m "release: <crate> X.Y.Z"
git push -u origin release/<crate>-vX.Y.Z
gh pr create --title "release: <crate> X.Y.Z" --body "..."
```

PR body must include: the artifact, old → new version, the bump classification and reasoning
from step 4, and the changelog entry text. **Do not merge it** — this repo's standard workflow is
branch → PR → human review → merge, with no exception for release PRs.

**9. Stop and wait.** Tell the user the PR is open and that you will not proceed until they
confirm it has been merged into `main`. Do not poll for merge status.

**10. Confirm before tagging.** Once the user confirms the merge, `git fetch origin` and locate
the merge commit on `main`. Before doing anything else, explicitly ask the user to confirm —
state the exact tag name and the commit SHA it will point to. For `emma65`, this is the one
irreversible, public-facing step in the whole process: dispatching the release build (next step)
publishes a public GitHub Release. For the other four crates it's lower-stakes (tag-and-push only,
no build), but still confirm before pushing a public tag.

**11. Tag and push**, only after that confirmation:

```bash
git tag -a <crate>-vX.Y.Z <merge-sha> -m "<crate> X.Y.Z"
git push origin <crate>-vX.Y.Z
```

If `<crate>` is **not** `emma65`, stop here — tell the user the tag is pushed for bookkeeping and
that this change will ship in whichever `emma65-v*` release comes next; do not dispatch a build.

If `<crate>` **is** `emma65`, dispatch the combined build:

```bash
gh workflow run release.yml --ref main -f tag=emma65-vX.Y.Z
```

The tag always points at a commit that was already pushed to `main` (merging the version-bump PR
in step 8), and GitHub Actions dedupes check-suites by commit SHA — so the tag push by itself
never fires `release.yml`'s `on: push: tags:` trigger, even though the tag exists on GitHub and
matches the trigger's glob pattern. The `gh workflow run` dispatch is what actually starts the
build: it reads the workflow definition from `--ref main`, but pins the build job's checkout to
the tag itself, so the build is still produced from the tagged commit — including whatever the
*other* four crates' `Cargo.toml` versions currently are at that commit, since the combined
package always reflects current `main` state for every binary — not from whatever `main`'s tip
happens to be at dispatch time.

Report the push and dispatch, then poll `gh run list --workflow=release.yml --limit 1` (it can
take a few seconds for the dispatched run to appear) and give the user the run URL. The skill's
job ends here — it does not watch the run through to completion.
