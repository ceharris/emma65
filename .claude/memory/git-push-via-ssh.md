---
name: git-push-via-ssh
description: The git remote uses SSH; push with plain `git push`, never HTTPS credential workarounds
metadata:
  type: feedback
---

Use SSH for git with GitHub. `origin` was switched from HTTPS to `git@github.com:ceharris/emma65.git` on 2026-11-26 at the user's request. `gh` is also set to the ssh protocol. Once the GitHub repo is renamed (see [[emma6k-rename]]), `origin` becomes `git@github.com:ceharris/emma6k.git`.

**Why:** the old HTTPS origin had no stored credential, so pushes failed with "could not read Username". The user said "You can use ssh with git".

**How to apply:** push normally (`git push -u origin <branch>`). If a push asks for HTTPS credentials, check `git remote -v` rather than working around it with credential-helper tricks. These notes came from the macOS workstation; confirm with `git remote -v` that the current host is set up the same way.
