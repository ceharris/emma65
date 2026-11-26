---
name: branch-from-origin-main
description: Cut new branches from a freshly fetched origin/main, not from a local main that has unpushed commits
metadata:
  type: feedback
---

Before starting a new branch, check whether local `main` has unpushed commits. If it does, push them first or run `git fetch && git checkout -b <branch> origin/main`.

**Why:** in PR #535 a stray unpushed local `main` commit was carried into the feature branch and silently folded into the PR's squash merge. Later `git pull --rebase` runs then tried to replay it and hit conflicts, including on binary files.
**How to apply:** if local `main` has diverged after such a merge, first check that the local-only commit's changes really are upstream. Only then reset local `main` to `origin/main`; don't assume it's redundant.
