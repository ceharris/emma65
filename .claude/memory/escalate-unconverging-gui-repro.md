---
name: escalate-unconverging-gui-repro
description: After 2–3 live GUI repro rounds that add no new information, stop; do a fresh end-to-end code read or bring in a stronger-model reviewer
metadata:
  type: feedback
---

Don't keep repeating the same live experiment (launch, wait, screenshot, kill). Once 2–3 rounds have told you nothing beyond what the user already reported, either:
- read the implicated code path end to end from scratch, or
- suggest a fresh non-fork agent on a stronger model as an independent reviewer, giving it a self-contained write-up of the symptom and everything already ruled out.

**Why:** on the LED matrix SDL rendering bug (PR #529), the screenshot rounds were flaky and only re-confirmed what the user had said first. The user: "That was a lot of time and expense for you to discover the same thing I told you." An Opus advisor then found the root cause, [[sdl2-gfx-color-byte-order]], in one pass.
**How to apply:** this applies whenever GUI driving has been authorized ([[gui-uat-is-the-users]]).
