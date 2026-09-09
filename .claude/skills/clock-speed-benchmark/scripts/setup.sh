#!/usr/bin/env bash
# Copies the throwaway clock-speed benchmark examples into examples/ and
# builds them in release mode. Run from the repository root.
#
# Usage: .claude/skills/clock-speed-benchmark/scripts/setup.sh

set -euo pipefail

skill_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ ! -f Cargo.toml ] || [ ! -d src/emulator ]; then
  echo "error: run this from the emma65 repository root" >&2
  exit 1
fi

mkdir -p examples
cp "$skill_dir/assets/clock_bench.rs" examples/clock_bench.rs
cp "$skill_dir/assets/clock_bench_minimal.rs" examples/clock_bench_minimal.rs

echo "Building release examples (this can take a minute on a clean target/)..."
cargo build --release --example clock_bench --example clock_bench_minimal

echo "Ready. Run:"
echo "  ./target/release/examples/clock_bench"
echo "  ./target/release/examples/clock_bench_minimal"
echo "Each takes roughly (number of target speeds + 1) x 3 seconds to complete."
