import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ExecutionProvider, useExecutionContext } from "./ExecutionContext";
import { snapshot6502 as snapshot } from "./test/cpuFixtures";
import { expectHookThrows } from "./test/expectHookThrows";
import { emitMockEvent, invoke, resetTauriMocks } from "./test/tauriMock";

beforeEach(() => {
  resetTauriMocks();
});

describe("useExecutionContext", () => {
  it("throws when used outside an ExecutionProvider", () => {
    expect(expectHookThrows(() => useExecutionContext())).toThrow(
      /must be used within an ExecutionProvider/,
    );
  });
});

describe("ExecutionProvider", () => {
  it("starts with no snapshot, stopped state, and cpuStopped false", () => {
    const { result } = renderHook(() => useExecutionContext(), { wrapper: ExecutionProvider });

    expect(result.current.lastSnapshot).toBeNull();
    expect(result.current.execState).toBe("stopped");
    expect(result.current.cpuStopped).toBe(false);
  });

  it("onStep, onEdit, and onReset all update lastSnapshot (and thus cpuStopped)", () => {
    const { result } = renderHook(() => useExecutionContext(), { wrapper: ExecutionProvider });

    act(() => result.current.onStep(snapshot({ pc: 0x9000 })));
    expect(result.current.lastSnapshot?.pc).toBe(0x9000);
    expect(result.current.cpuStopped).toBe(false);

    act(() => result.current.onEdit(snapshot({ pc: 0x9001, a: 0x42 })));
    expect(result.current.lastSnapshot?.values.A).toBe(0x42);

    act(() => result.current.onReset(snapshot({ cpu_stopped: true })));
    expect(result.current.cpuStopped).toBe(true);
  });

  it("onExecStateChange updates execState", () => {
    const { result } = renderHook(() => useExecutionContext(), { wrapper: ExecutionProvider });

    act(() => result.current.onExecStateChange("running"));

    expect(result.current.execState).toBe("running");
  });

  it("fetches registers and updates lastSnapshot when a debugger-running-tick event arrives", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(snapshot({ pc: 0xabcd }));
    const { result } = renderHook(() => useExecutionContext(), { wrapper: ExecutionProvider });

    act(() => emitMockEvent("debugger-running-tick", undefined));

    await waitFor(() => expect(result.current.lastSnapshot?.pc).toBe(0xabcd));
    expect(invoke).toHaveBeenCalledWith("get_registers");
  });
});
