#!/usr/bin/env bash
# Rebuilds the CamelForth template's program.bin and program.lbl from the
# GPL-3 source in forth/. Both outputs are checked in, so normal builds don't
# need this.
#
# Needs GForth 0.7.0 to 0.7.3 (Ubuntu's `gforth` package is 0.7.3). Later
# development snapshots, such as the 0.7.9 snap, stop partway through the
# metacompiler. Set GFORTH to choose the program, for example
#   GFORTH=/usr/bin/gforth ./build.sh
# and python3 for tools.py.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
gforth="${GFORTH:-gforth}"

version="$("$gforth" --version 2>&1 | head -1)"
if ! [[ "$version" =~ ^gforth\ 0\.7\.[0-3]([^0-9]|$) ]]; then
    echo "build.sh: needs GForth 0.7.0-0.7.3, but '$gforth' is: $version" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
python3 "$here/tools.py" to-block "$here/forth/chromium_ans.txt" "$work/chromium_ans.scr"
cp "$here/forth/gforth_shim.fs" "$work/"
(cd "$work" && "$gforth" gforth_shim.fs -e bye < /dev/null > build.log 2>&1) || {
    tail -20 "$work/build.log" >&2
    echo "build.sh: the GForth build failed" >&2
    exit 1
}
[[ -f "$work/6809M.HEX" ]] || { tail -20 "$work/build.log" >&2; echo "build.sh: no 6809M.HEX" >&2; exit 1; }
python3 "$here/tools.py" hex2bin "$work/6809M.HEX" "$here/program.bin"
python3 "$here/tools.py" labels "$work/build.log" "$here/program.lbl"
echo "build.sh: wrote program.bin and program.lbl"
