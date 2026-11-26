---
name: diagnose-gui-bugs-with-evidence
description: For GUI/async bugs you can't reproduce, gather evidence (labeled diagnostic logging, precise user description) before fixing; don't iterate on theories that merely fit the symptoms
metadata:
  type: feedback
---

When you can't reproduce a GUI bug directly and a plausible fix hasn't worked, don't propose a second fix based on a theory. Instead:

- Add narrow, clearly labeled diagnostic logging (easy to strip out later) at the points that tell the live hypotheses apart.
- Ask the user to reproduce the bug with it, and derive the fix from the actual output. `AskUserQuestion` helps narrow which hypothesis to instrument for.
- When pixel-level analysis of the user's screenshots finds nothing, ask them to describe the defect more precisely (shape? color? timing?) rather than guess.
- Treat a timing or symptom the user has already reported as the strongest evidence, not something to re-confirm.

**Why:** on #474's double file-chooser bug, a re-entrancy guard built on an unverified theory changed nothing. The user said: "I think you're trying to address symptoms without determining the underlying pathology." Logging then showed the event was being lost, not duplicated. On #593, two wrong theories (anti-aliasing, letterbox scaling) added a resize mechanism that later had to be reverted. The real defect, rounded dots pinching at the seams between adjacent dots, only surfaced once the user described the dots' shape. A failure to launch SDL windows around the same time was a host windowing problem that a reboot fixed, unrelated to the code.
**How to apply:** see also [[escalate-unconverging-gui-repro]].
