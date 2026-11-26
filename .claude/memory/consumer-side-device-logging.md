---
name: consumer-side-device-logging
description: Logging decisions belong on the DeviceEvent/ErrorSender consumer side, not inside individual devices or the Bus
metadata:
  type: feedback
---

Don't add logging plumbing (a `Bus`-owned log sender, a "log" variant on a device's policy enum) to devices or the bus. Devices report through the existing `ErrorSender`/`DeviceEvent` path, and the consumer (`log_device_event`/`log_device_events` in `src/emulator/device/mod.rs`) decides what to log.

**Why:** PR #638 (#635, ROM write policy) added `RomWritePolicy::Log` and a `Bus`-owned `LogSender`, and was rejected. The smaller PR #639, which reused the existing write-policy handling, was merged.
**How to apply:** don't reintroduce that direction when adding device diagnostics.
