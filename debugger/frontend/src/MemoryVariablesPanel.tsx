import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useExecutionContext } from "./ExecutionContext";
import { formatDataRadix } from "./RadixControl";
import { usePanelHeaderAction } from "./layout/panelHeaderActions";
import SelectPopover from "./SelectPopover";
import "./styles/memory-variables.scss";

/**
 * Mirrors `memory_variables::VariableType`'s snake_case serde tags — storage
 * width only. Deliberately carries no signed/unsigned distinction: these
 * variables are never used in an evaluation context where a scalar type's
 * signedness would matter, so signed-vs-unsigned is purely a display choice
 * (see `MvRadix`), not a data-type one.
 */
type VariableType = "byte" | "word" | "dword";

/**
 * Display format for a memory variable's value. Distinct from the shared
 * `DataRadix` (`RadixControl.tsx`, used by registers/stack/etc.): no
 * octal/binary (not offered here), but adds `bool`/`char` — interpreting the
 * raw bytes as a boolean or character, independent of `VariableType`'s
 * width, in place of what used to be fixed-format *types*.
 */
type MvRadix = "hex" | "udec" | "sdec" | "bool" | "char";

/** One row of the panel, as returned by `get_memory_variables`/`edit_memory_variable`. */
interface MemoryVariableRow {
  name: string;
  data_type: VariableType;
  radix: MvRadix;
  address: number | null;
  source: string | null;
  value: number | null;
}

/** Short labels for the table's Type column. */
const TYPE_LABEL: Record<VariableType, string> = {
  byte: "Byte",
  word: "Word",
  dword: "DWord",
};

const TYPE_WIDTH_BITS: Record<VariableType, number> = {
  byte: 8,
  word: 16,
  dword: 32,
};

/** Labels for the Data Type dropdown — spells out "Double Word" where the table column's `TYPE_LABEL` abbreviates. */
const TYPE_OPTION_LABEL: Record<VariableType, string> = {
  byte: "Byte",
  word: "Word",
  dword: "Double Word",
};

const TYPE_OPTIONS: { value: VariableType; label: string }[] = (
  Object.keys(TYPE_OPTION_LABEL) as VariableType[]
).map((value) => ({ value, label: TYPE_OPTION_LABEL[value] }));

/** The fixed 5-way display-format cycle, the same for every variable regardless of its `VariableType`. */
const MV_RADIX_CYCLE: MvRadix[] = ["hex", "udec", "sdec", "bool", "char"];

const RADIX_LABEL: Record<MvRadix, string> = {
  hex: "Hexadecimal",
  udec: "Unsigned Decimal",
  sdec: "Signed Decimal",
  bool: "Boolean",
  char: "Character",
};

const RADIX_OPTIONS: { value: MvRadix; label: string }[] = MV_RADIX_CYCLE.map((value) => ({
  value,
  label: RADIX_LABEL[value],
}));

/** Short glyphs for the row's radix-cycle button — mirrors `RadixControl.tsx`'s `DATA_RADIX_LABEL` convention but covers this panel's own 5-way set. */
const MV_RADIX_BUTTON_LABEL: Record<MvRadix, string> = {
  hex: "HEX",
  udec: "DEC",
  sdec: "±DEC",
  bool: "BOOL",
  char: "CHAR",
};

function formatAddr(addr: number): string {
  return addr.toString(16).toUpperCase().padStart(4, "0");
}

/** Address field display for an already-resolved name: the symbol's live address, or blank if unknown. */
function formatAddrOrBlank(addr: number | null): string {
  return addr !== null ? formatAddr(addr) : "";
}

const HEX_DIGITS = /^[0-9a-fA-F]+$/;

/**
 * Parses an address input into a 16-bit address, accepting an optional `$`
 * or `0x` hex prefix (unprefixed text is also parsed as hex, matching
 * `BreakpointPanel.tsx`/`DisassemblyPanel.tsx`'s address inputs). Returns
 * null if the text doesn't parse as a value in 0..0xFFFF.
 */
function parseAddressInput(raw: string): number | null {
  const s = raw.trim();
  const body = s.startsWith("$") ? s.slice(1) : /^0x/i.test(s) ? s.slice(2) : s;
  if (!HEX_DIGITS.test(body)) return null;
  const n = parseInt(body, 16);
  return n >= 0 && n <= 0xffff ? n : null;
}

/** Quoted printable character (0x20-0x7E is printable ASCII), or the full raw value in hexadecimal otherwise. */
function formatChar(value: number, widthBits: number): string {
  return value >= 0x20 && value <= 0x7e
    ? `'${String.fromCharCode(value)}'`
    : formatDataRadix(value, "hex", widthBits);
}

function formatValue(row: MemoryVariableRow): string {
  if (row.value === null) return "undefined";
  switch (row.radix) {
    case "bool":
      return row.value !== 0 ? "true" : "false";
    case "char":
      return formatChar(row.value, TYPE_WIDTH_BITS[row.data_type]);
    default:
      return formatDataRadix(row.value, row.radix, TYPE_WIDTH_BITS[row.data_type]);
  }
}

/** Shared fields backing both the Add and Edit popovers. */
interface VariableFormState {
  name: string;
  /** Controlled value of the address input; only consulted when `name` doesn't currently resolve. */
  address: string;
  dataType: VariableType;
  radix: MvRadix;
  /** Validation or backend error; empty string means no error. */
  error: string;
}

/** State for the add-variable popover; null means closed. */
type AddDialogState = VariableFormState;

/** State for the edit-variable popover; null means closed. */
interface EditDialogState extends VariableFormState {
  /** The variable's name before this edit, identifying which definition to update. */
  originalName: string;
}

const MAX_NAME_SUGGESTIONS = 8;

/** Idle time after the last keystroke before the suggestion list pops up, so typing a name the user already knows isn't interrupted mid-keystroke. */
const NAME_SUGGESTION_DELAY_MS = 400;

interface NameAutocompleteProps {
  id: string;
  value: string;
  invalid: boolean;
  suggestions: string[];
  onChange: (value: string) => void;
  /** Enter with the suggestion list closed: commit the dialog. */
  onCommit: () => void;
  /** Escape with the suggestion list closed: dismiss the dialog. */
  onCancel: () => void;
}

/**
 * A plain-text input with a hand-rendered suggestion dropdown, filtered by
 * substring against `suggestions` — deliberately NOT a native `<input
 * list="...">` (HTML `<datalist>`): that renders as the same unstyleable,
 * unreliable native list chrome under WebKitGTK that `SelectPopover`'s doc
 * comment already documents for `<select>`, and here it went further and
 * wedged the whole app (a stuck native popup/grab), per issue #646 unit 4
 * UAT. Follows `SelectPopover`/`ColorPickerPopover`'s established
 * click-outside/Escape popover pattern instead.
 */
function NameAutocomplete({
  id,
  value,
  invalid,
  suggestions,
  onChange,
  onCommit,
  onCancel,
}: NameAutocompleteProps) {
  const [open, setOpen] = useState(false);
  // -1 means no suggestion is keyboard-highlighted.
  const [highlightedIndex, setHighlightedIndex] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);
  const openTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelPendingOpen = useCallback(() => {
    if (openTimerRef.current !== null) {
      clearTimeout(openTimerRef.current);
      openTimerRef.current = null;
    }
  }, []);

  // Clears the debounce timer on unmount (dialog closed mid-delay).
  useEffect(() => () => cancelPendingOpen(), [cancelPendingOpen]);

  // Drop any stale keyboard highlight once the list closes (for any reason).
  useEffect(() => {
    if (!open) setHighlightedIndex(-1);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const needle = value.trim().toLowerCase();
  // Prefix match (not substring): a symbol is a candidate completion only
  // when the typed text is the start of its name. Exact matches (including
  // single-character symbol names, where the first keystroke already is an
  // exact match) are kept rather than excluded, so those names still appear.
  const matches = needle
    ? suggestions
        .filter((s) => s.toLowerCase().startsWith(needle))
        .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()))
        .slice(0, MAX_NAME_SUGGESTIONS)
    : [];

  return (
    <div className="mv-name-combo" ref={containerRef}>
      <input
        id={id}
        className={`mv-add-input${invalid ? " invalid" : ""}`}
        autoFocus
        spellCheck={false}
        autoComplete="off"
        role="combobox"
        aria-expanded={open && matches.length > 0}
        aria-autocomplete="list"
        value={value}
        onChange={(e) => {
          const next = e.target.value;
          onChange(next);
          cancelPendingOpen();
          setHighlightedIndex(-1);
          if (!next.trim()) {
            setOpen(false);
            return;
          }
          // Debounced: only pop the list once the user pauses, not on every keystroke.
          openTimerRef.current = setTimeout(() => setOpen(true), NAME_SUGGESTION_DELAY_MS);
        }}
        onBlur={() => {
          cancelPendingOpen();
          setOpen(false);
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "ArrowDown" && matches.length > 0) {
            e.preventDefault();
            cancelPendingOpen();
            if (!open) {
              setOpen(true);
              setHighlightedIndex(0);
            } else {
              setHighlightedIndex((i) => (i + 1) % matches.length);
            }
            return;
          }
          if (e.key === "ArrowUp" && matches.length > 0) {
            e.preventDefault();
            cancelPendingOpen();
            if (!open) {
              setOpen(true);
              setHighlightedIndex(matches.length - 1);
            } else {
              setHighlightedIndex((i) => (i <= 0 ? matches.length - 1 : i - 1));
            }
            return;
          }
          if (e.key === "Enter") {
            cancelPendingOpen();
            e.preventDefault();
            if (open && highlightedIndex >= 0 && highlightedIndex < matches.length) {
              onChange(matches[highlightedIndex]);
              setOpen(false);
              return;
            }
            setOpen(false);
            onCommit();
          }
          if (e.key === "Escape") {
            cancelPendingOpen();
            e.preventDefault();
            if (open) setOpen(false);
            else onCancel();
          }
        }}
      />
      {open && matches.length > 0 && (
        <div className="mv-name-suggestions" role="listbox">
          {matches.map((s, i) => (
            <button
              key={s}
              type="button"
              className={`mv-name-suggestion${i === highlightedIndex ? " highlighted" : ""}`}
              role="option"
              aria-selected={i === highlightedIndex}
              // Keeps arrow-key and mouse highlighting in sync with each other.
              onMouseEnter={() => setHighlightedIndex(i)}
              // mousedown (not click) fires before the input's blur, so the
              // click-outside handler above never gets a chance to close
              // this list out from under the selection.
              onMouseDown={(e) => {
                e.preventDefault();
                cancelPendingOpen();
                onChange(s);
                setOpen(false);
              }}
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export default function MemoryVariablesPanel() {
  const { execState } = useExecutionContext();
  const panelRef = useRef<HTMLDivElement>(null);
  const [rows, setRows] = useState<MemoryVariableRow[] | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [addDialog, setAddDialog] = useState<AddDialogState | null>(null);
  const [editDialog, setEditDialog] = useState<EditDialogState | null>(null);
  const [symbols, setSymbols] = useState<{ name: string; address: number }[]>([]);
  const symbolNames = useMemo(() => symbols.map((s) => s.name), [symbols]);

  const canEdit = execState === "stopped";

  const fetchRows = useCallback(() => {
    invoke<MemoryVariableRow[]>("get_memory_variables")
      .then(setRows)
      .catch((e) => console.error("get_memory_variables failed:", e));
  }, []);

  useEffect(() => {
    fetchRows();
  }, [fetchRows]);

  // Refreshes on: this panel's own add/edit/remove (memory-variables-changed),
  // a value write through the Memory panel or a register/memory edit
  // (memory-modified), a rename that touched the live symbol table
  // (symbols-changed), and every step/free-run tick (debugger-halted /
  // debugger-running-tick) — matching StackPanel.tsx's push-driven free-run
  // refresh rather than a poll timer.
  useEffect(() => {
    const unlistenVars = listen("memory-variables-changed", fetchRows);
    const unlistenMem = listen("memory-modified", fetchRows);
    const unlistenSymbols = listen("symbols-changed", fetchRows);
    const unlistenHalted = listen("debugger-halted", fetchRows);
    const unlistenTick = listen("debugger-running-tick", fetchRows);
    return () => {
      unlistenVars.then((f) => f());
      unlistenMem.then((f) => f());
      unlistenSymbols.then((f) => f());
      unlistenHalted.then((f) => f());
      unlistenTick.then((f) => f());
    };
  }, [fetchRows]);

  /**
   * Cycles a row's radix and persists the choice via
   * `set_memory_variable_radix` — a dedicated command (rather than
   * `edit_memory_variable`) because it doesn't need a live CPU/symbol table,
   * so it keeps working while the CPU is free-running.
   */
  const cycleRadix = useCallback((row: MemoryVariableRow) => {
    const nextRadix =
      MV_RADIX_CYCLE[(MV_RADIX_CYCLE.indexOf(row.radix) + 1) % MV_RADIX_CYCLE.length];
    invoke<MemoryVariableRow[]>("set_memory_variable_radix", {
      change: { name: row.name, radix: nextRadix },
    })
      .then(setRows)
      .catch((e) => console.error("set_memory_variable_radix failed:", e));
  }, []);

  /** Refreshes the symbol list backing the Name field's autocomplete and address lookup. */
  const fetchSymbolNames = useCallback(() => {
    invoke<{ name: string; address: number }[]>("get_symbols")
      .then((rows) => setSymbols(rows.map((s) => ({ name: s.name, address: s.address }))))
      .catch((e) => console.error("get_symbols failed:", e));
  }, []);

  /** True when `name` already resolves to a live symbol (so the address field is derived, not entered). */
  const nameResolves = useCallback(
    (name: string) => symbols.some((s) => s.name === name.trim()),
    [symbols],
  );

  /** The live address backing an already-resolved name, for display in the disabled Address field. */
  const resolvedAddress = useCallback(
    (name: string) => symbols.find((s) => s.name === name.trim())?.address ?? null,
    [symbols],
  );

  /** Click on a row selects it. */
  const handleRowClick = useCallback((name: string) => {
    setSelectedName(name);
  }, []);

  /** Clears the row-selection highlight once the user's focus moves outside the panel. */
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setSelectedName(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  /** Removes the named variable and clears its selection. */
  const removeVariableAt = useCallback(async (name: string) => {
    try {
      const result = await invoke<MemoryVariableRow[]>("remove_memory_variable", { name });
      setRows(result);
      setSelectedName((prev) => (prev === name ? null : prev));
    } catch (e) {
      console.error("remove_memory_variable failed:", e);
    }
  }, []);

  /** Delete key removes the selected variable while the CPU is stopped. */
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (document.activeElement instanceof HTMLInputElement) return;
      if (!canEdit || addDialog || editDialog || selectedName === null) return;
      if (e.key === "Delete") {
        e.preventDefault();
        removeVariableAt(selectedName);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [canEdit, addDialog, editDialog, selectedName, removeVariableAt]);

  const openAddDialog = useCallback(() => {
    if (!canEdit) return;
    fetchSymbolNames();
    setAddDialog({ name: "", address: "", dataType: "byte", radix: "hex", error: "" });
  }, [canEdit, fetchSymbolNames]);

  /** Double-click on a row (outside the Value cell, reserved for inline value editing) opens the edit popover. */
  const openEditDialog = useCallback(
    (row: MemoryVariableRow) => {
      if (!canEdit) return;
      fetchSymbolNames();
      setEditDialog({
        originalName: row.name,
        name: row.name,
        address: row.address !== null ? formatAddr(row.address) : "",
        dataType: row.data_type,
        radix: row.radix,
        error: "",
      });
    },
    [canEdit, fetchSymbolNames],
  );

  /** Validates the input, invokes add_memory_variable, and closes the popover on success. */
  const commitAddVariable = useCallback(async () => {
    if (!addDialog) return;
    const name = addDialog.name.trim();
    if (!name) {
      setAddDialog((d) => d && { ...d, error: "Enter a name" });
      return;
    }
    let address: number | null = null;
    if (!nameResolves(name)) {
      address = parseAddressInput(addDialog.address);
      if (address === null) {
        setAddDialog((d) => d && { ...d, error: "Enter a valid address" });
        return;
      }
    }
    try {
      const result = await invoke<MemoryVariableRow[]>("add_memory_variable", {
        fields: { name, address, data_type: addDialog.dataType, radix: addDialog.radix },
      });
      setRows(result);
      setAddDialog(null);
    } catch (e) {
      setAddDialog((d) => d && { ...d, error: String(e) });
    }
  }, [addDialog, nameResolves]);

  /** Dismiss the add popover on Escape while it is open. */
  useEffect(() => {
    if (!addDialog) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setAddDialog(null);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [addDialog]);

  /** Validates the input, invokes edit_memory_variable, and closes the popover on success. */
  const commitEditVariable = useCallback(async () => {
    if (!editDialog) return;
    const name = editDialog.name.trim();
    if (!name) {
      setEditDialog((d) => d && { ...d, error: "Enter a name" });
      return;
    }
    let address: number | null = null;
    if (!nameResolves(name)) {
      address = parseAddressInput(editDialog.address);
      if (address === null) {
        setEditDialog((d) => d && { ...d, error: "Enter a valid address" });
        return;
      }
    }
    try {
      const result = await invoke<MemoryVariableRow[]>("edit_memory_variable", {
        oldName: editDialog.originalName,
        fields: { name, address, data_type: editDialog.dataType, radix: editDialog.radix },
      });
      setRows(result);
      setEditDialog(null);
    } catch (e) {
      setEditDialog((d) => d && { ...d, error: String(e) });
    }
  }, [editDialog, nameResolves]);

  /** Dismiss the edit popover on Escape while it is open. */
  useEffect(() => {
    if (!editDialog) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setEditDialog(null);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [editDialog]);

  usePanelHeaderAction("memory-variables", {
    title: "Add memory variable",
    onClick: openAddDialog,
    disabled: !canEdit,
    disabledTitle: "Stop the CPU to edit memory variables",
  });

  return (
    <div className="memory-variables-panel" ref={panelRef}>
      {rows === null ? (
        <span className="mv-empty">Waiting…</span>
      ) : rows.length === 0 ? (
        <span className="mv-empty">No memory variables</span>
      ) : (
        <div className="mv-table">
          <div className="mv-header-row">
            <span className="mv-col-name">Name</span>
            <span className="mv-col-type">Type</span>
            <span className="mv-col-address">Address</span>
            <span className="mv-col-radix">Radix</span>
            <span className="mv-col-value">Value</span>
          </div>
          <div className="mv-body">
            {rows.map((row) => (
              <div
                key={row.name}
                className={`mv-row${selectedName === row.name ? " selected" : ""}`}
                onClick={() => handleRowClick(row.name)}
                onDoubleClick={(e) => {
                  // Everywhere in the row opens Edit except the radix cycle
                  // button and the remove button, which have their own click
                  // behavior that a double-click would otherwise clobber.
                  const target = e.target as Element;
                  if (target.closest(".radix-btn") || target.closest(".mv-remove-btn")) return;
                  openEditDialog(row);
                }}
              >
                <span className="mv-col-name" title={row.name}>
                  {row.name}
                </span>
                <span className="mv-col-type">{TYPE_LABEL[row.data_type]}</span>
                <span className="mv-col-address">
                  {row.address !== null ? formatAddr(row.address) : "—"}
                </span>
                <span className="mv-col-radix">
                  {/* No control for an unresolved name: it has no value to apply a radix to (always renders "undefined"). */}
                  {row.address !== null && (
                    <button
                      className="radix-btn"
                      onClick={() => cycleRadix(row)}
                      title="Cycle radix"
                    >
                      {MV_RADIX_BUTTON_LABEL[row.radix]}
                    </button>
                  )}
                </span>
                <span className={`mv-col-value${row.value === null ? " mv-undefined" : ""}`}>
                  {formatValue(row)}
                </span>
                {canEdit && (
                  <button
                    className="mv-remove-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeVariableAt(row.name);
                    }}
                    title="Remove variable"
                  >
                    ×
                  </button>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {addDialog && (
        <div className="mv-add-backdrop" onClick={() => setAddDialog(null)}>
          <div className="mv-add-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="mv-add-title">Add Memory Variable</div>

            <div className="mv-add-field">
              <label className="modal-label" htmlFor="mv-add-name">
                Name
              </label>
              <NameAutocomplete
                id="mv-add-name"
                invalid={!!addDialog.error}
                suggestions={symbolNames}
                value={addDialog.name}
                onChange={(name) => setAddDialog((d) => d && { ...d, name, error: "" })}
                onCommit={commitAddVariable}
                onCancel={() => setAddDialog(null)}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label" htmlFor="mv-add-address">
                Address
              </label>
              <input
                id="mv-add-address"
                className={`mv-add-input${addDialog.error ? " invalid" : ""}`}
                spellCheck={false}
                placeholder="e.g. $0200"
                disabled={nameResolves(addDialog.name)}
                value={
                  nameResolves(addDialog.name)
                    ? formatAddrOrBlank(resolvedAddress(addDialog.name))
                    : addDialog.address
                }
                onChange={(e) =>
                  setAddDialog((d) => d && { ...d, address: e.target.value, error: "" })
                }
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitAddVariable();
                  }
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setAddDialog(null);
                  }
                }}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label">Data Type</label>
              <SelectPopover<VariableType>
                label="Data type"
                value={addDialog.dataType}
                options={TYPE_OPTIONS}
                onChange={(dataType) => setAddDialog((d) => d && { ...d, dataType, error: "" })}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label">Radix</label>
              <SelectPopover<MvRadix>
                label="Radix"
                value={addDialog.radix}
                options={RADIX_OPTIONS}
                onChange={(radix) => setAddDialog((d) => d && { ...d, radix })}
              />
            </div>

            {addDialog.error && <div className="mv-add-error">{addDialog.error}</div>}

            <div className="mv-add-buttons">
              <button
                className="mv-add-btn-action mv-add-btn-cancel"
                onClick={() => setAddDialog(null)}
              >
                Cancel
              </button>
              <button className="mv-add-btn-action mv-add-btn-ok" onClick={commitAddVariable}>
                OK
              </button>
            </div>
          </div>
        </div>
      )}

      {editDialog && (
        <div className="mv-add-backdrop" onClick={() => setEditDialog(null)}>
          <div className="mv-add-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="mv-add-title">Edit Memory Variable</div>

            <div className="mv-add-field">
              <label className="modal-label" htmlFor="mv-edit-name">
                Name
              </label>
              <NameAutocomplete
                id="mv-edit-name"
                invalid={!!editDialog.error}
                suggestions={symbolNames}
                value={editDialog.name}
                onChange={(name) => setEditDialog((d) => d && { ...d, name, error: "" })}
                onCommit={commitEditVariable}
                onCancel={() => setEditDialog(null)}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label" htmlFor="mv-edit-address">
                Address
              </label>
              <input
                id="mv-edit-address"
                className={`mv-add-input${editDialog.error ? " invalid" : ""}`}
                spellCheck={false}
                placeholder="e.g. $0200"
                disabled={nameResolves(editDialog.name)}
                value={
                  nameResolves(editDialog.name)
                    ? formatAddrOrBlank(resolvedAddress(editDialog.name))
                    : editDialog.address
                }
                onChange={(e) =>
                  setEditDialog((d) => d && { ...d, address: e.target.value, error: "" })
                }
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitEditVariable();
                  }
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setEditDialog(null);
                  }
                }}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label">Data Type</label>
              <SelectPopover<VariableType>
                label="Data type"
                value={editDialog.dataType}
                options={TYPE_OPTIONS}
                onChange={(dataType) => setEditDialog((d) => d && { ...d, dataType, error: "" })}
              />
            </div>

            <div className="mv-add-field">
              <label className="modal-label">Radix</label>
              <SelectPopover<MvRadix>
                label="Radix"
                value={editDialog.radix}
                options={RADIX_OPTIONS}
                onChange={(radix) => setEditDialog((d) => d && { ...d, radix })}
              />
            </div>

            {editDialog.error && <div className="mv-add-error">{editDialog.error}</div>}

            <div className="mv-add-buttons">
              <button
                className="mv-add-btn-action mv-add-btn-cancel"
                onClick={() => setEditDialog(null)}
              >
                Cancel
              </button>
              <button
                className="mv-add-btn-action mv-add-btn-ok"
                onClick={commitEditVariable}
                disabled={!!editDialog.error}
                title={editDialog.error ? "Fix the error before saving" : "Save changes"}
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
