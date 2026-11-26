---
name: git-push-via-ssh
description: The git remote uses SSH; push with plain `git push`, never HTTPS credential workarounds
metadata:
  type: feedback
---

Use SSH for git with GitHub. `origin` was switched from HTTPS to SSH on 2026-11-26 at the user's request, and `gh` is also set to the ssh protocol. After the repo rename ([[emma6k-rename]]), `origin` is `git@github.com:ceharris/emma6k.git`. That's set on Linux; the macOS checkout still needs it.

**Why:** the old HTTPS origin had no stored credential, so pushes failed with "could not read Username". The user said "You can use ssh with git".

**How to apply:** push normally (`git push -u origin <branch>`). If a push asks for HTTPS credentials, check `git remote -v` rather than working around it with credential-helper tricks. Both workstations have `origin` on SSH; the Linux one was confirmed 2026-11-26. The Linux workstation has `gh`, installed 2026-11-26, with the `repo`, `read:org`, `gist`, `admin:public_key` and `read:packages` scopes.
