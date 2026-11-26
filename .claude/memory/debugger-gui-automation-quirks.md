---
name: debugger-gui-automation-quirks
description: How to launch `cargo tauri dev` despite the broken beforeDevCommand, and how to screenshot around GTK menu grabs, for when GUI driving is explicitly requested
metadata:
  type: reference
---

Only for when the user explicitly asks to drive the GUI ([[gui-uat-is-the-users]]). Observed on the Linux workstation (X11, xdotool + imagemagick).

- **`cargo tauri dev` run from `debugger/src-tauri` fails** with `npm ERR! ENOENT ... debugger/package.json`, because `beforeDevCommand: "npm run dev"` resolves its working directory to `debugger/`. Workaround: start `npx vite` in `debugger/frontend` as a background process first. Then run `cargo tauri dev --no-dev-server-wait -c '{"build":{"beforeDevCommand":""}}'` from `debugger/src-tauri`.
- That dev process watches the whole repo. Editing any frontend file triggers a full Rust rebuild and relaunch (new PID, new windows), not just a Vite hot update.
- **`import -window root` hangs while a GTK context menu is open**, because the menu's pointer/keyboard grab blocks `XGetImage`. Screenshotting a specific managed window (`import -window <id>`) still works. A GTK menu reopens with the last-chosen item highlighted, so don't assume a fixed number of `Down` presses reaches an item.
- An empty window-ID lookup passed as `import -window ""` hangs until the tool timeout. Check the ID first.
