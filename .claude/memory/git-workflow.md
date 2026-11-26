---
name: git-workflow
description: Standing branch → commit → PR (with manual UAT steps and Closes #n) → wait-for-human-review workflow for every code change
metadata:
  type: feedback
---

For every code change, without being asked each time:

1. Create a branch before starting work (see [[branch-from-origin-main]] and [[no-worktrees]]).
2. Do the work and verify it: build, tests, every CI clippy command ([[ci-clippy-deny-warnings]]), and `cargo fmt --all`.
3. Commit to the branch.
4. Open a PR with a manual UAT section: concrete steps a human can follow in the running app, not just "tests pass". If the PR fixes an issue, put `Closes #<n>` in the description.
5. Stop and wait for human UAT and review. Don't merge, and don't treat the task as done.

Plan/spec docs (`plan/*.md`) and some docs-only changes have been committed straight to `main`, but only when the user explicitly asked for that. Otherwise ask before committing anything.

Agent memory changes (`.claude/memory/`) are an exception. Commit them with the PR for the work they describe. When no PR applies, commit them directly to `main` without asking (user, 2026-11-26). Push that commit right away, so it can't be swept into a later branch's squash merge ([[branch-from-origin-main]]).

When the user asks to "prepare memory for the next session", write precise memory notes only. Don't start implementing or editing code in that session, even if a bug turns up while writing them. Record the finding (root cause, fix, regression tests to add) in memory instead.

**Why:** the user called this their "agreed upon workflow" (2026-08-16). They added `Closes #n` on 2026-09-02, after an issue had to be closed by hand. The "prepare memory" rule came from a 2026-09-02 session that started editing `main` mid-wrap-up and was interrupted.
**How to apply:** default to this cycle for any fix or feature. For multi-unit plans, see [[multi-unit-plan-workflow]].
