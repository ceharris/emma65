---
name: tauri-new-window-capabilities
description: A new Tauri window needs both a tauri.conf.json declaration and a capabilities/*.json windows entry, or its event listeners silently receive nothing
metadata:
  type: feedback
---

A new debugger window (e.g. a `*-detached` panel window) must be registered in two places:
1. the `app.windows` array in `debugger/src-tauri/tauri.conf.json`
2. the `windows` array of `debugger/src-tauri/capabilities/default.json`, plus the checked-in mirror in `gen/schemas/capabilities.json`

**Why:** with only the first, the window shows, renders, and its `invoke()` commands work. But `listen()` from `@tauri-apps/api/event` is capability-gated, so no events ever arrive. On the LED matrix plan this cost several rounds of "fixes" to the emit code that were each correct but changed nothing.
**How to apply:** if a secondary window's `invoke()` calls work but it gets no events, check the capabilities file first.
