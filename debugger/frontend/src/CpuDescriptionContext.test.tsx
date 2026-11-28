import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { flagMask, isUnusedFlag } from "./cpuDescription";
import { CpuDescriptionProvider, useCpuDescription } from "./CpuDescriptionContext";
import { DESCRIPTION_6502 } from "./test/cpuFixtures";
import { emitMockEvent, invoke, resetTauriMocks } from "./test/tauriMock";

beforeEach(() => {
  resetTauriMocks();
});

describe("CpuDescriptionProvider", () => {
  it("is null until get_cpu_description answers, then holds its description", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) =>
      cmd === "get_cpu_description" ? DESCRIPTION_6502 : undefined,
    );
    const { result } = renderHook(() => useCpuDescription(), { wrapper: CpuDescriptionProvider });

    expect(result.current).toBeNull();
    await waitFor(() => expect(result.current).toEqual(DESCRIPTION_6502));
  });

  it("replaces the description with each session-loaded event's payload", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) =>
      cmd === "get_cpu_description" ? DESCRIPTION_6502 : undefined,
    );
    const { result } = renderHook(() => useCpuDescription(), { wrapper: CpuDescriptionProvider });
    await waitFor(() => expect(result.current).not.toBeNull());

    const reloaded = { ...DESCRIPTION_6502, variant: "WDC65C02" };
    act(() => emitMockEvent("session-loaded", reloaded));

    expect(result.current?.variant).toBe("WDC65C02");
  });

  it("stays null when get_cpu_description fails", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(invoke).mockRejectedValue("CPU not ready");
    const { result } = renderHook(() => useCpuDescription(), { wrapper: CpuDescriptionProvider });

    await waitFor(() => expect(error).toHaveBeenCalled());
    expect(result.current).toBeNull();
    error.mockRestore();
  });
});

describe("flag helpers", () => {
  it("masks a flag by its bit position and recognizes the unused bit", () => {
    const [n, , unused, , , , , c] = DESCRIPTION_6502.flags;
    expect(flagMask(n)).toBe(0x80);
    expect(flagMask(c)).toBe(0x01);
    expect(isUnusedFlag(unused)).toBe(true);
    expect(isUnusedFlag(n)).toBe(false);
  });
});
