/**
 * The shape of the backend's CPU description (`debugger/src-tauri/src/cpu.rs`), and
 * helpers for reading its flags. `CpuDescriptionContext` holds the session's description.
 */

/** What a register is for, which decides where the panels show it. */
export type RegisterRole = "data" | "index" | "stack" | "pc" | "flags";

/** One register, in the order the Register panel lists it. */
export interface RegisterDescription {
  /** The register's name: its key in a snapshot's `values`, and the name `set_register` takes. */
  name: string;
  width_bits: number;
  role: RegisterRole;
}

/** One bit of the flags register. */
export interface FlagDescription {
  /** The character shown when the flag is set; `-` for an unused bit. */
  label: string;
  /** The bit's position in the flags register, 0 for the least significant. */
  bit: number;
  name: string;
}

/** What the session's CPU looks like, as the backend describes it. */
export interface CpuDescription {
  family: string;
  variant: string;
  registers: RegisterDescription[];
  /** The flags register's bits, high bit first. */
  flags: FlagDescription[];
  /** The name of the register `flags` describes. */
  flags_register: string;
  interrupt_lines: string[];
  stack: { pointer: string; page_fixed: boolean };
  byte_order: "little" | "big";
  max_instruction_len: number;
  assembler: boolean;
}

/** True for a flag bit that has no meaning, which is never marked changed or toggled. */
export function isUnusedFlag(flag: FlagDescription): boolean {
  return flag.label === "-";
}

/** The bitmask of `flag` within the flags register. */
export function flagMask(flag: FlagDescription): number {
  return 1 << flag.bit;
}
