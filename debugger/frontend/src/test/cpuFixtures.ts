/**
 * Test fixtures for the CPU description and register snapshots: the 6502's description as
 * the backend sends it (`describe_6502` in `debugger/src-tauri/src/cpu.rs`), and a helper
 * that builds a 6502 register snapshot from per-register overrides.
 */
import type { CpuDescription } from "../cpuDescription";
import type { RegisterSnapshot } from "../RegisterPanel";

export const DESCRIPTION_6502: CpuDescription = {
  family: "6502",
  variant: "65C02",
  registers: [
    { name: "A", width_bits: 8, role: "data" },
    { name: "X", width_bits: 8, role: "index" },
    { name: "Y", width_bits: 8, role: "index" },
    { name: "PC", width_bits: 16, role: "pc" },
    { name: "S", width_bits: 8, role: "stack" },
    { name: "P", width_bits: 8, role: "flags" },
  ],
  flags: [
    { label: "N", bit: 7, name: "Negative" },
    { label: "V", bit: 6, name: "Overflow" },
    { label: "-", bit: 5, name: "Unused" },
    { label: "B", bit: 4, name: "Break" },
    { label: "D", bit: 3, name: "Decimal" },
    { label: "I", bit: 2, name: "Interrupt disable" },
    { label: "Z", bit: 1, name: "Zero" },
    { label: "C", bit: 0, name: "Carry" },
  ],
  flags_register: "P",
  interrupt_lines: ["IRQ", "NMI"],
  stack: { pointer: "S", page_fixed: true },
  byte_order: "little",
  max_instruction_len: 3,
  assembler: true,
};

/** 6502 register values, any of which a test can override. */
export interface Registers6502 {
  a: number;
  x: number;
  y: number;
  s: number;
  pc: number;
  p: number;
}

/** A 6502 register snapshot: these defaults, then `overrides`, applied by register. */
export function snapshot6502(
  overrides: Partial<Registers6502> & Partial<Omit<RegisterSnapshot, "values" | "pc">> = {},
): RegisterSnapshot {
  const { a, x, y, s, pc, p, ...rest } = {
    a: 0x42,
    x: 0x01,
    y: 0xff,
    s: 0xfd,
    pc: 0x8000,
    p: 0x20,
    ...overrides,
  };
  return {
    values: { A: a, X: x, Y: y, PC: pc, S: s, P: p },
    pc,
    changed_flags: 0,
    cpu_stopped: false,
    cpu_waiting: false,
    breakpoint_hit: false,
    ...rest,
  };
}
