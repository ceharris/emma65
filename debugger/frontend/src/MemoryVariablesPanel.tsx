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

// --- inline value editing ---

const DEC_DIGITS = /^-?[0-9]+$/;
const SIGNED_DEC = /^[+-][0-9]+$/;

/** Parses `rest` as an integer in `base` if it matches `charset` exactly, else null. */
function parseDigits(rest: string, charset: RegExp, base: number): number | null {
  return rest.length > 0 && charset.test(rest) ? parseInt(rest, base) : null;
}

/**
 * Parses a Value-cell edit's raw text into an integer, for the `hex`/`udec`/`sdec`
 * radixes. Mirrors `RegisterPanel.tsx`'s `parseRegisterInput` (an explicit `$`/`0x`/
 * `0d`/`.`/sign prefix overrides `defaultRadix`), minus the `oct`/`bin` cases this
 * panel doesn't offer.
 */
function parseNumericValueInput(raw: string, defaultRadix: "hex" | "udec" | "sdec"): number | null {
  const s = raw.trim();
  if (s === "") return null;
  if (s.startsWith("$")) return parseDigits(s.slice(1), HEX_DIGITS, 16);
  const lower = s.toLowerCase();
  if (lower.startsWith("0x")) return parseDigits(s.slice(2), HEX_DIGITS, 16);
  if (lower.startsWith("0d")) return parseDigits(s.slice(2), DEC_DIGITS, 10);
  if (s.startsWith(".")) return parseDigits(s.slice(1), DEC_DIGITS, 10);
  if (s.startsWith("-") || s.startsWith("+")) return parseDigits(s, SIGNED_DEC, 10);
  switch (defaultRadix) {
    case "hex":
      return parseDigits(s, HEX_DIGITS, 16);
    case "udec":
    case "sdec":
      return parseDigits(s, DEC_DIGITS, 10);
  }
}

/**
 * Parses a `char`-radix Value-cell edit: either a single literal character (its
 * code unit) or a `$`/`0x`-prefixed byte value. Multi-character input (anything
 * else) is rejected.
 */
function parseCharValueInput(raw: string): number | null {
  const s = raw.trim();
  if (s.length === 1) return s.charCodeAt(0);
  if (s.startsWith("$")) return parseDigits(s.slice(1), HEX_DIGITS, 16);
  if (/^0x/i.test(s)) return parseDigits(s.slice(2), HEX_DIGITS, 16);
  return null;
}

/**
 * Validates a parsed integer against a `widthBits`-wide storage location and
 * returns its unsigned representation, or null if out of range. Accepts the
 * union of the unsigned range (0..2^widthBits-1) and the signed two's-complement
 * range (-2^(widthBits-1)..-1), so e.g. typing `-1` for a byte means 0xFF.
 *
 * Uses plain arithmetic rather than `RegisterPanel.tsx`'s bitwise
 * `toUnsignedInRange` (`1 << widthBits`/`value & max`): those operators coerce
 * to 32-bit signed integers, which silently misbehaves at `widthBits === 32`
 * (`dword` variables) — `1 << 32 === 1`, not 4294967296.
 */
function toUnsignedInRangeWide(value: number, widthBits: number): number | null {
  if (!Number.isInteger(value)) return null;
  const range = 2 ** widthBits;
  const max = range - 1;
  const min = -(range / 2);
  if (value < min || value > max) return null;
  return value >= 0 ? value : value + range;
}

/** Splits `value` into `sizeBytes` little-endian bytes for `write_memory`. */
function toLittleEndianBytes(value: number, sizeBytes: number): number[] {
  const bytes: number[] = [];
  for (let i = 0; i < sizeBytes; i++) {
    bytes.push((value >>> (i * 8)) & 0xff);
  }
  return bytes;
}

/**
 * Seeds a Value-cell edit's text input with the row's current value, formatted
 * without a radix prefix — parsed back via the row's own radix by default, the
 * same convention `RegisterPanel.tsx`'s edit fields use. Not called for `bool`
 * (a checkbox, no text) or an undefined row (nothing to edit).
 */
function formatValueEditText(row: MemoryVariableRow): string {
  if (row.value === null) return "";
  switch (row.radix) {
    case "bool":
      return "";
    case "char":
      return row.value >= 0x20 && row.value <= 0x7e
        ? String.fromCharCode(row.value)
        : `$${formatDataRadix(row.value, "hex", TYPE_WIDTH_BITS[row.data_type])}`;
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
  // Inline Value-cell editing (Unit 5): which row's value is being edited,
  // and the text-input state backing every radix except `bool` (a checkbox
  // commits immediately, with no separate text-editing state to hold).
  const [editingValueName, setEditingValueName] = useState<string | null>(null);
  const [valueEditText, setValueEditText] = useState("");
  const [valueEditInvalid, setValueEditInvalid] = useState(false);
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
      if (!canEdit || addDialog || editDialog || editingValueName !== null || selectedName === null)
        return;
      if (e.key === "Delete") {
        e.preventDefault();
        removeVariableAt(selectedName);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [canEdit, addDialog, editDialog, editingValueName, selectedName, removeVariableAt]);

  /** Opens inline editing for a defined row's Value cell (CPU stopped only). */
  const beginValueEdit = useCallback(
    (row: MemoryVariableRow) => {
      if (!canEdit || row.address === null || row.value === null) return;
      setEditingValueName(row.name);
      setValueEditText(formatValueEditText(row));
      setValueEditInvalid(false);
    },
    [canEdit],
  );

  const cancelValueEdit = useCallback(() => {
    setEditingValueName(null);
    setValueEditInvalid(false);
  }, []);

  /**
   * Writes `value` (already validated/range-checked) to `row`'s resolved
   * address via the existing `write_memory` command — per the Unit 2 design,
   * value edits never go through a memory-variables-specific backend command.
   * `write_memory`'s resulting `memory-modified` event refreshes this panel
   * via the listener already installed above, so no local row update is
   * needed here on success.
   */
  const commitValueEdit = useCallback(async (row: MemoryVariableRow, value: number) => {
    if (row.address === null) return;
    try {
      await invoke("write_memory", {
        addr: row.address,
        data: toLittleEndianBytes(value, TYPE_WIDTH_BITS[row.data_type] / 8),
        patch: false,
      });
      setEditingValueName(null);
      setValueEditInvalid(false);
    } catch (e) {
      console.error("write_memory failed:", e);
      setValueEditInvalid(true);
    }
  }, []);

  /** A `bool` row's checkbox commits immediately on toggle — no separate text-parse step. */
  const commitBoolValueEdit = useCallback(
    (row: MemoryVariableRow, checked: boolean) => {
      commitValueEdit(row, checked ? 1 : 0);
    },
    [commitValueEdit],
  );

  /** Parses `valueEditText` per `row.radix` (every radix but `bool`) and commits it. */
  const commitTextValueEdit = useCallback(
    (row: MemoryVariableRow) => {
      const widthBits = TYPE_WIDTH_BITS[row.data_type];
      let parsed: number | null;
      if (row.radix === "char") {
        parsed = parseCharValueInput(valueEditText);
      } else if (row.radix === "bool") {
        return;
      } else {
        parsed = parseNumericValueInput(valueEditText, row.radix);
      }
      const value = parsed === null ? null : toUnsignedInRangeWide(parsed, widthBits);
      if (value === null) {
        setValueEditInvalid(true);
        return;
      }
      commitValueEdit(row, value);
    },
    [valueEditText, commitValueEdit],
  );

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
                  // button and the remove button (their own click behavior
                  // that a double-click would otherwise clobber) and the
                  // Value cell, which a double-click opens for inline value
                  // editing instead (Unit 5) rather than the Edit popover.
                  const target = e.target as Element;
                  if (target.closest(".radix-btn") || target.closest(".mv-remove-btn")) return;
                  if (target.closest(".mv-col-value")) {
                    beginValueEdit(row);
                    return;
                  }
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
                <span
                  className={`mv-col-value${row.value === null ? " mv-undefined" : ""}${
                    canEdit && row.value !== null ? " mv-value-editable" : ""
                  }`}
                  title={canEdit && row.value !== null ? "Double-click to edit" : undefined}
                >
                  {editingValueName === row.name && row.value !== null ? (
                    row.radix === "bool" ? (
                      <input
                        type="checkbox"
                        className="mv-value-checkbox"
                        autoFocus
                        checked={row.value !== 0}
                        onChange={(e) => commitBoolValueEdit(row, e.target.checked)}
                        onBlur={cancelValueEdit}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === "Escape") {
                            e.preventDefault();
                            cancelValueEdit();
                          }
                        }}
                      />
                    ) : (
                      <input
                        className={`mv-value-input${valueEditInvalid ? " invalid" : ""}`}
                        autoFocus
                        spellCheck={false}
                        onFocus={(e) => e.target.select()}
                        value={valueEditText}
                        onChange={(e) => {
                          setValueEditText(e.target.value);
                          setValueEditInvalid(false);
                        }}
                        onKeyDown={(e) => {
                          e.stopPropagation();
                          if (e.key === "Enter") {
                            e.preventDefault();
                            commitTextValueEdit(row);
                          } else if (e.key === "Escape") {
                            e.preventDefault();
                            cancelValueEdit();
                          }
                        }}
                        onBlur={cancelValueEdit}
                      />
                    )
                  ) : (
                    formatValue(row)
                  )}
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
