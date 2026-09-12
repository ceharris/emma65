import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useExecutionContext } from "./ExecutionContext";
import {
  DataRadix,
  DATA_RADIX_CYCLE,
  UNSIGNED_DATA_RADIX_CYCLE,
  formatDataRadix,
  RadixButton,
} from "./RadixControl";
import { usePanelHeaderAction } from "./layout/panelHeaderActions";
import SelectPopover from "./SelectPopover";
import "./styles/memory-variables.scss";

/** Mirrors `memory_variables::VariableType`'s snake_case serde tags. */
type VariableType = "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "char" | "bool";

/** One row of the panel, as returned by `get_memory_variables`/`edit_memory_variable`. */
interface MemoryVariableRow {
  name: string;
  data_type: VariableType;
  radix: DataRadix;
  address: number | null;
  source: string | null;
  value: number | null;
}

const TYPE_LABEL: Record<VariableType, string> = {
  i8: "I8",
  u8: "U8",
  i16: "I16",
  u16: "U16",
  i32: "I32",
  u32: "U32",
  char: "Char",
  bool: "Bool",
};

const TYPE_WIDTH_BITS: Record<VariableType, number> = {
  i8: 8,
  u8: 8,
  i16: 16,
  u16: 16,
  i32: 32,
  u32: 32,
  char: 8,
  bool: 8,
};

const TYPE_OPTIONS: { value: VariableType; label: string }[] = (
  Object.keys(TYPE_LABEL) as VariableType[]
).map((value) => ({ value, label: TYPE_LABEL[value] }));

const RADIX_LABEL: Record<DataRadix, string> = {
  hex: "Hexadecimal",
  udec: "Unsigned Decimal",
  sdec: "Signed Decimal",
  oct: "Octal",
  bin: "Binary",
};

function isSignedType(dataType: VariableType): boolean {
  return dataType === "i8" || dataType === "i16" || dataType === "i32";
}

/** Char and Bool have fixed display semantics rather than a radix (plan §"Char/Bool have no radix control"). */
function isRadixType(dataType: VariableType): boolean {
  return dataType !== "char" && dataType !== "bool";
}

function radixCycleFor(dataType: VariableType): DataRadix[] {
  return isSignedType(dataType) ? DATA_RADIX_CYCLE : UNSIGNED_DATA_RADIX_CYCLE;
}

/** Options for the Radix `SelectPopover`, matching the per-row cycle for `dataType`. */
function radixOptionsFor(dataType: VariableType): { value: DataRadix; label: string }[] {
  return radixCycleFor(dataType).map((value) => ({ value, label: RADIX_LABEL[value] }));
}

function formatAddr(addr: number): string {
  return addr.toString(16).toUpperCase().padStart(4, "0");
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

/** Quoted printable character, or a `\xNN` escape for a non-printable byte (0x20-0x7E is printable ASCII). */
function formatChar(value: number): string {
  const byte = value & 0xff;
  return byte >= 0x20 && byte <= 0x7e
    ? `'${String.fromCharCode(byte)}'`
    : `\\x${byte.toString(16).toUpperCase().padStart(2, "0")}`;
}

function formatValue(row: MemoryVariableRow): string {
  if (row.value === null) return "undefined";
  switch (row.data_type) {
    case "bool":
      return row.value !== 0 ? "true" : "false";
    case "char":
      return formatChar(row.value);
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
  radix: DataRadix;
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

/** Clamps `radix` into `dataType`'s cycle when switching types (e.g. signed -> unsigned drops `sdec`). */
function applyTypeChange<T extends VariableFormState>(state: T, dataType: VariableType): T {
  const cycle = radixCycleFor(dataType);
  const radix = cycle.includes(state.radix) ? state.radix : cycle[0];
  return { ...state, dataType, radix, error: "" };
}

const MAX_NAME_SUGGESTIONS = 8;

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
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const needle = value.trim().toLowerCase();
  const matches = needle
    ? suggestions
        .filter((s) => s.toLowerCase() !== needle && s.toLowerCase().includes(needle))
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
          onChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") {
            e.preventDefault();
            setOpen(false);
            onCommit();
          }
          if (e.key === "Escape") {
            e.preventDefault();
            if (open) setOpen(false);
            else onCancel();
          }
        }}
      />
      {open && matches.length > 0 && (
        <div className="mv-name-suggestions" role="listbox">
          {matches.map((s) => (
            <button
              key={s}
              type="button"
              className="mv-name-suggestion"
              role="option"
              // mousedown (not click) fires before the input's blur, so the
              // click-outside handler above never gets a chance to close
              // this list out from under the selection.
              onMouseDown={(e) => {
                e.preventDefault();
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
  const [symbolNames, setSymbolNames] = useState<string[]>([]);

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
    const cycle = radixCycleFor(row.data_type);
    const nextRadix = cycle[(cycle.indexOf(row.radix) + 1) % cycle.length];
    invoke<MemoryVariableRow[]>("set_memory_variable_radix", {
      change: { name: row.name, radix: nextRadix },
    })
      .then(setRows)
      .catch((e) => console.error("set_memory_variable_radix failed:", e));
  }, []);

  /** Refreshes the symbol-name list backing the Name field's autocomplete. */
  const fetchSymbolNames = useCallback(() => {
    invoke<{ name: string }[]>("get_symbols")
      .then((symbols) => setSymbolNames(symbols.map((s) => s.name)))
      .catch((e) => console.error("get_symbols failed:", e));
  }, []);

  /** True when `name` already resolves to a live symbol (so no address input is needed). */
  const nameResolves = useCallback(
    (name: string) => symbolNames.includes(name.trim()),
    [symbolNames],
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
    setAddDialog({ name: "", address: "", dataType: "u8", radix: "hex", error: "" });
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
              >
                <span
                  className="mv-col-name"
                  title={row.name}
                  onDoubleClick={() => openEditDialog(row)}
                >
                  {row.name}
                </span>
                <span className="mv-col-type" onDoubleClick={() => openEditDialog(row)}>
                  {TYPE_LABEL[row.data_type]}
                </span>
                <span className="mv-col-address" onDoubleClick={() => openEditDialog(row)}>
                  {row.address !== null ? formatAddr(row.address) : "—"}
                </span>
                <span className="mv-col-radix" onDoubleClick={() => openEditDialog(row)}>
                  {/* No control when there's nothing to cycle: Char/Bool have a
                      fixed display, and an unresolved name has no value to
                      apply a radix to (it always renders "undefined"). */}
                  {isRadixType(row.data_type) && row.address !== null && (
                    <RadixButton radix={row.radix} onCycle={() => cycleRadix(row)} />
                  )}
                </span>
                <span className={`mv-col-value${row.value === null ? " mv-undefined" : ""}`}>
                  {formatValue(row)}
                </span>
                <button
                  className="mv-remove-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeVariableAt(row.name);
                  }}
                  disabled={!canEdit}
                  title={canEdit ? "Remove variable" : "Stop the CPU to edit memory variables"}
                >
                  ×
                </button>
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

            {!nameResolves(addDialog.name) && (
              <div className="mv-add-field">
                <label className="modal-label" htmlFor="mv-add-address">
                  Address
                </label>
                <input
                  id="mv-add-address"
                  className={`mv-add-input${addDialog.error ? " invalid" : ""}`}
                  spellCheck={false}
                  placeholder="e.g. $0200"
                  value={addDialog.address}
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
            )}

            <div className="mv-add-field">
              <label className="modal-label">Data Type</label>
              <SelectPopover<VariableType>
                label="Data type"
                value={addDialog.dataType}
                options={TYPE_OPTIONS}
                onChange={(dataType) => setAddDialog((d) => d && applyTypeChange(d, dataType))}
              />
            </div>

            {isRadixType(addDialog.dataType) && (
              <div className="mv-add-field">
                <label className="modal-label">Radix</label>
                <SelectPopover<DataRadix>
                  label="Radix"
                  value={addDialog.radix}
                  options={radixOptionsFor(addDialog.dataType)}
                  onChange={(radix) => setAddDialog((d) => d && { ...d, radix })}
                />
              </div>
            )}

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

            {!nameResolves(editDialog.name) && (
              <div className="mv-add-field">
                <label className="modal-label" htmlFor="mv-edit-address">
                  Address
                </label>
                <input
                  id="mv-edit-address"
                  className={`mv-add-input${editDialog.error ? " invalid" : ""}`}
                  spellCheck={false}
                  placeholder="e.g. $0200"
                  value={editDialog.address}
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
            )}

            <div className="mv-add-field">
              <label className="modal-label">Data Type</label>
              <SelectPopover<VariableType>
                label="Data type"
                value={editDialog.dataType}
                options={TYPE_OPTIONS}
                onChange={(dataType) => setEditDialog((d) => d && applyTypeChange(d, dataType))}
              />
            </div>

            {isRadixType(editDialog.dataType) && (
              <div className="mv-add-field">
                <label className="modal-label">Radix</label>
                <SelectPopover<DataRadix>
                  label="Radix"
                  value={editDialog.radix}
                  options={radixOptionsFor(editDialog.dataType)}
                  onChange={(radix) => setEditDialog((d) => d && { ...d, radix })}
                />
              </div>
            )}

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
