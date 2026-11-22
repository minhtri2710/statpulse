import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DiskBenchmarkResult, QuickSwitchState, SystemExtrasSnapshot } from "../../types";
import { useTransientValue } from "../useTransient";

type ExtrasSetter = React.Dispatch<React.SetStateAction<SystemExtrasSnapshot | null>>;

// Cache cleaning confirmation, quick switches, maintenance and the disk
// benchmark for the System view.
export function useSystemTools(
  setExtras: ExtrasSetter,
  refreshExtras: () => void,
  extras: SystemExtrasSnapshot | null,
) {
  const [cleaningCacheId, setCleaningCacheId] = useState<string | null>(null);
  const [confirmingCacheId, setConfirmingCacheId] = useState<string | null>(null);
  const [cacheMsg, setCacheMsg] = useState<string | null>(null);
  const [diskBench, setDiskBench] = useState<DiskBenchmarkResult | null>(null);
  const [runningDiskBench, setRunningDiskBench] = useState(false);
  const [diskBenchErr, setDiskBenchErr] = useState<string | null>(null);
  const [maintenanceMsg, flashMaintenanceMsg] = useTransientValue<string>(4500);
  const [quickSwitchMsg, flashQuickSwitchMsg] = useTransientValue<string>(3800);
  const [showLaunchAgents, setShowLaunchAgents] = useState(false);

  // A refreshed inventory that no longer lists the pending target drops the
  // stale confirmation instead of confirming against a vanished leaf.
  useEffect(() => {
    if (confirmingCacheId !== null && !extras?.caches.some((item) => item.id === confirmingCacheId)) {
      setConfirmingCacheId(null);
    }
  }, [confirmingCacheId, extras]);

  const runMaintenance = async (action: "flush_dns" | "purge_ram") => {
    try {
      const msg = await invoke<string>("run_system_maintenance", { action });
      flashMaintenanceMsg(msg);
      refreshExtras();
    } catch (err) {
      flashMaintenanceMsg(String(err));
    }
  };

  const runQuickSwitch = async (
    switchId:
      | "toggle_dark_mode"
      | "toggle_hidden_files"
      | "toggle_desktop_icons"
      | "clear_clipboard"
      | "empty_trash",
  ) => {
    try {
      const [nextState, msg] = await invoke<[QuickSwitchState, string]>("run_quick_switch", {
        switchId,
      });
      setExtras((prev) => (prev ? { ...prev, quick_switches: nextState } : prev));
      flashQuickSwitchMsg(msg);
      refreshExtras();
    } catch (err) {
      flashQuickSwitchMsg(String(err));
    }
  };

  const runDiskBenchmark = async () => {
    setRunningDiskBench(true);
    setDiskBenchErr(null);
    try {
      const res = await invoke<DiskBenchmarkResult>("run_disk_benchmark");
      setDiskBench(res);
    } catch (err) {
      setDiskBenchErr(String(err));
    } finally {
      setRunningDiskBench(false);
    }
  };

  const cleanCache = async (cacheId: string, label: string) => {
    setConfirmingCacheId(null);
    setCleaningCacheId(cacheId);
    setCacheMsg(null);
    try {
      const updated = await invoke<SystemExtrasSnapshot["caches"]>("clean_rebuildable_cache", {
        cacheId,
      });
      setExtras((prev) => (prev ? { ...prev, caches: updated } : prev));
      setCacheMsg(`Cleaned ${label}.`);
      refreshExtras();
    } catch (err) {
      setCacheMsg(`Error cleaning ${label}: ${String(err)}`);
    } finally {
      setCleaningCacheId(null);
    }
  };

  return {
    cleaningCacheId,
    confirmingCacheId,
    setConfirmingCacheId,
    cacheMsg,
    diskBench,
    runningDiskBench,
    diskBenchErr,
    maintenanceMsg,
    quickSwitchMsg,
    showLaunchAgents,
    setShowLaunchAgents,
    runMaintenance,
    runQuickSwitch,
    runDiskBenchmark,
    cleanCache,
  };
}
