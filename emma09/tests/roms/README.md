# 6809 test ROMs

Test-only files for `emma09`'s integration tests. Nothing here ships in an emma6k package.

## W. Schwotzer's MC6809 CPU test suite

`cputest.asm` and `cputest.bin`, run by `tests/schwotzer_6809.rs`.

- **Author:** W. Schwotzer, 2003, who validated it on an SGS-Thomson EF6809P.
- **Upstream:** [aladur/flexemu](https://github.com/aladur/flexemu), file `src/tools/cputest.txt`, commit `ef85d17de525711cc290e6946f9a423d7cb400a4` (2016-06-12, "flexemu V2.15 as originally published in March 2004"). The file hasn't changed upstream since.
- **License:** GNU General Public License, version 2: flexemu's license, which the file falls under since it has no header of its own. The full text is in [`COPYING`](COPYING), taken from the same upstream commit. Only these two files are under the GPL; the rest of emma6k is MIT.
- **emma6k's changes**, so that lwasm (lwtools 4.24) assembles it:
  - A comment block at the top recording where the file came from and what changed.
  - Each FLEX `FCC 'text',EOT` line is split into `FCC 'text'` and `FCB EOT`, which assemble to the same bytes.

  Nothing else is changed.

The suite covers every addressing mode, including all indexed and indirect forms, and the arithmetic, logic, shift, rotate, load, store, LEA, TFR and EXG instructions with their condition codes. It does not cover interrupts, SWI, RTI, CWAI, SYNC or cycle counts; `emma09`'s unit tests do.

## The FLEX shim

`flex_shim.asm` and `flex_shim.bin`, written for emma6k (MIT). The suite reports through FLEX monitor entry points: `PUTCHR` (`$CD18`), `PSTRNG` (`$CD1E`) and `PCRLF` (`$CD24`). It ends by jumping to `WARMS` (`$CD03`). The shim implements the first three on a one-byte output register at `$C000`, where the test maps a capture device. The test stops when PC reaches `WARMS`.

## Rebuilding

The binaries are checked in, so `cargo test` doesn't need an assembler. To rebuild them after editing a source, run `./build.sh` here; it needs `lwasm` from [lwtools](http://www.lwtools.ca/).

Sean Riddle's Williams Emulation Test Suite isn't used. It's a ROM for Williams arcade hardware (PIAs, blitter, watchdog, video beam counter), and its 6809 part is an adapted copy of this suite. Its other tests need hardware emma doesn't have, and it states no license.
