import { createContext, ReactNode, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CpuDescription } from "./cpuDescription";

const CpuDescriptionContext = createContext<CpuDescription | null>(null);

/**
 * Holds the session's CPU description: fetched on mount, and replaced by the payload of
 * each `session-loaded` event, so a profile reload can change it.
 */
export function CpuDescriptionProvider({ children }: { children: ReactNode }) {
  const [description, setDescription] = useState<CpuDescription | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<CpuDescription>("get_cpu_description")
      .then((d) => {
        if (!cancelled && d) setDescription(d);
      })
      .catch((e) => console.error("get_cpu_description failed:", e));
    const unlisten = listen<CpuDescription>("session-loaded", (event) => {
      setDescription(event.payload);
    });
    return () => {
      cancelled = true;
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <CpuDescriptionContext.Provider value={description}>{children}</CpuDescriptionContext.Provider>
  );
}

/** Returns the session's CPU description, or null until it has been fetched. */
export function useCpuDescription(): CpuDescription | null {
  return useContext(CpuDescriptionContext);
}
