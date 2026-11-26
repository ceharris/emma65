---
name: no-worktrees
description: Use plain `git checkout -b` branches in the main checkout, never `git worktree add`
metadata:
  type: feedback
---

Create work branches in the primary checkout with `git checkout -b`. Never use `git worktree add`, even for a single in-flight unit.

**Why:** the user runs dev and test commands (e.g. `cargo tauri dev`) from the one directory they always use. A worktree at another path meant their UAT silently ran against `main`, not the branch under review. A whole diagnostic-logging cycle (2026-08-23, PR #505) was spent chasing a bug that didn't exist. The user had already rejected worktrees for this project.
**How to apply:** if a worktree turns up, move its changes onto a plain branch in the main checkout as soon as possible.
