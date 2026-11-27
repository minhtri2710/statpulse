import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppConnectionItem, SpeedTestResult } from "../../types";

// A failed drilldown read shows its own error row rather than an empty list.
export function useNetworkTools() {
  const [expandedNetApp, setExpandedNetApp] = useState<string | null>(null);
  const [appConnections, setAppConnections] = useState<Record<string, AppConnectionItem[]>>({});
  const [appConnectionErrors, setAppConnectionErrors] = useState<Record<string, string>>({});
  const [loadingConnApp, setLoadingConnApp] = useState<string | null>(null);
  const [speedTest, setSpeedTest] = useState<SpeedTestResult | null>(null);
  const [runningSpeedTest, setRunningSpeedTest] = useState(false);
  const [speedTestError, setSpeedTestError] = useState<string | null>(null);

  const inspectNetApp = async (processName: string) => {
    if (expandedNetApp === processName) {
      setExpandedNetApp(null);
      return;
    }
    setExpandedNetApp(processName);
    setLoadingConnApp(processName);
    try {
      const conns = await invoke<AppConnectionItem[]>("fetch_app_connections", { processName });
      setAppConnections((prev) => ({ ...prev, [processName]: conns }));
      setAppConnectionErrors((prev) => {
        const next = { ...prev };
        delete next[processName];
        return next;
      });
    } catch (err) {
      setAppConnections((prev) => ({ ...prev, [processName]: [] }));
      setAppConnectionErrors((prev) => ({ ...prev, [processName]: String(err) }));
    } finally {
      setLoadingConnApp((curr) => (curr === processName ? null : curr));
    }
  };

  const runSpeedTest = async () => {
    setRunningSpeedTest(true);
    setSpeedTestError(null);
    try {
      const result = await invoke<SpeedTestResult>("run_speed_test");
      setSpeedTest(result);
    } catch (err) {
      setSpeedTestError(String(err));
    } finally {
      setRunningSpeedTest(false);
    }
  };

  return {
    expandedNetApp,
    appConnections,
    appConnectionErrors,
    loadingConnApp,
    inspectNetApp,
    speedTest,
    runningSpeedTest,
    speedTestError,
    runSpeedTest,
  };
}
