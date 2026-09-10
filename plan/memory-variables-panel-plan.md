# Memory Variables Panel — MVP implementation plan

Tracked by issue #646 (epic). This plan covers only the MVP scope named in
that issue's body — scalar types, add/edit/delete, value editing, profile
persistence, symbol-table integration. Arrays, structs, bit-set types, and
variable-length arrays are explicitly out of scope here; they're later
stories in the epic, each to get its own plan once this MVP has shipped and
been used for a while.

## Goal

A new dockable "Memory Variables" panel where the user defines named,
typed views onto memory locations — bound either to an existing symbol
(File/Assembler/User) or to a brand-new user-defined one — and watches their
live values refresh during step and free-run execution, the same way the
Registers panel does. Unlike Registers, each variable's display radix is set
per-variable at definition time (and independently changeable afterward),
not applied uniformly across a category.

## Background

Three existing subsystems this plan builds directly on top of:

- **`SymbolTable`** (`src/emulator/bus/symbol.rs`, redesigned in
  `plan/symbol-table-redesign-plan.md` / issue #490) already has everything
  needed to bind a variable to a name: `address_for(name)` (precedence-
  resolved: `User` > `Assembler` > `File(_)`), `insert_tagged`/`remove_tagged`
  for the `User` source, and `names_for`/`iter` for enumeration. The Symbols
  panel (`plan/symbols-panel-plan.md` / issue #489,
  `debugger/src-tauri/src/symbols.rs`, `SymbolsPanel.tsx`) is the precedent
  for snapshotting the table to the frontend and formatting a `SymbolSource`
  as a display string — this plan factors that formatting out for reuse
  rather than duplicating it.
- **Register/Stack/Watchpoint panels** establish every UI pattern this panel
  needs: `RadixControl.tsx`'s `DataRadix`/`formatDataRadix`/`RadixButton`/
  `useDataRadix` for per-value radix display and cycling;
  `RegisterPanel.tsx`'s double-click-to-edit inline value editing
  (`parseRegisterInput`/`toUnsignedInRange`, generalized here for variable
  byte widths instead of register-fixed ones); `WatchpointPanel.tsx`'s
  add/edit popover dialog and row-selection/Delete-key pattern;
  `panelHeaderActions.tsx`'s `usePanelHeaderAction` for the tab-header "add"
  button; `SelectPopover.tsx` for the Data Type and Radix pickers (a plain
  `<select>`'s open dropdown list is unstyled OS chrome under WebKitGTK, per
  [[feedback_debugger_native_controls_theming]]).
- **Profile persistence** — `debugger/src-tauri/src/breakpoints.rs`'s
  `save_breakpoints_to`/`breakpoints.json` (plain `serde_json` array,
  profile-scoped, loaded/saved via `ProfileDirState`) is the template for
  this panel's own `memory-variables.json`, rather than watchpoints.rs's
  bespoke text format (`.emw`) or a TOML file.

**Naming note (deliberate, non-obvious):** `WatchpointPanel.tsx` already
uses the word "Variables" for a completely different concept — the walrus
(`:=`) runtime variables owned by `WatchEvaluator`
(`wp-vars-*` CSS classes, "Add variables section"). To avoid confusion this
new panel and everything backing it is named **"Memory Variables"**
throughout: `MainPanelId` `"memory-variables"`, backend module
`memory_variables.rs`, events `memory-variables-changed`, persisted file
`memory-variables.json`. Never shorten it to "Variables" in UI text, event
names, or file names.

## Decisions locked in (do not revisit mid-implementation)

- **A variable definition's identity is its name**, not an opaque index —
  consistent with the symbol table it's bound to, and with how
  `breakpoints.rs` keys by address rather than index. `add`/`edit`/`remove`
  commands take `name` (edit additionally takes the *new* name/type/radix).
  Two memory-variable definitions may not share a name; the symbol table
  itself already prevents two `User`-tagged entries sharing a name.
- **Overlapping storage** (issue's explicit requirement — same address shown
  in multiple radixes) is achieved through *distinct names* that happen to
  resolve to the same address, not through allowing duplicate names. This
  falls out for free from keying by name plus the symbol table's existing
  "no shadowing, multiple names per address" behavior — no special-case code
  needed.
- **Scalar data model**:
  ```rust
  #[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum VariableType { I8, U8, I16, U16, I32, U32, Char, Bool }
  ```
  `size_bytes()` (1/1/2/2/4/4/1/1), `is_signed()` (I8/I16/I32 only). `Char`
  and `Bool` are 1-byte types with fixed display semantics (not subject to
  radix — see below); they are otherwise stored/read/written exactly like
  `U8`.
  ```rust
  #[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum Radix { Hex, UDec, SDec, Oct, Bin }
  ```
  A new Rust-side enum, string-tagged to match the frontend's existing
  `DataRadix` string literals (`"hex"|"udec"|"sdec"|"oct"|"bin"` in
  `RadixControl.tsx`) exactly, so no translation layer is needed at the
  Tauri boundary. Nothing in the backend today persists a radix choice
  (Symbols/Register/Stack panels keep radix as transient frontend state) —
  this is the first.
  ```rust
  #[derive(Clone, Debug, Serialize, Deserialize)]
  pub struct VariableDef {
      pub name: String,
      /// Last address this name was bound to. Used ONLY to re-materialize a
      /// User-sourced symbol-table entry on profile load when nothing else
      /// (File/Assembler) defines `name` — see materialize_bindings below.
      /// Live display always re-resolves through the symbol table, never
      /// this field directly.
      pub address: u16,
      pub data_type: VariableType,
      pub radix: Radix,
  }
  ```
- **Persistence**: `VariableDef` list serialized as a plain JSON array to
  `<profile_dir>/memory-variables.json`, following `breakpoints.rs`'s
  `save_breakpoints_to` pattern exactly (`serde_json::to_string_pretty`,
  `create_dir_all`, missing/unparseable file degrades to empty list — same
  as `layout.rs`'s `load_dock_layout_from`, not an error).
- **Re-materializing `User`-sourced bindings on profile load**: after the
  profile's own label files and any assembler run have populated the
  `SymbolTable` (i.e. at the same point `install_breakpoints`/watchpoint
  loading already happens in `load_or_reload_session`), a new
  `materialize_bindings(defs: &[VariableDef], table: &mut SymbolTable)` walks
  the loaded defs and calls `table.insert_tagged(&def.name, def.address,
  SymbolSource::User)` **only when** `table.address_for(&def.name).is_none()`.
  A name that a label file or the assembler already defines is left alone —
  its live resolution naturally takes over, `def.address` is stale-but-
  harmless, and the panel's "undefined" behavior in the issue (a File source
  unloading) is what makes `address_for` return `None` again on the *next*
  reload, not this function reinserting a ghost.
- **Value read**: `resolve_row` reads `data_type.size_bytes()` bytes from
  `bus.peek()` (side-effect-free — same reasoning as the disassembler and
  watchpoint gutter, not `read()`) at the resolved address, assembles them
  little-endian (matching `watch::context`'s existing `FetchByte/Word/DWord`
  semantics in `src/emulator/cpu/mod.rs`'s watch-context tests), and widens
  into a single `Option<i64>` container — big enough to hold every scalar
  type's full range (including `u32::MAX`) without a second signed/unsigned
  field. `None` means the name is currently undefined (`address_for` returned
  `None`).
- **Value write reuses the existing `write_memory` Tauri command as-is** —
  no new backend write command. The frontend already has the row's resolved
  `address` from `get_memory_variables`; it computes the little-endian byte
  vector for the edited value client-side (generalizing
  `RegisterPanel.tsx`'s `toUnsignedInRange`/`parseRegisterInput` from
  register-fixed widths to a variable's `size_bytes()`) and calls
  `write_memory(addr, bytes, patch: false)` directly. This is also why the
  panel must listen for `memory-modified` (already emitted by
  `write_memory`) to refresh — not just its own `memory-variables-changed`.
- **Renaming in `edit_memory_variable`**: if the edit changes `name` and the
  *old* name currently resolves via a live `SymbolSource::User` entry (i.e.
  this panel owns it — check with `address_for` plus a source lookup, not
  just "was it User at creation," since a File source could have since
  claimed the name), remove that stale entry with `remove_tagged(&old_name,
  &SymbolSource::User)` before inserting the new binding. Prevents silent
  User-symbol leaks on rename. Renaming to a name that already resolves
  (existing symbol) rebinds to that symbol exactly like `add` does; renaming
  to a wholly new name requires an address, exactly like `add`.
- **Source display string** for a resolved row reuses (factored into a
  shared `pub(crate)` helper, not copy-pasted) the same `"User"` /
  `"Assembler"` / `"File: <basename>"` formatting `symbols.rs` builds for
  `SymbolRow.source` — confirm during Unit 2 whether `symbols.rs` already
  exposes this as a standalone function or whether extracting one is a small
  refactor of that file.
- **Char/Bool have no radix control** — `Radix` is still stored on their
  `VariableDef` (schema stays uniform) but the frontend never shows a radix
  picker/cycle button for these two types, and always renders `Bool` as
  `true`/`false` (0 = false, anything else = true, matching the issue's "no
  storage width" phrasing loosely as still occupying 1 byte) and `Char` as
  a quoted printable character when the byte is printable ASCII
  (0x20–0x7E, reusing `RegisterPanel.tsx`'s `printableAscii` logic) or a
  `\xNN` escape otherwise.

## Verified findings (traced, not assumed)

- `debugger/src-tauri/src/breakpoints.rs:74-85` (`save_breakpoints_to`) and
  `debugger/src-tauri/src/layout.rs:59-74` (`load_dock_layout_from`/
  `save_dock_layout_to`) together are the confirmed template for a new
  profile-scoped JSON file: `create_dir_all` + `serde_json`, no bespoke text
  format, degrade-to-default on load failure.
- `src/emulator/cpu/mod.rs`'s watch-context tests (`watch_context_reads_mem_word`
  etc., lines ~2678-2723) confirm little-endian, wrapping-address multi-byte
  memory reads are already an established, tested convention in this
  codebase (via the watch VM's `FetchWord`/`FetchDWord`) — this plan's
  `resolve_row` reimplements the same semantics directly against `Bus::peek`
  rather than routing through the watch VM, since no watch-expression
  compilation is needed for a fixed address+width read.
- `debugger/src-tauri/src/memory.rs:93-117` (`write_memory`) confirmed
  side-effect-complete for reuse as this panel's value-write path: it
  already emits both `debugger-halted` (PC readout refresh) and
  `memory-modified`, and already respects the `patch` flag for ROM write
  protection — no changes needed there.
- `debugger/frontend/src/layout/panelRegistry.tsx`,
  `debugger/frontend/src/layout/DockLayout.tsx` (`DEFAULT_PANEL_POSITION`),
  and `debugger/src-tauri/src/menu.rs` (`set_*_menu_enabled` pattern) are
  confirmed as the three registration points every existing panel uses;
  Symbols panel's Unit 2 (`plan/symbols-panel-plan.md`) is the most recent,
  smallest worked example of all three being touched together.
- `debugger/frontend/src/WatchpointPanel.tsx`'s `wp-vars-*` section (lines
  16-20, 278-303) confirmed as the actual naming collision this plan works
  around — it is a *different* "variables" concept (watch-expression
  runtime variables) already live in the UI today.

## Work Units

### Unit 1 — Backend: data model + profile persistence + binding materialization

Files: `debugger/src-tauri/src/memory_variables.rs` (new), `lib.rs`
(register `mod memory_variables;`, wire `materialize_bindings` into
`load_or_reload_session` alongside the existing breakpoint/watchpoint
load calls).

- `VariableType`, `Radix`, `VariableDef` per the locked-in data model.
- `load_memory_variables_from(dir) -> Vec<VariableDef>` /
  `save_memory_variables_to(dir, &[VariableDef]) -> Result<(), String>`,
  mirroring `save_breakpoints_to`.
- `materialize_bindings(defs: &[VariableDef], table: &mut SymbolTable)` per
  the locked-in semantics (only inserts when `address_for` is currently
  `None`).
- `MemoryVariablesState(pub Mutex<Vec<VariableDef>>)` Tauri-managed state,
  loaded in `load_or_reload_session` right after `materialize_bindings` runs
  (same call site).
- Tests: round-trip save/load; save with empty list; load of a missing file
  returns empty (no error); `materialize_bindings` inserts for an unresolved
  name and is a no-op for a name a `File`/`Assembler` source already
  defines; `materialize_bindings` is idempotent (calling it twice doesn't
  duplicate or error).
- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings` clean.

### Unit 2 — Backend: Tauri commands, value resolution, events

Files: `debugger/src-tauri/src/memory_variables.rs`,
`debugger/src-tauri/src/symbols.rs` (extract the shared source-formatting
helper if not already standalone), `lib.rs` (register commands).

- `MemoryVariableRow { name, data_type, radix, address: Option<u16>,
  source: Option<String>, value: Option<i64> }` (serde `Serialize`).
- `resolve_row(def: &VariableDef, table: &SymbolTable, bus: &Bus) ->
  MemoryVariableRow` per the locked-in read semantics.
- `#[tauri::command] get_memory_variables(cpu_state) -> Result<Vec<MemoryVariableRow>, String>`.
- `#[tauri::command] add_memory_variable(name, address: Option<u16>, data_type, radix, cpu_state, memory_variables_state, profile_dir, app) -> Result<Vec<MemoryVariableRow>, String>`
  — if `address_for(&name)` already resolves, `address` is ignored (may be
  omitted from the frontend call entirely); otherwise `address` is required
  and a `SymbolSource::User` entry is inserted. Rejects a duplicate
  `VariableDef` name. Persists, emits `memory-variables-changed` and (only
  when it touched the symbol table) `symbols-changed`.
- `#[tauri::command] edit_memory_variable(old_name, new_name, address: Option<u16>, data_type, radix, ...) -> Result<Vec<MemoryVariableRow>, String>`
  per the locked-in rename semantics.
- `#[tauri::command] remove_memory_variable(name, ...) -> Result<Vec<MemoryVariableRow>, String>`
  — removes the `VariableDef` only; deliberately does **not** touch the
  symbol table (issue: "the user decides when to delete an undefined
  variable" is about the *variable*, and a bound symbol may still be useful
  elsewhere, e.g. in a watch expression — leave it in the table).
- No new write command — value edits go through the existing `write_memory`
  (Unit 5 wires the frontend to it).
- Only callable while the CPU is stopped: same `execState === "stopped"`
  gating pattern as watchpoints/breakpoints, enforced in the frontend (Units
  4/5); confirm during implementation whether any of these commands also
  need a server-side stopped check (watchpoints.rs's add/edit do not
  independently check — they rely on the frontend gate) and match that
  precedent unless a concrete problem surfaces.
- Tests: add with an existing symbol name (address ignored/omitted); add
  with a brand-new name+address (creates `User` symbol); add rejects a
  duplicate variable name; edit renames off a `User`-owned name (old entry
  removed) vs. off a `File`/`Assembler`-owned name (old entry left alone);
  edit changes type/radix only; remove leaves the symbol table untouched;
  `get_memory_variables` on a name whose source has since been cleared
  (simulate via `clear_source`) returns `address: None, source: None, value:
  None`.
- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings` clean. No frontend
  changes — backend-only, CI-verified, no UAT needed for this unit alone.

### Unit 3 — Frontend: read-only panel, registration, live refresh

Files: `debugger/frontend/src/MemoryVariablesPanel.tsx` (new),
`debugger/frontend/src/layout/panelRegistry.tsx`,
`debugger/frontend/src/layout/DockLayout.tsx`,
`debugger/src-tauri/src/menu.rs` (View menu entry),
`debugger/frontend/src/styles/memory-variables.scss` (new).

- Table columns: Name, Type, Radix (per-row `RadixButton`/`useRadixCycle`
  cycling through the type-appropriate cycle — `DATA_RADIX_CYCLE` for
  signed integer types, a 4-option no-`sdec` cycle for unsigned types
  matching `ADDR_RADIX_CYCLE`'s shape, none/disabled for Char/Bool), Address
  (hex, or "—" when undefined), Value (formatted per row's `data_type`/
  `radix`, or a muted "undefined" when `address` is `None`).
- Cycling the radix button calls `edit_memory_variable` (same name, new
  radix) to persist the choice — decide during implementation whether to
  optimistically update local state before the round-trip resolves, matching
  whatever the closest existing per-row-radix precedent does, if any exists
  by then.
- Fetches `get_memory_variables` on mount; re-fetches on
  `memory-variables-changed`, `memory-modified`, `symbols-changed`, and
  `debugger-halted`. Free-run refresh cadence: investigate how
  `MemoryPanel.tsx`/`StackPanel.tsx` currently refresh live memory values
  during free-run (poll timer vs. push channel) and match that pattern —
  this needs confirming against current code at implementation time, not
  assumed here.
- Registration: `"memory-variables"` in `MainPanelId`/`PANEL_TITLES`
  (`"Memory Variables"`)/`panelComponents`; `DEFAULT_PANEL_POSITION` entry
  (reasonable default: anchored near `symbols`, e.g.
  `{ referencePanel: "watchpoints", direction: "below" }` — confirm layout
  visually during UAT rather than treating this as load-bearing); View menu
  item in `menu.rs`'s panel array, lexically ordered per that array's
  existing convention.
- No add/edit/delete UI yet in this unit — ships a working, read-only
  panel first (same staging `plan/symbols-panel-plan.md`'s Unit 2 used).
- Manual UAT: define a `memory-variables.json` by hand in a test profile
  (or temporarily via a debug script) referencing a known symbol and a raw
  address, confirm values track memory and refresh across step/run/reset,
  confirm an undefined name renders "undefined".

### Unit 4 — Frontend: Add/Edit/Delete dialog

Files: `debugger/frontend/src/MemoryVariablesPanel.tsx`.

- Tab-header "+" action via `usePanelHeaderAction("memory-variables", ...)`,
  gated on `execState === "stopped"` — same disabled/title pattern as
  Watchpoints'.
- Add/Edit popover (backdrop + dialog, `WatchpointPanel.tsx`'s `wp-add-*`
  structure as the template, new `mv-add-*` class names to avoid colliding
  with that panel's styles): Name text input with a `<datalist>` sourced
  from `get_symbols` for autocomplete against existing symbol names; Address
  hex input, shown/enabled only when the typed name doesn't currently
  resolve (live-checked against the fetched symbol list as the user types);
  Data Type via `SelectPopover<VariableType>`; Radix via
  `SelectPopover<DataRadix>`, hidden when Data Type is Char or Bool.
  Validation errors (duplicate name, missing address for an unresolved new
  name, malformed hex address) surface inline like `wp-add-error`.
- Row selection + Delete key + a per-row "×" button remove a variable,
  mirroring `WatchpointPanel.tsx`'s `selectedIndex`/keydown-listener pattern
  exactly (keyed by `name` instead of index here).
- Double-click a row (outside the Value cell, which Unit 5 claims for value
  editing) opens the Edit popover seeded with that row's current fields.
- Manual UAT: add a variable bound to an existing symbol; add one with a
  brand-new name+address and confirm it now also appears in the Symbols
  panel tagged `User`; edit a variable's type/radix; edit-rename a
  `User`-owned variable and confirm the old symbol name disappears from the
  Symbols panel; delete a variable and confirm its symbol (if `User`-owned)
  still exists in the Symbols panel afterward; confirm every control is
  disabled while the CPU is running.

### Unit 5 — Frontend: inline value editing

Files: `debugger/frontend/src/MemoryVariablesPanel.tsx`.

- Double-click a defined row's Value cell (CPU stopped only) to edit,
  generalizing `RegisterPanel.tsx`'s `parseRegisterInput`/
  `toUnsignedInRange` from register-fixed widths to
  `data_type.size_bytes() * 8` bits and `data_type.is_signed()`; Bool edits
  via a checkbox or a two-state toggle instead of a text input; Char edits
  accept either a single literal character or a `$xx`/`0x`-prefixed byte
  value (reject multi-character input).
- Commits by computing the little-endian byte vector for the type's width
  and calling the existing `write_memory(addr, bytes, patch: false)`
  command directly — no new backend command, per the Unit 2 decision.
  `memory-modified`'s resulting event refreshes this panel via Unit 3's
  existing listener, so no separate local-state update is needed on
  success.
- Manual UAT: edit an I8/U8/I16/U16/I32/U32/Char/Bool variable each through
  a full round trip (edit value, confirm memory changed via the Memory
  panel, confirm the Memory Variables panel's own display updates);
  confirm out-of-range input is rejected with the same inline-invalid
  styling as `RegisterPanel.tsx`.

## Workflow

Five sequential units, one branch + PR each, following this repo's
established per-unit workflow ([[feedback_issue_462_workflow]]): create the
unit's branch (from `origin/main`, per
[[feedback_branch_from_origin_not_local_main]]), implement, verify per that
unit's checklist above, `cargo fmt --all`, commit, push, open a PR
referencing issue #646, then **stop and await explicit instruction** before
starting the next unit. Do not batch units into one PR.

This plan doc is itself opened as a PR (not committed straight to `main`,
per [[feedback_git_workflow]]) and should be merged before Unit 1 starts,
mirroring `plan/symbol-table-redesign-plan.md`'s and
`plan/symbols-panel-plan.md`'s precedent.
