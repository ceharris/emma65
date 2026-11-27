#!/usr/bin/env bash
# Rebuilds the 6809 test ROMs from their sources with lwasm (lwtools).
# The binaries are checked in, so `cargo test` doesn't need lwasm.
set -euo pipefail
cd "$(dirname "$0")"
lwasm --format=raw -o cputest.bin cputest.asm
# Raw output packs ORG sections together, so the shim pads with ZMB instead
# of ORG to keep its entry points at their FLEX addresses (from $CD00).
lwasm --format=raw -o flex_shim.bin flex_shim.asm
