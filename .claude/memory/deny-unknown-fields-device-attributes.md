---
name: deny-unknown-fields-device-attributes
description: Every device module's *Attributes struct gets #[serde(deny_unknown_fields)]; DeviceSpec itself can't because of its flattened attribute map
metadata:
  type: feedback
---

When adding a device config module, put `#[serde(deny_unknown_fields)]` on its `*Attributes` struct.

**Why:** issue #501 (PR #619) made unknown config keys errors at startup. `DeviceSpec` collects per-device attributes through `#[serde(flatten)]` into a map, which can't be combined with `deny_unknown_fields`. So the check lives one level down, in each module's attributes struct, and a new module without it silently accepts typos.
**How to apply:** follow an existing module's attributes struct as the pattern.
