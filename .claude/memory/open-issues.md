---
name: open-issues
description: Open GitHub issues with context worth knowing — the Memory Variables epic (#646), its follow-on stories (#656–659), and frontend test coverage (#559, #560)
metadata:
  type: project
---

Checked 2026-11-26; still open:

- **#646, Memory Variables Panel epic.** The MVP is fully merged (PRs #649, #650, #652, #654, #655), but the epic stays open. Ask before closing it. Its MVP design, `plan/memory-variables-panel-plan.md`, is frozen ([[plan-frozen-doc-current]]); plan the follow-ons separately.
- **#656–659, follow-on stories**, each filed with design questions for a planning agent:
  - #656 fixed-length arrays. Needs a schema change, and some local profiles already hold an older schema.
  - #657 structs with scalar-type fields.
  - #658 byte-sized bit-sets with named fields, generalizing `RegisterPanel.tsx`'s P-flag editing.
  - #659 variable-length arrays. Requires #656 first, and breaks the assumption that `ResolvedAddrCache` binds a fixed byte range.
  - #657 and #658 share an open question: define a type once and reuse it, or declare it inline per variable. Settle it the same way for both.
- **#559, #560: frontend test coverage** for `WatchpointPanel` and `SymbolsPanel`, follow-ups from #482. The xterm/canvas-heavy panels (Terminal, Display, LED Matrix, Memory, Assembler) were deliberately left out because jsdom can't render them. Tests for a hook throwing outside its provider should use `src/test/expectHookThrows.ts`, which keeps CI logs clean (#567).

**Why:** several older notes said to "ask before closing" issues that turned out to be closed already. This list holds only what was actually open on 2026-11-26.
**How to apply:** re-check an issue's state before acting on it. Don't close an epic without asking.
