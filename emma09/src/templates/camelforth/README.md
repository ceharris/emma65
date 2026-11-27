# CamelForth starter profile

The 6809's default starter profile: CamelForth for the Motorola 6809, rebuilt to run on emma6k's console device.

## License

**This template is licensed under the GNU General Public License, version 3 or later, unlike the rest of emma6k (MIT).** That covers the source in `forth/` and the ROM image (`program.bin`) and labels (`program.lbl`) built from it. The full license text is in [`forth/copying`](forth/copying).

- CamelForth for the Motorola 6809: Copyright 1995 Bradford J. Rodriguez.
- The Chromium 2 metacompiler and its 6809 cross-assembler, which build it: Copyright 1995 Bradford J. Rodriguez.
- The Multicomp 6809 port, its ANS/GForth version of the metacompiler (`chromium_ans.scr`), and `gforth_shim.fs`: Neal Crook.

**Upstream:** [nealcrook/multicomp6809](https://github.com/nealcrook/multicomp6809), directory `camelforth/`, at commit `d9171db2fd616a4ef40adbcf11b3e82c2c36b047` (2025-10-27). The build procedure is the upstream wiki's [CamelForthRebuildANS](https://github.com/nealcrook/multicomp6809/wiki/CamelForthRebuildANS) page.

## emma6k's changes

`forth/chromium_ans.txt` is upstream's `chromium_ans.scr` block file converted to text (`tools.py to-text`; the conversion round-trips byte for byte), then changed in four screens:

- **Screen 45 (load screen):** the Multicomp hardware extras, screens 50–59 and 121–127, aren't loaded. They hold the SD card driver, MMU set-up, the loaders for other operating systems, and the SD-card-backed ANS BLOCK words, none of which emma6k emulates.
- **Screen 46 (memory map):** the user area moves from `$0600` (sized for a 2K-RAM board) to `$DE00`, following upstream's rule that it sits at RAM top − `$200`, for RAM from `$0000` to `$DFFF`.
- **Screen 48 (console I/O):** `KEY`, `KEY?` and `EMIT` use emma6k's console device (data register at `$FFD0`, latch register at `$FFD1`) instead of the Multicomp VDU. `KEY?` now returns a true flag of `-1`.
- **Screen 49 (reset entry and vectors):** the reset entry unmasks IRQ, and a new IRQ handler answers the console's break key (Ctrl+C): it clears the break, resets the stacks, and jumps to `ABORT`, which returns to the interpreter. Screen titles mark each changed screen.

`gforth_shim.fs` and `copying` are unchanged.

## Using it

`emma09` with no configuration runs this profile, as does `emma6k --profile camelforth`. It has 56K of RAM (`$0000–$DFFF`), the 8K ROM at `$E000`, and the console at `$FFD0` over the ROM's old I/O window, at 2 MHz. Type Forth at the `OK` prompt. Ctrl+C abandons a running word (an endless loop, say) and returns to the interpreter without printing a new prompt; press Enter to see one. Words already defined survive; the data stack is cleared. To quit the emulator, close the terminal, or start it with `--keep-isig` so that Ctrl+C ends it instead.

## Rebuilding

`program.bin` and `program.lbl` are checked in, so normal builds don't need this. To rebuild after editing `forth/chromium_ans.txt`, run `./build.sh`. It needs GForth 0.7.0–0.7.3 (Ubuntu's `gforth` package is 0.7.3; the 0.7.9 development snapshot distributed as a snap stops partway through the metacompiler) and Python 3. Set `GFORTH` to choose the GForth program. `tools.py` (MIT, written for emma6k) converts between the block file and the text master, converts the Intel HEX output to the ROM image, and turns the metacompiler's symbol dump into VICE labels.
