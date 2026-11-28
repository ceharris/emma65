#!/usr/bin/env python3
"""Checks every_form.txt against lwasm: each valid listing must assemble,
at $1000, back to exactly the bytes it was disassembled from.

Usage: python3 lwasm_check.py [path/to/every_form.txt]   (needs lwasm on PATH)
"""
import os
import subprocess
import sys
import tempfile

ORG = 0x1000


def pcr_register_bits_only(listing, got, want):
    """PCR postbytes ignore their register bits ($60), which lwasm writes as 0."""
    if got is None or len(got) != len(want) or ",PCR" not in listing:
        return False
    diffs = [(g, w) for g, w in zip(got, want) if g != w]
    return len(diffs) == 1 and (diffs[0][0] ^ diffs[0][1]) & ~0x60 == 0


def main():
    table = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "every_form.txt")
    failures = checked = 0
    with tempfile.TemporaryDirectory() as work:
        src = os.path.join(work, "line.asm")
        out = os.path.join(work, "line.bin")
        for number, line in enumerate(open(table), 1):
            hex_bytes, listing = (part.strip() for part in line.split("|", 1))
            if listing.startswith("<ILL>"):
                continue
            want = bytes.fromhex(hex_bytes)
            with open(src, "w") as f:
                f.write(f" org ${ORG:04X}\n {listing}\n")
            result = subprocess.run(
                ["lwasm", "--6809", "--format=raw", "-o", out, src],
                capture_output=True, text=True)
            got = open(out, "rb").read() if result.returncode == 0 else None
            checked += 1
            if got != want and not pcr_register_bits_only(listing, got, want):
                failures += 1
                detail = got.hex(" ").upper() if got is not None else result.stdout + result.stderr
                print(f"line {number}: {listing!r}: want {hex_bytes}, got {detail.strip()}")
    print(f"{checked} listings checked, {failures} mismatched")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
