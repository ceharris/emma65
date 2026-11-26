---
name: wiki-user-facing-writing
description: The doc/src/ wiki is for users; no Rust internals, source paths or issue numbers outside For Contributors, and check facts against source, not description strings
metadata:
  type: feedback
---

Every `doc/src/` page except `for-contributors.md` is written for people who run and configure the emulator, debugger and peripherals:
- Describe behavior, TOML/CLI attributes and observable effects.
- Don't use Rust type, trait or method names (`Via6522`, `Cpu::step()`), source-file paths, or GitHub issue numbers.
- The appendix pages (wire protocols, trace format) may give byte-level detail, but still no internals.
- `for-contributors.md` is the one page that describes Rust architecture.

Check facts against the source, not against summary strings. A bundled template's one-line description once left out a device and got another's role wrong. Read the template's `emulator-template.toml` instead.

**Why:** the user asked for a full pass on 2026-09-07 to remove narrative "pertinent/accessible only to a developer or an agent."
**How to apply:** before finishing a page, grep it for `` `[A-Z][a-zA-Z]*::` ``, `src/.*\.rs` and `issue #[0-9]+`. Related: [[plan-frozen-doc-current]].
