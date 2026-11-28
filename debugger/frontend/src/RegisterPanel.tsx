import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FlagDescription, flagMask, isUnusedFlag, RegisterDescription } from "./cpuDescription";
import { useCpuDescription } from "./CpuDescriptionContext";
import { useExecutionContext } from "./ExecutionContext";
import {
  ADDR_RADIX_CYCLE,
  DataRadix,
  formatDataRadix,
  RadixButton,
  useDataRadix,
  useRadixCycle,
} from "./RadixControl";
import "./styles/registers.scss";

export interface RegisterSnapshot {
  /** Every register in the CPU description, keyed by its name. */
  values: Record<string, number>;
  /** The program counter, also in `values`. */
  pc: number;
  /** Bitmask of flags-register bits that changed on the most recent step (0 on initial load). */
  changed_flags: number;
  /** True when the CPU executed STP and is now halted. */
  cpu_stopped: boolean;
  /** True when the CPU executed WAI and is now halted awaiting an interrupt. */
  cpu_waiting: boolean;
  /** True when this snapshot resulted from hitting a breakpoint. */
  breakpoint_hit: boolean;
}

/** Returns the character in single quotes if `value` is a printable ASCII byte (0x20–0x7E), otherwise null. */
function printableAscii(value: number): string | null {
  return value >= 0x20 && value <= 0x7e ? `'${String.fromCharCode(value)}'` : null;
}

// --- register editing ---

/** The edit target while the flags row is being edited; never a register's name. */
const FLAGS_EDIT = "#flags";

/** Parses `rest` as an integer in `base` if it matches `charset` exactly, else null. */
function parseDigits(rest: string, charset: RegExp, base: number): number | null {
  return rest.length > 0 && charset.test(rest) ? parseInt(rest, base) : null;
}

const HEX_DIGITS = /^[0-9a-fA-F]+$/;
const OCT_DIGITS = /^[0-7]+$/;
const BIN_DIGITS = /^[01]+$/;
const DEC_DIGITS = /^-?[0-9]+$/;
const SIGNED_DEC = /^[+-][0-9]+$/;

/**
 * Parses a register edit field's raw text into an integer.
 *
 * An explicit prefix always overrides the register's current display radix:
 * `$`/`0x` (hex), `0o`/`0q` (octal), `0b` (binary), `0d`/`.` (decimal), or a
 * bare leading `+`/`-` (also decimal — no radix's unprefixed literal ever
 * starts with a sign, so this is unambiguous). With no prefix or sign, the
 * text is parsed in `defaultRadix` (the register's current display radix).
 * Returns null if the text doesn't parse cleanly as an integer.
 */
function parseRegisterInput(raw: string, defaultRadix: DataRadix): number | null {
  const s = raw.trim();
  if (s === "") return null;

  if (s.startsWith("$")) return parseDigits(s.slice(1), HEX_DIGITS, 16);
  const lower = s.toLowerCase();
  if (lower.startsWith("0x")) return parseDigits(s.slice(2), HEX_DIGITS, 16);
  if (lower.startsWith("0o") || lower.startsWith("0q"))
    return parseDigits(s.slice(2), OCT_DIGITS, 8);
  if (lower.startsWith("0b")) return parseDigits(s.slice(2), BIN_DIGITS, 2);
  if (lower.startsWith("0d")) return parseDigits(s.slice(2), DEC_DIGITS, 10);
  if (s.startsWith(".")) return parseDigits(s.slice(1), DEC_DIGITS, 10);
  if (s.startsWith("-") || s.startsWith("+")) return parseDigits(s, SIGNED_DEC, 10);

  switch (defaultRadix) {
    case "hex":
      return parseDigits(s, HEX_DIGITS, 16);
    case "udec":
      return parseDigits(s, DEC_DIGITS, 10);
    case "sdec":
      return parseDigits(s, DEC_DIGITS, 10);
    case "oct":
      return parseDigits(s, OCT_DIGITS, 8);
    case "bin":
      return parseDigits(s, BIN_DIGITS, 2);
  }
}

/**
 * Validates a parsed integer against a register's bit width and returns its
 * unsigned representation, or null if out of range.
 *
 * Byte/word fields (`allowSigned`) accept the union of the unsigned range
 * (0..2^width-1) and the signed two's-complement range (-2^(width-1)..-1),
 * so e.g. typing `-1` for an 8-bit register means 0xFF. PC has no signed
 * display mode, so only the unsigned range is accepted for it.
 */
function toUnsignedInRange(value: number, widthBits: number, allowSigned: boolean): number | null {
  if (!Number.isInteger(value)) return null;
  const max = (1 << widthBits) - 1;
  if (!allowSigned) {
    return value >= 0 && value <= max ? value : null;
  }
  const min = -(1 << (widthBits - 1));
  return value >= min && value <= max ? value & max : null;
}

// --- flag display ---

function FlagDisplay({
  flags,
  value,
  changed,
}: {
  flags: FlagDescription[];
  value: number;
  changed: number;
}) {
  return (
    <>
      {flags.map((flag) => {
        const mask = flagMask(flag);
        const isSet = (value & mask) !== 0;
        // An unused bit (displayed as "-") never changes meaningfully.
        const didChange = !isUnusedFlag(flag) && (changed & mask) !== 0;
        return (
          <span
            key={flag.bit}
            className={[
              "flag-char",
              isSet ? "flag-set" : "flag-clear",
              didChange ? "flag-changed" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            {isSet ? flag.label : "-"}
          </span>
        );
      })}
    </>
  );
}

/** True for the registers shown in the data group (with the data radix); the rest are
 *  shown in the address group, with the address radix. */
function isDataGroup(reg: RegisterDescription): boolean {
  return reg.role === "data" || reg.role === "index";
}

// --- component ---

export default function RegisterPanel() {
  const { lastSnapshot: snapFromParent, execState, onEdit } = useExecutionContext();
  const description = useCpuDescription();
  const [snap, setSnap] = useState<RegisterSnapshot | null>(null);
  const [dataRadix, cycleDataRadix] = useDataRadix("hex");
  const [addrRadix, cycleAddrRadix] = useRadixCycle(ADDR_RADIX_CYCLE, "hex");
  const [editingTarget, setEditingTarget] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [editInvalid, setEditInvalid] = useState(false);
  const [editFlags, setEditFlags] = useState(0);

  const isEditable = execState === "stopped";

  useEffect(() => {
    if (snapFromParent !== null) {
      setSnap(snapFromParent);
      // A fresh snapshot (e.g. from Reset) invalidates any in-progress edit.
      setEditingTarget(null);
      setEditInvalid(false);
    }
  }, [snapFromParent]);

  const fetchRegisters = useCallback(async () => {
    try {
      const result = await invoke<RegisterSnapshot>("get_registers");
      setSnap(result);
    } catch (e) {
      console.error("get_registers failed:", e);
    }
  }, []);

  useEffect(() => {
    fetchRegisters();
  }, [fetchRegisters]);

  const beginEdit = useCallback(
    (name: string, currentText: string) => {
      if (!isEditable) return;
      setEditingTarget(name);
      setEditValue(currentText);
      setEditInvalid(false);
    },
    [isEditable],
  );

  const cancelEdit = useCallback(() => {
    setEditingTarget(null);
    setEditInvalid(false);
  }, []);

  const commitEdit = useCallback(
    async (name: string, radix: DataRadix, widthBits: number, allowSigned: boolean) => {
      const parsed = parseRegisterInput(editValue, radix);
      const value = parsed === null ? null : toUnsignedInRange(parsed, widthBits, allowSigned);
      if (value === null) {
        setEditInvalid(true);
        return;
      }
      try {
        const result = await invoke<RegisterSnapshot>("set_register", { name, value });
        onEdit(result);
        setEditingTarget(null);
        setEditInvalid(false);
      } catch (e) {
        console.error("set_register failed:", e);
        setEditInvalid(true);
      }
    },
    [editValue, onEdit],
  );

  const beginFlagsEdit = useCallback(
    (currentFlags: number) => {
      if (!isEditable) return;
      setEditingTarget(FLAGS_EDIT);
      setEditFlags(currentFlags);
    },
    [isEditable],
  );

  const cancelFlagsEdit = useCallback(() => {
    setEditingTarget(null);
  }, []);

  const toggleFlagBit = useCallback((bit: number) => {
    setEditFlags((f) => f ^ bit);
  }, []);

  const flagsRegister = description?.flags_register;
  const commitFlagsEdit = useCallback(async () => {
    if (flagsRegister === undefined) return;
    try {
      const result = await invoke<RegisterSnapshot>("set_register", {
        name: flagsRegister,
        value: editFlags,
      });
      onEdit(result);
      setEditingTarget(null);
    } catch (e) {
      console.error("set_register failed:", e);
      setEditingTarget(null);
    }
  }, [editFlags, flagsRegister, onEdit]);

  /** Renders a register's value cell: an inline edit input when `name` is
   *  being edited, otherwise the formatted display text (double-clickable
   *  to start editing when the CPU is stopped). */
  const renderRegisterValue = (
    name: string,
    displayText: string,
    radix: DataRadix,
    widthBits: number,
    allowSigned: boolean,
  ) => {
    if (editingTarget === name) {
      return (
        <input
          className={`reg-edit-input${editInvalid ? " invalid" : ""}`}
          autoFocus
          onFocus={(e) => e.target.select()}
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") {
              e.preventDefault();
              commitEdit(name, radix, widthBits, allowSigned);
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancelEdit();
            }
          }}
          onBlur={cancelEdit}
        />
      );
    }
    return (
      <span
        className={isEditable ? "reg-editable" : ""}
        onDoubleClick={() => beginEdit(name, displayText)}
        title={isEditable ? "Double-click to edit" : undefined}
      >
        {displayText}
      </span>
    );
  };

  /** Renders the flags register's flag characters: individually toggleable
   *  (click a flag to flip it, except an unused "-" position) while editing,
   *  otherwise the plain display (double-clickable to start editing). */
  const renderFlags = (flags: FlagDescription[], value: number, changed: number) => {
    if (editingTarget === FLAGS_EDIT) {
      return (
        <div
          className="reg-flags-edit"
          tabIndex={0}
          autoFocus
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") {
              e.preventDefault();
              commitFlagsEdit();
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancelFlagsEdit();
            }
          }}
          onBlur={cancelFlagsEdit}
        >
          {flags.map((flag) => {
            const mask = flagMask(flag);
            const isSet = (editFlags & mask) !== 0;
            const toggleable = !isUnusedFlag(flag);
            return (
              <span
                key={flag.bit}
                className={[
                  "flag-char",
                  isSet ? "flag-set" : "flag-clear",
                  toggleable ? "flag-toggleable" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onClick={toggleable ? () => toggleFlagBit(mask) : undefined}
              >
                {isSet ? flag.label : "-"}
              </span>
            );
          })}
        </div>
      );
    }
    return (
      <span
        className={isEditable ? "reg-editable" : ""}
        onDoubleClick={() => beginFlagsEdit(value)}
        title={isEditable ? "Double-click to edit flags" : undefined}
      >
        <FlagDisplay flags={flags} value={value} changed={changed} />
      </span>
    );
  };

  /** Renders one register's row, followed by the flags row when it is the flags
   *  register. */
  const renderRegisterRows = (
    reg: RegisterDescription,
    snapshot: RegisterSnapshot,
    flags: FlagDescription[],
    radix: DataRadix,
  ) => {
    const value = snapshot.values[reg.name] ?? 0;
    const ascii = reg.role === "data" && reg.width_bits === 8 ? printableAscii(value) : null;
    const rows = [
      <tr key={reg.name}>
        <td className="reg-name">{reg.name}</td>
        <td className="reg-value">
          {renderRegisterValue(
            reg.name,
            formatDataRadix(value, radix, reg.width_bits),
            radix,
            reg.width_bits,
            // PC has no signed display mode, so it accepts only unsigned values.
            reg.role !== "pc",
          )}
          {editingTarget !== reg.name && ascii !== null && (
            <span className="reg-ascii">{ascii}</span>
          )}
        </td>
      </tr>,
    ];
    if (reg.role === "flags") {
      rows.push(
        <tr key={`${reg.name}-flags`}>
          <td className="reg-flags" colSpan={2}>
            {renderFlags(flags, value, snapshot.changed_flags)}
          </td>
        </tr>,
      );
    }
    return rows;
  };

  return (
    <div className="register-panel">
      {snap === null || description === null ? (
        <span className="registers-empty">Waiting…</span>
      ) : (
        // Two side-by-side name/value column pairs (data registers, address
        // registers + flags) rather than one long stacked list — cuts the
        // panel's needed height roughly in half, freeing dockview split
        // space for the fixed 8-row Stack panel below it.
        <div className="reg-groups">
          <table className="reg-table">
            <tbody>
              <tr className="reg-group-header">
                <td />
                <td>
                  <RadixButton radix={dataRadix} onCycle={cycleDataRadix} />
                </td>
              </tr>
              {description.registers
                .filter(isDataGroup)
                .flatMap((reg) => renderRegisterRows(reg, snap, description.flags, dataRadix))}
            </tbody>
          </table>
          <table className="reg-table">
            <tbody>
              <tr className="reg-group-header">
                <td />
                <td>
                  <RadixButton radix={addrRadix} onCycle={cycleAddrRadix} />
                </td>
              </tr>
              {description.registers
                .filter((reg) => !isDataGroup(reg))
                .flatMap((reg) => renderRegisterRows(reg, snap, description.flags, addrRadix))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
