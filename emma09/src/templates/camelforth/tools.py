#!/usr/bin/env python3
"""Build helpers for emma6k's CamelForth template (MIT, part of emma6k).

    tools.py to-text  <file.scr> <file.txt>   Forth block file -> text master
    tools.py to-block <file.txt> <file.scr>   text master -> Forth block file
    tools.py hex2bin  <file.hex> <file.bin>   Intel HEX -> 8K ROM image at $E000
    tools.py labels   <build.log> <file.lbl>  metacompiler symbols -> VICE labels

A Forth block file is a sequence of 1024-byte screens of 16 lines x 64
characters. The text master holds, for each screen that isn't entirely
blank, a header line "=== screen N" and then its 16 lines with trailing
spaces removed; lines may be at most 64 characters. Converting to blocks pads
lines with spaces and writes blank screens for the ones the text omits.
"""
import re
import sys

LINE, LINES = 64, 16
SCREEN = LINE * LINES
MIN_SCREENS = 256
ROM_BASE, ROM_SIZE = 0xE000, 0x2000


def to_text(src, dst):
    data = open(src, "rb").read()
    out = []
    for n in range(len(data) // SCREEN):
        block = data[n * SCREEN:(n + 1) * SCREEN]
        if block.strip(b" ") == b"":
            continue
        out.append(f"=== screen {n}")
        for i in range(LINES):
            out.append(block[i * LINE:(i + 1) * LINE].decode("latin-1").rstrip(" "))
    open(dst, "w", encoding="latin-1").write("\n".join(out) + "\n")


def to_block(src, dst):
    screens = {}
    current = None
    for number, line in enumerate(open(src, encoding="latin-1").read().split("\n"), 1):
        if line.startswith("=== screen "):
            current = int(line.split()[2])
            screens[current] = []
        elif current is not None and len(screens[current]) < LINES:
            if len(line) > LINE:
                sys.exit(f"{src}:{number}: line longer than {LINE} characters")
            screens[current].append(line)
        elif line.strip():
            sys.exit(f"{src}:{number}: text outside a screen, or a 17th line")
    count = max(MIN_SCREENS, max(screens, default=-1) + 1)
    out = bytearray(b" " * (count * SCREEN))
    for n, lines in screens.items():
        for i, line in enumerate(lines):
            start = n * SCREEN + i * LINE
            out[start:start + len(line)] = line.encode("latin-1")
    open(dst, "wb").write(out)


def hex2bin(src, dst):
    rom = bytearray(b"\xff" * ROM_SIZE)
    for number, line in enumerate(open(src), 1):
        line = line.strip()
        if not line:
            continue
        if not line.startswith(":"):
            sys.exit(f"{src}:{number}: not an Intel HEX record")
        record = bytes.fromhex(line[1:])
        if sum(record) & 0xFF:
            sys.exit(f"{src}:{number}: bad checksum")
        length, address, kind = record[0], (record[1] << 8) | record[2], record[3]
        if kind == 1:
            break
        if kind != 0:
            sys.exit(f"{src}:{number}: unsupported record type {kind}")
        if not ROM_BASE <= address <= address + length - 1 < ROM_BASE + ROM_SIZE:
            sys.exit(f"{src}:{number}: data outside the ROM at ${address:04X}")
        rom[address - ROM_BASE:address - ROM_BASE + length] = record[4:4 + length]
    open(dst, "wb").write(rom)


def labels(src, dst):
    """Extracts the metacompiler's symbol dump (.MIRRORS with EVERYONE set):
    lines of the form "NAME : value= XXXX ...". Keeps the symbols whose values
    fall in the ROM, so constants such as UP-INIT-HI don't label low memory."""
    pattern = re.compile(r"^(\S+)\s+: value=\s*([0-9A-F]+)\s")
    found = {}
    for line in open(src, encoding="latin-1"):
        match = pattern.match(line)
        if match:
            value = int(match.group(2), 16)
            if ROM_BASE <= value < ROM_BASE + ROM_SIZE:
                found[match.group(1)] = value
    if not found:
        sys.exit(f"{src}: no symbols found")
    with open(dst, "w") as out:
        for name, value in sorted(found.items(), key=lambda kv: (kv[1], kv[0])):
            out.write(f"al {value:04X} .{name}\n")


if __name__ == "__main__":
    commands = {"to-text": to_text, "to-block": to_block, "hex2bin": hex2bin, "labels": labels}
    if len(sys.argv) != 4 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2], sys.argv[3])
