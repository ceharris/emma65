---
name: pipe-transport-outbound-capacity
description: PipeTransport sends are all-or-nothing; size the outbound ring for the worst-case synchronous burst, or retry a pending frame gated on has_outbound_capacity
metadata:
  type: feedback
---

`PipeTransport::send_bytes` pushes a whole buffer or drops it; it never writes part of one or blocks. So:

- **Size the ring for the largest burst** one bus write or tick can send with no `.await` in between, e.g. N sub-units × message size, not for one message. In PR #531, the LED matrix ring held one block message, but a swap of all matrices sends one per matrix. Only the first message got through.
- **Alternatively, retry until delivered.** In PR #582, `LcdDisplay` keeps an unsent frame as `pending_frame` and retries it on every `tick()`, with a newer frame replacing it. Without that, a dropped *last* frame left the peripheral showing stale content forever. In PR #588, gating the send and its retries on `Transport::has_outbound_capacity(len)` stopped each failed retry from being logged as a fresh drop, which had produced counts in the millions.

**Why:** a background Tokio drain task can't keep up with synchronous sends in a tight loop, and an atomic drop gives no partial progress.
**How to apply:** for a new pipe-backed device, ask whether one event can send more than one message before yielding. Regression test: spawn a real pipe child (`/bin/cat`, a path that also exists on macOS, see [[macos-linux-portability]]) with the production capacity, send the worst-case burst back to back, and assert every send returns `true`. Reuse `has_outbound_capacity` for any new retry-until-delivered device.
