---
name: debug-ci-image-locally
description: When a CI job inside the project's own Docker image fails non-obviously, reproduce it in a locally built copy of the image instead of push-and-wait cycles
metadata:
  type: feedback
---

To reproduce a failing CI job locally:
1. Build the image: `docker build -t <tag> -f .github/docker/ci/Dockerfile .`
2. Start a long-lived container with the repo mounted: `docker run --rm -d -v $(pwd):/work -w /work <tag> sleep infinity`
3. `docker exec` in, run the job's per-job toolchain steps, then run the failing command with verbose flags.
4. Fix the Dockerfile, then confirm with one clean run in a *fresh* container.
5. Before committing, clean up root-owned `target/` and `node_modules/` output.

**Why:** in the first debugger release, Tauri's swallowed `failed to run linuxdeploy` error was traced in minutes to a missing `file` package. Each push-and-wait CI round costs about 5–8 minutes just to reach the failure again.
**How to apply:** this works for any job that runs in the CI container image. Docker is on the Linux workstation; it isn't recorded for the Mac.
