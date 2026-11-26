---
name: debugger-hand-styled-controls
description: Debugger frontend form controls must be hand-styled; native popup surfaces (select lists, datalist) don't follow the theme and can hang the WebKitGTK app
metadata:
  type: feedback
---

In the debugger frontend (Tauri + WebKitGTK on Linux), don't use native form controls' default chrome. Reuse or follow the existing hand-styled components:
- `SelectPopover` for dropdowns.
- `ColorPickerPopover` for colors.
- `NumberStepper`, which keeps the native number input but hides its spinner and draws its own +/- buttons.
- `modal.scss`'s `.modal-checkbox-row` and `.modal-template-row` for checkboxes and radio buttons.
- `NameAutocomplete` in `MemoryVariablesPanel.tsx` for autocomplete.

`appearance: none` is enough only when the control has no separate popup surface.

**Why:** native popups ignore the light/dark theme. Worse, a `<datalist>` autocomplete (PR #654) **froze the whole app** when opened, almost certainly through a stuck GTK pointer grab, and only a force-quit recovered it.
**How to apply:** treat any control with a native popup surface as a possible app hang. Check it by actually opening it, not just by reading the diff; UAT is the user's ([[gui-uat-is-the-users]]).
