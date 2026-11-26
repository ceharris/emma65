---
name: avoid-side-effect-caveats
description: If a design needs a documented side-effect caveat to be understood, propose removing the capability instead of documenting the caveat
metadata:
  type: feedback
---

If a register or API design needs an "important consequence" caveat to explain surprising behavior, raise that tradeoff while planning. Suggest dropping the optional capability rather than documenting the caveat.

**Why:** the display palette-write design let a control write disarm an in-progress palette update, so any unrelated control write also aborted the update. The user: "The ability to disarm ... creates more complication than it warrants. Let's remove the ability to disarm it."
**How to apply:** this applies especially when nothing in the requirements calls for the capability.
