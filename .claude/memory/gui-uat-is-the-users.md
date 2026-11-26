---
name: gui-uat-is-the-users
description: Never launch or drive the debugger/SDL GUIs on your own initiative; verify by build/typecheck/test/lint and leave visual UAT to the user
metadata:
  type: feedback
---

Don't start `npx vite`, `cargo tauri dev` or the SDL peripheral binaries, and don't drive them with xdotool or take screenshots, unless the user's *current* message asks for it. That includes launching the app "so the user can UAT". An authorization given for one bug doesn't carry over to the next.

For a debugger UI change, finish with: build, `tsc --noEmit`, tests, lint, a statement of what passed, then stop. List the visual checks in the PR's UAT section.

**Why:** the user has said this several times (2026-08-16, 2026-08-17, 2026-08-21): "Please don't do manual UAT unless I explicitly direct you to do so -- automatic UAT is much slower and more costly than doing it myself", and "It's not helpful for you to start the UI for me." Having xdotool and imagemagick installed on the Linux workstation doesn't change this.
**How to apply:** when GUI driving *is* requested, see [[debugger-gui-automation-quirks]] and [[escalate-unconverging-gui-repro]].
