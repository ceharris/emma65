#!/usr/bin/env bash
# Removes the throwaway clock-speed benchmark examples and confirms the
# working tree is clean afterward. Run from the repository root, after
# capturing whatever benchmark output you need.
#
# Usage: .claude/skills/clock-speed-benchmark/scripts/cleanup.sh

set -euo pipefail

if [ ! -f Cargo.toml ] || [ ! -d src/emulator ]; then
  echo "error: run this from the emma65 repository root" >&2
  exit 1
fi

rm -f examples/clock_bench.rs examples/clock_bench_minimal.rs
rmdir examples 2>/dev/null || true

status="$(git status --porcelain)"
if [ -n "$status" ]; then
  echo "warning: working tree is not clean after cleanup:" >&2
  echo "$status" >&2
  exit 1
fi

echo "Cleaned up; working tree is clean."
