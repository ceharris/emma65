---
name: multi-unit-plan-workflow
description: For multi-unit plan docs, one branch + PR per unit, stop after each PR, and check up front that each unit compiles on its own
metadata:
  type: feedback
---

When working from a multi-unit plan in `plan/`, treat each work unit as its own [[git-workflow]] cycle:

1. Create a branch named for that unit, not for the whole issue.
2. Implement it and run all validation, plus the unit's UI verification list from the plan.
3. Commit, push, and open the PR right away, listing any manual UAT the reviewer still needs to do.
4. **Stop and wait for explicit instruction.** Don't merge, and don't start the next unit, even if CI is green.

A fresh session per unit is normal. That session picks up from this memory and the plan's section for that unit.

Before starting a unit, check its file list against the neighbouring units for compile dependencies in either direction. An example is one unit deleting a type that a later unit's file still imports. A plan's unit split isn't guaranteed to compile at each step. If you find a dependency, ask whether to combine the units, add a stub, or reorder them; don't decide silently.

**Why:** established for issue #462 and reused for #467, #482, #489, #554, #646 and others, so the user can review and merge each unit on its own. The compile check came from the display/keyboard integration plan. Its Unit 2 deleted a type that Unit 3's file imported, so Unit 2 alone broke the build. The user chose to combine the two units.
**How to apply:** this applies to every multi-unit plan, including the multi-CPU step 1 plan ([[multi-cpu-effort]]).
