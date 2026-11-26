# Project memory index

One line per memory file in this directory. See CLAUDE.md's "Agent memory" section for how to use and maintain it.

## Current work
- [emma6k rename (in progress)](emma6k-rename.md) — branch chore/rename-emma6k: what's renamed or kept, PR #661 awaiting review; macOS checkout rename pending; moved checkouts need a tauri `cargo clean`
- [Multi-CPU effort](multi-cpu-effort.md) — 6809 support via a `Processor` trait in this repo; step 1 plan in plan/; user decisions (core crate: emma6k-core)
- [Open issues](open-issues.md) — #646 epic (ask before closing) + follow-ons #656–659; frontend test coverage #559/#560

## Workflow
- [Git workflow](git-workflow.md) — branch → verify → commit → PR with UAT steps + `Closes #n` → wait for human review; "prepare memory" means memory only
- [Multi-unit plan workflow](multi-unit-plan-workflow.md) — one branch+PR per unit, stop after each; check up front that each unit compiles on its own
- [Branch from origin/main](branch-from-origin-main.md) — unpushed local-main commits get swept into squash merges
- [No worktrees](no-worktrees.md) — plain `git checkout -b` in the main checkout; worktrees broke the user's UAT
- [Git uses SSH](git-push-via-ssh.md) — origin is SSH on both hosts; push normally, no HTTPS credential workarounds
- [GitHub bodies: no hard wrap](github-body-no-hard-wrap.md) — one line per paragraph in issue/PR bodies and comments
- [CI clippy denies warnings](ci-clippy-deny-warnings.md) — any clippy warning fails CI; run CI's exact `-D warnings` commands before pushing
- [Sudo via script](sudo-via-script.md) — no passwordless sudo; write a script for the user to run
- [Debug the CI image locally](debug-ci-image-locally.md) — build/run .github/docker/ci locally instead of push-and-wait
- [Check action Node runtime](check-action-node-runtime.md) — verify `runs.using` in action.yml before pinning an action version

## Debugger GUI
- [GUI UAT is the user's](gui-uat-is-the-users.md) — never launch or drive the app unless the current message asks
- [GUI automation quirks](debugger-gui-automation-quirks.md) — `cargo tauri dev` launch workaround; screenshots vs GTK menu grabs
- [Diagnose GUI bugs with evidence](diagnose-gui-bugs-with-evidence.md) — instrument and ask for precise descriptions; don't iterate on theories
- [Escalate unconverging GUI repro](escalate-unconverging-gui-repro.md) — after 2–3 fruitless live rounds, re-read the code or bring in a stronger-model reviewer
- [Hand-styled controls](debugger-hand-styled-controls.md) — no native popups (select, datalist); datalist hung the WebKitGTK app
- [Tauri new-window capabilities](tauri-new-window-capabilities.md) — new windows need tauri.conf.json AND capabilities entries, or events vanish

## Code and design
- [Pipe transport outbound capacity](pipe-transport-outbound-capacity.md) — sends are all-or-nothing; size for bursts or retry gated on has_outbound_capacity
- [SDL2_gfx color byte order](sdl2-gfx-color-byte-order.md) — DrawRenderer colors need a native-order u32 (gfx_color), not a Color
- [Consumer-side device logging](consumer-side-device-logging.md) — logging decisions live in DeviceEvent consumers, not devices or the Bus
- [deny_unknown_fields on attributes](deny-unknown-fields-device-attributes.md) — every device *Attributes struct gets it
- [Avoid side-effect caveats](avoid-side-effect-caveats.md) — a design needing a caveat is a cue to drop the capability
- [macOS + Linux portability](macos-linux-portability.md) — Linux is primary; macOS port NOT complete (SDL2 doesn't link); no Linux-only APIs or paths
- [Clock bench baselines](clock-bench-baseline.md) — per host: Linux Ryzen ~34/~85 MHz (2026-09, re-take on main), macOS i9 ~26/~50 MHz

## Docs
- [plan/ frozen, doc/ current](plan-frozen-doc-current.md) — never retrofit superseded plan docs; always keep the wiki current
- [Wiki is user-facing](wiki-user-facing-writing.md) — no Rust internals, paths or issue numbers outside For Contributors; check facts against source
