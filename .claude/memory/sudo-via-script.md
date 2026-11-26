---
name: sudo-via-script
description: No passwordless sudo; for privileged diagnostics or installs, write a script file for the user to review and run with sudo
metadata:
  type: feedback
---

When something needs root (gdb or strace attaching under `ptrace_scope=1`, package installs), write the commands to a script in the scratchpad, `chmod +x` it, and give the user the `sudo <path>` command to run. Never lower security settings or ask for a password.

**Why:** observed on the Linux workstation: there is no passwordless sudo, and the user offered to run sudo commands themselves. They pushed back on a long inline one-liner as "really hard to read" and preferred a script file.
**How to apply:** use a script even for one-off commands. Whether the macOS workstation is the same isn't recorded.
