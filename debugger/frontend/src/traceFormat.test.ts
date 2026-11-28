import { describe, expect, it } from "vitest";
import { formatFlags, formatRegister, traceRegisters } from "./traceFormat";
import { DESCRIPTION_6502 } from "./test/cpuFixtures";

describe("traceRegisters", () => {
  it("lists every register but the PC, in the description's order", () => {
    expect(traceRegisters(DESCRIPTION_6502).map((r) => r.name)).toEqual(["A", "X", "Y", "S", "P"]);
  });
});

describe("formatRegister", () => {
  it("pads to the register's width in hex digits", () => {
    expect(formatRegister(0x0a, 8)).toBe("0A");
    expect(formatRegister(0x0a, 16)).toBe("000A");
  });
});

describe("formatFlags", () => {
  it("shows each set flag's label, and a blank for clear flags and the unused bit", () => {
    expect(formatFlags(0xff, DESCRIPTION_6502)).toBe("NV BDIZC");
    expect(formatFlags(0x20, DESCRIPTION_6502)).toBe("        ");
    expect(formatFlags(0x83, DESCRIPTION_6502)).toBe("N     ZC");
  });
});
