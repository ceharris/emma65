---
name: github-body-no-hard-wrap
description: Write GitHub issue/PR bodies and comments with one unwrapped line per paragraph; GitHub renders single newlines as hard breaks
metadata:
  type: feedback
---

In GitHub issue bodies, PR descriptions and comments, write each prose paragraph as one long line. List items and headings still get their own lines.

**Why:** GitHub renders a single newline inside a paragraph as a line break, so text wrapped at 80 columns comes out choppy. The user flagged this on issue #490's body.
**How to apply:** this covers any `gh ... --body`/`--body-file` text. Files checked into the repo (`plan/*.md`, `CLAUDE.md`, memory) keep their normal wrapping.
