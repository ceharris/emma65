import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DataRadix,
  DATA_RADIX_CYCLE,
  UNSIGNED_DATA_RADIX_CYCLE,
  formatDataRadix,
  RadixButton,
} from "./RadixControl";
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

function formatAddr(addr: number): string {
  return addr.toString(16).toUpperCase().padStart(4, "0");
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

export default function MemoryVariablesPanel() {
  const [rows, setRows] = useState<MemoryVariableRow[] | null>(null);

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

  /** Cycles a row's radix and persists the choice via `edit_memory_variable`. */
  const cycleRadix = useCallback((row: MemoryVariableRow) => {
    const cycle = radixCycleFor(row.data_type);
    const nextRadix = cycle[(cycle.indexOf(row.radix) + 1) % cycle.length];
    invoke<MemoryVariableRow[]>("edit_memory_variable", {
      oldName: row.name,
      fields: {
        name: row.name,
        address: row.address,
        data_type: row.data_type,
        radix: nextRadix,
      },
    })
      .then(setRows)
      .catch((e) => console.error("edit_memory_variable failed:", e));
  }, []);

  return (
    <div className="memory-variables-panel">
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
              <div key={row.name} className="mv-row">
                <span className="mv-col-name" title={row.name}>
                  {row.name}
                </span>
                <span className="mv-col-type">{TYPE_LABEL[row.data_type]}</span>
                <span className="mv-col-address">
                  {row.address !== null ? formatAddr(row.address) : "—"}
                </span>
                <span className="mv-col-radix">
                  {/* No control when there's nothing to cycle: Char/Bool have a
                      fixed display, and an unresolved name has no address to
                      persist a rebind against (edit_memory_variable requires
                      one when the name doesn't already resolve). */}
                  {isRadixType(row.data_type) && row.address !== null && (
                    <RadixButton radix={row.radix} onCycle={() => cycleRadix(row)} />
                  )}
                </span>
                <span className={`mv-col-value${row.value === null ? " mv-undefined" : ""}`}>
                  {formatValue(row)}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
