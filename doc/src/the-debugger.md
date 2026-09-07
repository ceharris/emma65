# The Debugger

`emma65-debugger` is a native desktop application (built with
[Tauri](https://tauri.app)) that turns the emulator into a full interactive
development environment for 65C02 programs. Its main window is a freely
rearrangeable dock of panels — disassembly, memory, registers, stack,
breakpoints, watchpoints, symbols, a live execution trace, a log, a built-in
terminal, an assembler, and one panel per graphical display device — plus an
always-visible status bar, a native menu bar, and a set of keyboard shortcuts
that mirror the menu commands. A segmented Auto/Dark/Light control in the
toolbar switches the whole UI's theme independent of the OS.

## Profiles

The debugger organizes its configuration into **profiles**: a profile is a
self-contained emulator configuration (the same TOML format described under
[Running the Emulator](running-the-emulator.md)) plus its own watchpoints.
The default profile lives at `~/.emma/debugger/profiles/default/`, with its
emulator configuration in `emulator.toml` and its watchpoints in
`watchpoints.emw`. The dock layout and other UI preferences — theme,
terminal settings, exit-confirmation — aren't tied to any particular
profile, so switching profiles never rearranges the window.

Four File menu items manage profiles, and — because switching or reloading a
profile tears down and rebuilds the active session — all four are available
only while the CPU is stopped:

- **New Profile** (`Ctrl+N`) opens a dialog for a new profile's name and a
  starter template to seed it from, then switches to it immediately. The
  bundled templates cover a range of starting points, from a bare interpreter
  to a graphical demo:
  - **TaliForth2** (the default) — Forth-2012 on the emulator's standard
    full device set: 32 KB RAM, 32 KB ROM, a VIA, two ACIAs, an LFSR, and
    the console
  - **Microsoft BASIC** — 48 KB RAM, 12 KB ROM, a VIA, and the console for
    input and output
  - **EhBASIC** — Lee Davison's EhBASIC, the same 48 KB RAM/12 KB ROM/VIA/
    console arrangement as Microsoft BASIC
  - **Digital Rain** — a "digital rain" demo driving the memory-mapped
    character display (also used for keyboard input) and using the LFSR
    for pseudo-randomness
  - **LCD Display** — a demo exercising the HD44780-compatible LCD display,
    with a VIA
  - **Snake** — the classic game, played over the console (input and
    output), with a VIA for timing and the LFSR for pseudo-randomness
- **Open Profile** (`Ctrl+O`) shows a native folder picker (defaulting to the
  profiles directory) and switches to whatever profile directory is chosen,
  filling in any files it's missing (an `emulator.toml`, say, but no
  `watchpoints.emw` yet) rather than requiring a fully-formed profile.
- **Reload Profile** (`Ctrl+Shift+R`) re-reads the active profile's files
  from disk without switching away from it — useful after editing
  `emulator.toml` by hand, or after a separate assembler or IDE has produced
  a new ROM image or labels file for the profile to pick up.
- **Open Recent** lists recently activated profiles for one-click switching,
  with a "Clear Recent…" item to empty that list.

## Docking and Window Layout

Every panel described below is a dockable tab: drag its tab header to split
the window and dock it in a new position, or to tab-group it with another
panel; drag the sash between groups to resize them; drag a tab out to float
it as its own panel within the main window. Four panels — Terminal,
Display, LED Matrix, and LCD Display — go a step further and can be fully
detached into independent OS-level windows, toggled from the Window menu or
their own keyboard shortcut; the menu item's label flips between "Detach
X…" and "Attach X" to reflect the current state, and a detached window's
native close button reattaches it just like the shortcut does.

Closing a panel's dock tab removes it from view entirely; the View menu —
one plain item per panel — is how it comes back, even if the tab was closed
outright rather than just buried behind another tab. A handful of panels
also expose a small action icon directly in their tab header: Breakpoints
and Watchpoints get a "+" (add) icon there, and Terminal gets a size-preset
icon when docked (see [Terminal](#terminal) below).

The entire arrangement — dock positions, sizes, tab groupings, and which of
the four detachable panels are currently detached — persists across
restarts. Window > Restore Layout… discards all of that, after a
confirmation prompt, and rebuilds the application's built-in default
arrangement.

## Panels

### Registers

Shows the CPU's registers as two groups — data (A, X, Y) and address/status
(PC, S, P plus flags) — each with its own radix button that cycles the
display base (hex, unsigned/signed decimal, octal, and, for the 8-bit data
group, binary). The A register's value also shows its printable ASCII
character alongside the number when applicable. The status register's flags
are shown as individual letters (N V - B D I Z C); a flag that changed on
the most recent step is highlighted.

While the CPU is stopped, double-clicking a register value opens an inline
edit field pre-filled with the current value — type a bare number in the
field's current radix, or override it with an explicit `$`/`0x`, `0o`/`0q`,
`0b`, or `0d`/`.` prefix (a leading `+`/`-` selects signed decimal). Enter
commits, Escape or clicking away cancels. Double-clicking the flags display
similarly turns each flag letter into a click-to-toggle control. None of
this is editable while the CPU is running.

### Disassembly

A scrolling, symbol-annotated instruction listing — breakpoint gutter,
address, raw opcode bytes, mnemonic, operand, and any inline comment, with
label rows interleaved above the instructions they annotate. The row at the
current program counter is highlighted and scrolled into view on every
halt or step, and the view auto-extends as execution approaches the bottom
of the currently loaded window.

Clicking a row's gutter (while the CPU is stopped) sets or removes a
breakpoint there; right-clicking a row opens a context menu with
Set/Enable/Disable/Remove Breakpoint (whichever apply) plus "Set Breakpoint
at Address…" for an arbitrary address. An address field in the panel header
jumps the view to a typed symbol name or hex address. Breakpoint state stays
in sync with the [Breakpoints](#breakpoints) panel no matter which one made
the change.

### Run Controls

A single fixed-height toolbar hosting Run, Stop, Step Into, Step Over, and
Step Return, plus an Auto-Step toggle with a speed slider and a millisecond
entry field for its interval. Every button here has an identical entry in
the top-level Run menu, kept enabled and disabled in lockstep, and each
enables only in the states where it makes sense — Run and the Step buttons
disable while free-running, auto-stepping, or already stepping; Stop enables
only while free-running. Reset and the IRQ/NMI controls live in the
[status bar](#status-bar) instead, not here — and there's no clock-speed
control anywhere in the UI, since clock speed is a configured, not a
live-adjustable, property of the emulated CPU.

### Memory

Displays one 256-byte page at a time as 16 rows of 16 bytes, each with its
address, hex bytes, and ASCII rendering (non-printable bytes shown as
`.`). Typing a hex address or symbol name into the header field and
pressing Enter jumps to (and page-aligns) that location; the mouse wheel and
arrow keys scroll a row at a time, Page Up/Down a full page, wrapping around
the 64 KB address space. Hovering a byte or its ASCII character shows any
symbol defined at that address as a tooltip.

While the CPU is stopped, double-clicking a hex byte or its ASCII character
opens an Edit Memory dialog pre-set to that address, in hex or text mode
respectively; it also has an "Allow ROM Overwrite" option for patching
otherwise write-protected regions. Three more dialogs, reached only through
the top-level Memory menu (there are no in-panel buttons): Load from
File… (auto-detects Binary Image, Intel Hex, or Motorola S-Record format
from the extension, with an optional VICE-format symbol file to import
alongside it), Save to File… (a start/end address range), and Fill…
(a start/end range plus a fill byte). All Memory menu commands, like the
in-panel edit, are available only while the CPU is stopped.

### Stack

Shows eight rows of the current stack page as word pairs, with a marker on
the most recently pushed byte and the stack pointer's own slot rendered as a
placeholder rather than a stale value. A radix button cycles the display
base; an EVEN/ODD toggle shifts how bytes are paired into words, a display
convenience with no effect on the underlying data. It always tracks the
active stack position — there's no manual scrolling.

### Breakpoints

A flat list of every breakpoint: an enable/disable indicator (click to
toggle), the address, any symbol that resolves to it, and a remove button.
The tab header's "+" icon opens an Add Breakpoint popover accepting a hex
address or a symbol name. All actions require the CPU to be stopped, and the
list stays in sync with breakpoints set or toggled from the
[Disassembly](#disassembly) panel's gutter or context menu.

### Watchpoints

Lists each watchpoint from `watchpoints.emw` with an enable/disable
indicator, its expression source, and a remove button; a row is colored by
its current state — disabled, a compile/evaluation error, currently
triggered, or not triggered. Double-clicking a row's expression opens an
Edit Watchpoint popover; the tab header's "+" icon opens Add Watchpoint the
same way. A collapsible Variables section below the list shows every named
`:=` variable and its current value. All editing is available only while
the CPU is stopped and the file has no compile error; see
[Watchpoint Expressions](#watchpoint-expressions) below for the expression
language itself.

### Symbols

A sortable, filterable table of the program's symbol table — Name, Address,
Source, and Aliases columns. A filter field does live substring matching
across all four columns; clicking a column header sorts by it, clicking
again reverses direction. Column widths are user-adjustable and persist
across restarts. It refreshes automatically whenever the symbol table
changes, such as after assembling and loading a program or loading a memory
image with an associated symbol file.

### Trace

A live view of recently executed instructions, recorded via the same
facility described in
[Execution Tracing](the-emulator-core.md#execution-tracing). A toolbar
starts recording to a chosen file, stops it, or pauses just the live-follow
scrolling without stopping the recording itself. Below it, a windowed log
shows sequence number, cycle count, every register, decoded flags, address,
raw bytes, and the disassembled instruction, with the same label-row
interleaving as the Disassembly panel; while recording and unpaused, the
view auto-follows the newest instructions. Clicking a row populates a Bus
Operations pane below the log with every bus read or write that instruction
performed.

### Log

A running table of emulator log messages — timestamp, cycle count, level,
category, and message — that always auto-scrolls to show the newest entry.
There's no filtering or manual scroll-lock; it simply accumulates.

### Terminal

A full terminal emulator wired directly to the configured, memory-mapped
console device, so a running program can be interacted with directly, with
no external terminal emulator or PTY setup needed. Right-clicking the
terminal, or clicking the hamburger icon in its dock-tab header, opens a
size-preset menu offering four fixed grid sizes (80×24, 132×24, 80×43,
132×43) that resize the panel — or, once detached, the OS window itself — to
match exactly. The same menu's Preferences… item opens a tabbed dialog
covering text appearance (font, scrollback, colors, the ANSI palette),
cursor style and blink, and Backspace/Delete key compatibility. Copy and
paste use Ctrl+Shift+C/V specifically so they don't collide with the
terminal's own use of Ctrl+C and Ctrl+V.

### Display

Renders the memory-mapped [character display device](io-devices.md#character-display-display)'s
composited output, scaled to the largest clean integer multiple of its
native resolution that fits the panel. The canvas auto-focuses itself
whenever its tab or window becomes active, and — when the device is
configured with a keyboard range — forwards keystrokes to the emulated
keyboard as single bytes, so a program can be driven entirely from this
panel with no separate input device needed.

### LED Matrix

Renders the memory-mapped [RGB LED matrix device](io-devices.md#rgb-led-matrix-display-displaymatrix)
as a grid of round LEDs on a dark PCB-styled background, deliberately
modeled on a real hobbyist LED matrix panel rather than a flat pixel blit.
Multiple attached matrices are laid out edge-to-edge exactly as the
device's own `arrangement` describes; there's no independent
panel-side layout control. Display-only — it accepts no keyboard or mouse
input.

### LCD Display

Renders the memory-mapped [LCD display device](io-devices.md#lcd-display-displaylcd)
as a dot-matrix grid behind a bezel, using the device's configured
polarity, backlight, and geometry to reproduce a real module's look — dim
"off" segments stay faintly visible against the backlight rather than going
flat black. Display-only, like LED Matrix.

### Assembler

A full source editor (line numbers, a lint gutter for assembly errors,
tab-to-indent, undo/redo) for writing 6502 assembly and loading it straight
into the emulator's memory. It's meant for quickly assembling small programs
and patches directly against a running session — sketching out a routine,
tweaking a few instructions, and reloading them without leaving the
debugger — rather than replacing a full assembler toolchain or IDE for
larger projects. File operations — New, Open…, Save, Save As… —
are reached through the top-level Assembler menu; the dock tab's title
tracks the open file's name, with a trailing `*` while there are unsaved
changes. Assemble… compiles the current buffer and, on success, opens a
confirmation dialog listing each output segment's origin address and byte
length before anything is actually written to memory — a successful compile
alone never touches memory. A failed assemble shows errors both as gutter
markers with hover text and as a plain-text list below the editor. Assemble…
is available only while the CPU is stopped; the file operations are always
available.

## Status Bar

A slim bar fixed to the bottom of the main window — not a dock panel, so
it's visible no matter what's docked, floated, or hidden — showing, left to
right: a running cycle count; the emulator's current effective clock speed;
an NMI indicator that also triggers a one-shot NMI on click; an IRQ
indicator that asserts or releases IRQ as a toggle while the CPU is stopped,
or fires a one-shot pulse while it's running; a Run/Stop indicator that also
distinguishes a CPU halted on a `STP` or `WAI` instruction from an ordinary
stop; and a Reset button. This is the only place Reset lives in the UI —
there's no equivalent button on the Run Controls panel.

## Menus and Keyboard Shortcuts

The native menu bar is File, Edit, View, Run, Memory, Assembler, Window, and
Help:

- **File** — New/Open/Reload Profile and Open Recent (see
  [Profiles](#profiles) above), plus Exit. Everything but Exit requires the
  CPU to be stopped, since each reloads the active session.
- **Edit** — Cut, Copy, Paste, enabled according to whatever panel
  currently has focus and a selection (the Terminal and Assembler panels
  both participate). Deliberately unaccelerated so nothing here steals
  Ctrl+C/Ctrl+V from the terminal.
- **View** — one item per panel (see [Docking and Window Layout](#docking-and-window-layout)
  above) to reveal a closed or buried dock tab.
- **Run** — mirrors the [Run Controls](#run-controls) panel exactly.
- **Memory** — the sole way to reach Memory's Load/Save/Edit/Fill dialogs.
- **Assembler** — New/Open/Save/Save As…/Assemble…, mirroring the
  [Assembler](#assembler) panel's own menu-only workflow.
- **Window** — Detach/Attach for Terminal, Display, LED Matrix, and LCD
  Display, plus Restore Layout….
- **Help** — View on GitHub, and an About dialog with build information.

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` / `Ctrl+O` / `Ctrl+Shift+R` | New / Open / Reload Profile |
| `Ctrl+Q` | Exit |
| `F5` / `Shift+F5` | Run / Stop |
| `F10` / `F11` / `Shift+F11` | Step Over / Step Into / Step Return |
| `Ctrl+Shift+F5` | Toggle Auto-Step |
| `Ctrl+L` / `Ctrl+S` | Memory: Load from File… / Save to File… |
| `Ctrl+Shift+E` / `Ctrl+Shift+F` | Memory: Edit… / Fill… |
| `Alt+N` / `Alt+O` / `Alt+S` / `F9` | Assembler: New / Open… / Save / Assemble… |
| `Ctrl+Shift+T` / `D` / `M` / `I` | Detach or attach Terminal / Display / LED Matrix / LCD Display |
| `Ctrl+Shift+C` / `Ctrl+Shift+V` | Copy / Paste, inside the Terminal panel specifically |

The four detach/attach shortcuts work from any window, including a
detached device's own window, so a panel can be sent back to the dock
without switching back to the main window first.

## Watchpoint Expressions

Watchpoints are boolean expressions evaluated against live machine state
before each instruction; each line of `watchpoints.emw` is one watchpoint,
and the Watchpoint panel shows whether it's currently triggered. The
expression language covers:

- **Registers** — `A`, `X`, `Y`, `P`, `S`, `PC`
  ```
  X > 10
  PC == $8010
  ```
- **CPU status flags**, prefixed with a backtick — `` `N ``, `` `V ``,
  `` `B ``, `` `D ``, `` `I ``, `` `Z ``, `` `C ``
  ```
  `C
  `N && `Z
  ```
- **Literals** — decimal, or hex with a `$` or `0x` prefix (`0o`/`0q`
  octal and `0b` binary are also recognized)
  ```
  A == 42
  A == $2A
  ```
- **Memory operands** — `B[addr]`, `W[addr]`, `D[addr]` read a byte, word, or
  doubleword from memory; a leading `+` or `-` interprets the value as signed
  (`-` also negates it)
  ```
  B[$0200] == $FF
  +B[$D010] < 0    // true when bit 7 (the sign bit) of the byte at $D010 is set
  W[$FE] != 0
  ```
- **Symbols** — a bare identifier resolves to the address of a label loaded
  from a VICE-format label file (the `labels` device attribute), so a
  watchpoint can reference a source-level name instead of a hardcoded address
  ```
  PC == reset_vector
  B[cursor_x] > 79
  ```
- **Arithmetic, bitwise, and comparison operators** — `+ - * / %`,
  `& | ^ ~`, `<< >>`, `== != < <= > >=`, `&& || !`
  ```
  (B[$D010] & $80) != 0
  ```
- **The walrus operator (`:=`)** snapshots a value into a named variable that
  persists across steps, so one watchpoint can be compared against a value
  captured on an earlier step
  ```
  A != x    // triggers once A differs from the value snapshotted below
  x := A    // snapshot this step's A for comparison on the next step
  ```

Expressions are compiled to bytecode once, at load time, and evaluated
efficiently on every step, making it practical to run many watchpoints
simultaneously.

Build and run the debugger from `debugger/src-tauri` with the
[Tauri CLI](https://tauri.app/develop/) (`cargo tauri dev` for development,
`cargo tauri build` for a packaged release); this drives an `npm run build`
of the `debugger/frontend` React/TypeScript UI automatically.
