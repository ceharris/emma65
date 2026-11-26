---
name: sdl2-gfx-color-byte-order
description: sdl2::gfx DrawRenderer colors must be passed as a native-byte-order u32 (the gfx_color helper), not a raw sdl2::pixels::Color
metadata:
  type: feedback
---

Never pass an `sdl2::pixels::Color` straight to an `sdl2::gfx::primitives::DrawRenderer` method (`filled_circle`, `pixel`, …). Pass `u32::from_ne_bytes([r, g, b, a])`; `led-matrix/src/main.rs`'s `gfx_color` helper does this.

**Why:** `impl ToColor for Color` packs the color big-endian, but SDL2_gfx's C side reads the `u32` as bytes in native order. On little-endian hosts every channel is swapped, and a color with red = 0 is drawn fully transparent. Colors with high red happened to work, which hid the bug (PR #529).
**How to apply:** this applies to any future `DrawRenderer` use in the workspace. To regression-test, draw on a surface-backed canvas in `PixelFormatEnum::RGBA32` and read the pixels back. Don't use `RGBA8888`, whose in-memory byte order also depends on endianness.
