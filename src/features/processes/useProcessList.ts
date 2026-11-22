import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProcessInspection } from "../../types";

export type ActionFeedback = { kind: "ok" | "err"; text: string };

// Process list controls plus termination/inspection/orphan actions shared by
// the Apps, Ports, Network and Agents surfaces. `onAfterSignal` refreshes the
// related panels once the signal is acknowledged — a delivered SIGTERM is not
// a guarantee the target already exited, so ordinary polling supplies truth.
export function useProcessList(onAfterSignal: () => void) {
  const [appSearch, setAppSearch] = useState("");
  const [appSort, setAppSort] = useState<"cpu" | "mem" | "energy">("cpu");
  const [expandedAppName, setExpandedAppName] = useState<string | null>(null);
  const [processInspections, setProcessInspections] = useState<
    Record<number, ProcessInspection>
  >({});
  const [inspectingPid, setInspectingPid] = useState<number | null>(null);
  const [actionFeedback, setActionFeedback] = useState<ActionFeedback | null>(null);

  const inspectProcess = async (pid: number) => {
    if (pid <= 0 || processInspections[pid]) return;
    setInspectingPid(pid);
    try {
      const info = await invoke<ProcessInspection>("inspect_process", { pid });
      setProcessInspections((prev) => ({ ...prev, [pid]: info }));
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    } finally {
      setInspectingPid((curr) => (curr === pid ? null : curr));
    }
  };

  const terminateProcess = async (pid: number, label: string): Promise<boolean> => {
    if (pid <= 1) {
      setActionFeedback({ kind: "err", text: `Cannot stop ${label}: invalid PID.` });
      return false;
    }
    try {
      await invoke("terminate_process", { pid });
      setActionFeedback({ kind: "ok", text: `Sent SIGTERM to ${label} (PID ${pid}).` });
      onAfterSignal();
      return true;
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
      return false;
    }
  };

  const cleanupOrphans = async () => {
    try {
      const count = await invoke<number>("cleanup_orphaned_servers");
      setActionFeedback({
        kind: "ok",
        text: `Cleaned up ${count} orphaned dev server process${count === 1 ? "" : "es"}.`,
      });
      onAfterSignal();
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    }
  };

  return {
    appSearch,
    setAppSearch,
    appSort,
    setAppSort,
    expandedAppName,
    setExpandedAppName,
    processInspections,
    inspectingPid,
    actionFeedback,
    setActionFeedback,
    inspectProcess,
    terminateProcess,
    cleanupOrphans,
  };
}
