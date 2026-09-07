#!/usr/bin/env bash
# Bump emma65-debugger's version across its three lockstep-versioned files:
# debugger/src-tauri/Cargo.toml, debugger/src-tauri/tauri.conf.json, and
# debugger/frontend/package.json. Verifies all three agree before exiting 0.
#
# Usage: bump-debugger-version.sh <new-version>
# Run from the repository root.

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <new-version>" >&2
  exit 1
fi

new_version="$1"

if ! [[ "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: '$new_version' is not a plain X.Y.Z semver" >&2
  exit 1
fi

cargo_toml="debugger/src-tauri/Cargo.toml"
tauri_conf="debugger/src-tauri/tauri.conf.json"
package_json="debugger/frontend/package.json"

for f in "$cargo_toml" "$tauri_conf" "$package_json"; do
  if [ ! -f "$f" ]; then
    echo "error: expected file not found: $f (run this script from the repo root)" >&2
    exit 1
  fi
done

# Cargo.toml: bump the first `version = "..."` line (the [package] version).
sed -i -E "0,/^version *= *\"[^\"]+\"/s//version = \"${new_version}\"/" "$cargo_toml"

# tauri.conf.json: bump the top-level "version" field.
sed -i -E "0,/\"version\" *: *\"[^\"]+\"/s//\"version\": \"${new_version}\"/" "$tauri_conf"

# package.json: bump the top-level "version" field.
sed -i -E "0,/\"version\" *: *\"[^\"]+\"/s//\"version\": \"${new_version}\"/" "$package_json"

fail=0
check() {
  file="$1"
  actual="$2"
  if [ "$actual" != "$new_version" ]; then
    echo "error: $file still reads '$actual' after edit, expected '$new_version'" >&2
    fail=1
  fi
}

cargo_actual=$(grep -m1 '^version *= *"' "$cargo_toml" | sed -E 's/^version *= *"([^"]+)".*/\1/')
check "$cargo_toml" "$cargo_actual"

tauri_actual=$(grep -m1 '"version"' "$tauri_conf" | sed -E 's/.*"version" *: *"([^"]+)".*/\1/')
check "$tauri_conf" "$tauri_actual"

pkg_actual=$(grep -m1 '"version"' "$package_json" | sed -E 's/.*"version" *: *"([^"]+)".*/\1/')
check "$package_json" "$pkg_actual"

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "bumped emma65-debugger to $new_version in:"
echo "  $cargo_toml"
echo "  $tauri_conf"
echo "  $package_json"
