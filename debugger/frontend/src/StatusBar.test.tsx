import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CpuDescriptionProvider } from "./CpuDescriptionContext";
import { ExecutionProvider } from "./ExecutionContext";
import StatusBar, { formatCycles, splitSpeed, stoppedLabel } from "./StatusBar";
import { DESCRIPTION_6502 } from "./test/cpuFixtures";
import { invoke, resetTauriMocks } from "./test/tauriMock";

describe("splitSpeed", () => {
  it("splits a value and unit separated by a space", () => {
    expect(splitSpeed("1.8432 MHz")).toEqual(["1.8432", "MHz"]);
  });

  it("splits only on the last space, keeping multi-word values intact", () => {
    expect(splitSpeed("1 2 MHz")).toEqual(["1 2", "MHz"]);
  });

  it("returns the whole string as the value with an empty unit when there's no space", () => {
    expect(splitSpeed("0")).toEqual(["0", ""]);
  });
});

describe("formatCycles", () => {
  it("formats small numbers without separators", () => {
    expect(formatCycles(42)).toBe("42");
  });

  it("inserts comma thousands separators for large numbers", () => {
    expect(formatCycles(1234567)).toBe("1,234,567");
  });

  it("formats zero", () => {
    expect(formatCycles(0)).toBe("0");
  });
});

describe("stoppedLabel", () => {
  it("names STP when the program didn't exit", () => {
    expect(stoppedLabel(null)).toBe("STP Executed");
    expect(stoppedLabel(undefined)).toBe("STP Executed");
  });

  it("shows the exit status when the program exited", () => {
    expect(stoppedLabel(0)).toBe("Exit 0");
    expect(stoppedLabel(42)).toBe("Exit 42");
  });
});

const CPU_BUS_STATE = {
  irq_active: false,
  nmi_pending: false,
  cycles: 0,
  effective_speed: "1.8432 MHz",
  is_running: false,
  cpu_stopped: false,
  cpu_waiting: false,
  exit_status: null,
};

describe("StatusBar interrupt controls", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  function renderWithLines(interruptLines: string[]) {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_cpu_description") {
        return { ...DESCRIPTION_6502, interrupt_lines: interruptLines };
      }
      if (cmd === "get_cpu_bus_state") return CPU_BUS_STATE;
      return undefined;
    });
    render(
      <CpuDescriptionProvider>
        <ExecutionProvider>
          <StatusBar />
        </ExecutionProvider>
      </CpuDescriptionProvider>,
    );
  }

  it("shows NMI and IRQ for the 6502's interrupt lines", async () => {
    renderWithLines(DESCRIPTION_6502.interrupt_lines);

    expect(await screen.findByTitle("Trigger NMI")).toBeInTheDocument();
    expect(screen.getByTitle("Assert/Release IRQ")).toBeInTheDocument();
  });

  it("leaves out the control for a line the CPU doesn't have", async () => {
    renderWithLines(["IRQ"]);

    expect(await screen.findByTitle("Assert/Release IRQ")).toBeInTheDocument();
    expect(screen.queryByTitle("Trigger NMI")).not.toBeInTheDocument();
  });
});
