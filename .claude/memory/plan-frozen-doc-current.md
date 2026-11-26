---
name: plan-frozen-doc-current
description: plan/ docs are frozen history once superseded (at most a banner); doc/ (the mdBook wiki) must always describe the current design
metadata:
  type: feedback
---

- **`plan/`** holds specs, implementation plans and decision logs. Once a plan doc is superseded, leave its content as written, including outdated names. At most add a short "superseded by X" banner or cross-reference.
- **`doc/`** (the mdBook user wiki published to GitHub Pages) is live reference material. When the design changes, edit it in the same change. Never give it "superseded" banners.

**Why:** the user keeps decision history (`plan/`) separate from reference docs, which are only useful when accurate (`doc/`).
**How to apply:** the emma6k rename left historical plan docs untouched for this reason ([[emma6k-rename]]). For how to write wiki pages, see [[wiki-user-facing-writing]].
