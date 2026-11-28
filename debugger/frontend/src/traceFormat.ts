/** Formatting for the Trace panel's register and flags columns, from the CPU description. */
import { CpuDescription, flagMask, isUnusedFlag, RegisterDescription } from "./cpuDescription";

/** Formats a flags-register value as one character per flag, high bit first, with a
 *  blank for each clear flag and for each unused bit. */
export function formatFlags(value: number, description: CpuDescription): string {
  return description.flags
    .map((flag) => (!isUnusedFlag(flag) && value & flagMask(flag) ? flag.label : " "))
    .join("");
}

/** The registers shown as trace columns: every register but the PC, which the Addr
 *  column already shows. */
export function traceRegisters(description: CpuDescription): RegisterDescription[] {
  return description.registers.filter((reg) => reg.role !== "pc");
}

/** Formats a register value in hex, zero-padded to the register's width. */
export function formatRegister(value: number, widthBits: number): string {
  return value
    .toString(16)
    .toUpperCase()
    .padStart(Math.ceil(widthBits / 4), "0");
}
