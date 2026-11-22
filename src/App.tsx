import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AppTab, Header, NavigationRail } from "./components/Header";
import { TemperatureGauge } from "./components/TemperatureGauge";
import { TemperatureChart } from "./components/TemperatureChart";
import { CoreBreakdown } from "./components/CoreBreakdown";
import { FanRuleManager } from "./components/FanRuleManager";
import { FanCard } from "./components/FanCard";
import { BatteryCard } from "./components/BatteryCard";
import { SettingsModal } from "./components/SettingsModal";
import { DesignStudioModal } from "./components/DesignStudioModal";
import { UpdateModal } from "./components/UpdateModal";
import { checkForUpdates, prepareAutoInstall, ReleaseInfo, CURRENT_VERSION } from "./services/updater";
import {
  AppConnectionItem,
  ApplicationPreferenceChange,
  ApplicationPreferences,
  AwakeStatus,
  BatteryReading,
  ConfigurationBundle,
  DevicesSnapshot,
  DiskBenchmarkResult,
  FanReading,
  HardwareTelemetrySnapshot,
  KeepAwakeRules,
  LayoutDensity,
  ListeningPort,
  PopoverLook,
  PortHttpProbe,
  ProcessInspection,
  ProcessReading,
  QuickSwitchState,
  SpeedTestResult,
  SystemExtrasSnapshot,
  TemperatureReading,
  ThermalPolicyMode,
  ThermalPolicySettings,
  ThermalRule,
} from "./types";
import {
  Activity,
  BatteryCharging,
  Bluetooth,
  Box,
  Camera,
  Check,
  ChevronRight,
  ChevronUp,
  Coffee,
  Command,
  Copy,
  Cpu,
  ExternalLink,
  FolderOpen,
  Gauge,
  Globe,
  HardDrive,
  Mic,
  Moon,
  RefreshCw,
  RotateCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Square,
  Stethoscope,
  Terminal,
  Trash2,
  Usb,
  Volume2,
  VolumeX,
  Wifi,
  Wind,
  Zap,
} from "lucide-react";

function unavailableReason(
  availability: { status: "available" } | { status: "not_present" } | { status: "unavailable"; reason: string },
  notPresentMessage: string,
) {
  return availability.status === "unavailable" ? availability.reason : notPresentMessage;
}

interface HistorySample {
  time: number;
  cpu: number;
  gpu: number | null;
  fan1Rpm: number | null;
  fan2Rpm: number | null;
  powerWatts: number | null;
  batteryPct: number | null;
}

const POLICY_CYCLE: ThermalPolicyMode[] = ["system_auto", "quiet", "performance", "custom"];
const TAB_ORDER: AppTab[] = [
  "home",
  "fans",
  "system",
  "apps",
  "network",
  "devices",
  "volume",
  "ports",
  "agents",
  "awake",
  "design",
  "settings",
];

function sparkPoints(values: number[], minFloor?: number): string {
  if (values.length === 0) return "0,12 100,12";
  const series = values.length === 1 ? [values[0], values[0]] : values;
  const minVal = minFloor !== undefined ? Math.min(minFloor, ...series) : Math.min(...series) - 2;
  const maxVal = Math.max(...series) + 2;
  const span = Math.max(maxVal - minVal, 1);
  return series
    .map((v, i) => {
      const x = (i / Math.max(series.length - 1, 1)) * 100;
      const y = 12 - ((v - minVal) / span) * 10;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function formatTempShort(celsius: number | null, unit: "C" | "F"): string {
  if (celsius === null) return `--°${unit}`;
  const val = unit === "F" ? (celsius * 9) / 5 + 32 : celsius;
  return `${Math.round(val)}°${unit}`;
}

function formatMemory(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)}G`;
  return `${Math.round(mb)}M`;
}

function formatRate(bytesPerSec: number): string {
  if (bytesPerSec >= 1024 * 1024) return `${(bytesPerSec / (1024 * 1024)).toFixed(1)}M`;
  if (bytesPerSec >= 1024) return `${Math.round(bytesPerSec / 1024)}K`;
  return `${Math.round(bytesPerSec)}B`;
}

function formatDuration(secs: number): string {
  const hrs = Math.floor(secs / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (hrs > 0) return `${hrs}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return `${secs}s`;
}

const LONG_HISTORY_STORAGE_KEY = "statpulse.long_history.v1";

type HistoryWindow = "2m" | "1h" | "24h" | "7d";

const HISTORY_WINDOW_MS: Record<HistoryWindow, number> = {
  "2m": 2 * 60 * 1000,
  "1h": 60 * 60 * 1000,
  "24h": 24 * 60 * 60 * 1000,
  "7d": 7 * 24 * 60 * 60 * 1000,
};

function loadLongHistory(): HistorySample[] {
  try {
    const raw = localStorage.getItem(LONG_HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function App() {
  const [telemetry, setTelemetry] = useState<HardwareTelemetrySnapshot | null>(null);
  const [history, setHistory] = useState<HistorySample[]>([]);
  const [longHistory, setLongHistory] = useState<HistorySample[]>(() => loadLongHistory());
  const [historyWindow, setHistoryWindow] = useState<HistoryWindow>("2m");
  const lastLongSampleRef = useRef<number>(0);

  const [netHistory, setNetHistory] = useState<{ rx: number; tx: number }[]>([]);
  const [memHistory, setMemHistory] = useState<number[]>([]);
  const [extras, setExtras] = useState<SystemExtrasSnapshot | null>(null);
  const [devices, setDevices] = useState<DevicesSnapshot | null>(null);
  const [loadingDevices, setLoadingDevices] = useState(false);

  const [showHealthReport, setShowHealthReport] = useState(false);
  const [copiedHealthReport, setCopiedHealthReport] = useState(false);
  const [copiedMcpConfig, setCopiedMcpConfig] = useState(false);

  const [cmdOpen, setCmdOpen] = useState(false);
  const [cmdQuery, setCmdQuery] = useState("");
  const [recordingSession, setRecordingSession] = useState(false);
  const [recordedSamples, setRecordedSamples] = useState<
    { time: number; cpu: number; gpu: number | null; fan: number | null; watts: number | null; memGb: number | null }[]
  >([]);
  const [sessionExportMsg, setSessionExportMsg] = useState<string | null>(null);

  const [appSort, setAppSort] = useState<"cpu" | "mem" | "energy">("cpu");
  const [expandedNetApp, setExpandedNetApp] = useState<string | null>(null);
  const [appConnections, setAppConnections] = useState<Record<string, AppConnectionItem[]>>({});
  const [loadingConnApp, setLoadingConnApp] = useState<string | null>(null);
  const [unloadingModel, setUnloadingModel] = useState<string | null>(null);
  const [controllingCtr, setControllingCtr] = useState<string | null>(null);
  const [maintenanceMsg, setMaintenanceMsg] = useState<string | null>(null);

  const [thermalPolicy, setThermalPolicy] = useState<ThermalPolicySettings>({
    mode: "system_auto",
    rules: [],
  });
  const [activeTab, setActiveTab] = useState<AppTab>("home");

  const [appSearch, setAppSearch] = useState("");
  const [expandedAppName, setExpandedAppName] = useState<string | null>(null);
  const [processInspections, setProcessInspections] = useState<Record<number, ProcessInspection>>({});
  const [inspectingPid, setInspectingPid] = useState<number | null>(null);
  const [actionFeedback, setActionFeedback] = useState<{ kind: "ok" | "err"; text: string } | null>(
    null,
  );

  const [ports, setPorts] = useState<ListeningPort[]>([]);
  const [portSearch, setPortSearch] = useState("");
  const [copiedPort, setCopiedPort] = useState<number | null>(null);
  const [portsError, setPortsError] = useState<string | null>(null);

  const [speedTest, setSpeedTest] = useState<SpeedTestResult | null>(null);
  const [runningSpeedTest, setRunningSpeedTest] = useState(false);
  const [speedTestError, setSpeedTestError] = useState<string | null>(null);

  const [cleaningCacheId, setCleaningCacheId] = useState<string | null>(null);
  const [cacheMsg, setCacheMsg] = useState<string | null>(null);

  const [diskBench, setDiskBench] = useState<DiskBenchmarkResult | null>(null);
  const [runningDiskBench, setRunningDiskBench] = useState(false);
  const [diskBenchErr, setDiskBenchErr] = useState<string | null>(null);
  const [httpProbes, setHttpProbes] = useState<Record<number, PortHttpProbe>>({});
  const [probingPorts, setProbingPorts] = useState(false);
  const [dustBlastSecs, setDustBlastSecs] = useState<number>(0);
  const [quickSwitchMsg, setQuickSwitchMsg] = useState<string | null>(null);
  const [showLaunchAgents, setShowLaunchAgents] = useState(false);
  const [islandCollapsed, setIslandCollapsed] = useState(false);
  const [islandPinned, setIslandPinned] = useState(false);
  const [islandCollapsing, setIslandCollapsing] = useState(false);
  const hoverTimeoutRef = useRef<number | null>(null);
  const collapseAnimRef = useRef<number | null>(null);
  const justCollapsedAtRef = useRef<number>(0);
  const cmdOpenRef = useRef(false);
  const dismissIslandRef = useRef<() => void>(() => {});

  const updateIslandPinned = useCallback((next: boolean) => {
    setIslandPinned(next);
    void invoke("set_island_pinned", { pinned: next }).catch(() => {});
  }, []);

  const handleDismissIslandIntoNotch = useCallback(() => {
    if (hoverTimeoutRef.current !== null) {
      window.clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    if (collapseAnimRef.current !== null) {
      window.clearTimeout(collapseAnimRef.current);
      collapseAnimRef.current = null;
    }
    justCollapsedAtRef.current = Date.now();
    updateIslandPinned(false);
    setIslandCollapsing(true);
    collapseAnimRef.current = window.setTimeout(() => {
      setIslandCollapsing(false);
      setIslandCollapsed(true);
      void invoke("dismiss_dynamic_island").catch(() => {});
    }, 110);
  }, [updateIslandPinned]);

  useEffect(() => {
    dismissIslandRef.current = handleDismissIslandIntoNotch;
  }, [handleDismissIslandIntoNotch]);

  const [awakeStatus, setAwakeStatus] = useState<AwakeStatus>({
    keep_awake_active: false,
    remaining_seconds: null,
    assertions: [],
    rules: { while_on_ac: false, while_ports_active: false, while_ai_agents_active: false },
    auto_rule_reason: null,
  });
  const [awakeError, setAwakeError] = useState<string | null>(null);

  const [policyError, setPolicyError] = useState<string | null>(null);
  const [preferences, setPreferences] = useState<ApplicationPreferences>({
    temperature_unit: "celsius",
    telemetry_interval_ms: 1500,
    launch_at_login: false,
    menu_bar_style: "compact_text",
    traffic_light_menu_bar: false,
    popover_look: "liquid_glass",
    layout_density: "comfortable",
    chart_palette: "default",
  });
  const [preferencesError, setPreferencesError] = useState<string | null>(null);

  const [updateRelease, setUpdateRelease] = useState<ReleaseInfo | null>(null);
  const [hasUpdateAvailable, setHasUpdateAvailable] = useState<boolean>(false);
  const [showUpdateModal, setShowUpdateModal] = useState<boolean>(false);
  const [updateReady, setUpdateReady] = useState<boolean>(false);
  const [updatePreparationError, setUpdatePreparationError] = useState<string | null>(null);
  const preparingUpdateVersion = useRef<string | null>(null);
  const preparedUpdateVersion = useRef<string | null>(null);

  const prepareUpdate = (release: ReleaseInfo) => {
    setHasUpdateAvailable(true);
    setUpdateRelease(release);

    if (preparedUpdateVersion.current === release.version) {
      setUpdateReady(true);
      return;
    }
    if (preparingUpdateVersion.current === release.version) return;
    if (updatePreparationError) setUpdatePreparationError(null);

    const downloadUrl = release.downloadUrl;
    if (!downloadUrl?.split("?")[0].toLowerCase().endsWith(".dmg")) {
      setUpdatePreparationError("This release does not include a DMG installer.");
      return;
    }

    preparingUpdateVersion.current = release.version;
    setUpdateReady(false);
    setUpdatePreparationError(null);
    prepareAutoInstall(downloadUrl)
      .then(() => {
        preparedUpdateVersion.current = release.version;
        setUpdateReady(true);
        setShowUpdateModal(true);
      })
      .catch((error) => {
        setUpdatePreparationError(
          error?.message || "Failed to prepare the update in the background.",
        );
      })
      .finally(() => {
        preparingUpdateVersion.current = null;
      });
  };

  const recordSnapshot = (snapshot: HardwareTelemetrySnapshot) => {
    setTelemetry(snapshot);
    const temps = snapshot.temperatures.status === "available" ? snapshot.temperatures.value : null;
    const cpu = temps?.cpu_celsius ?? null;
    if (cpu === null) return;

    const fanList = snapshot.fans.status === "available" ? snapshot.fans.value : [];
    const bat = snapshot.battery.status === "available" ? snapshot.battery.value : null;

    const sample: HistorySample = {
      time: snapshot.captured_at_unix_ms,
      cpu,
      gpu: temps?.gpu_celsius ?? null,
      fan1Rpm: fanList[0]?.speed_rpm ?? null,
      fan2Rpm: fanList[1]?.speed_rpm ?? null,
      powerWatts: bat?.power_watts ?? null,
      batteryPct: bat?.charge_percent ?? null,
    };

    setHistory((previous) => [...previous.slice(-79), sample]);

    if (recordingSession) {
      setRecordedSamples((prev) => [
        ...prev,
        {
          time: sample.time,
          cpu,
          gpu: temps?.gpu_celsius ?? null,
          fan: fanList[0]?.speed_rpm ?? null,
          watts: bat?.power_watts ?? null,
          memGb: extras?.memory.used_gb ?? null,
        },
      ]);
    }

    if (sample.time - lastLongSampleRef.current >= 20_000) {
      lastLongSampleRef.current = sample.time;
      const cutoff7d = sample.time - HISTORY_WINDOW_MS["7d"];
      setLongHistory((prev) => {
        const next = [...prev.filter((p) => p.time >= cutoff7d), sample].slice(-500);
        try {
          localStorage.setItem(LONG_HISTORY_STORAGE_KEY, JSON.stringify(next));
        } catch {
          // Ignore storage quota issues
        }
        return next;
      });
    }
  };

  const extrasInFlightRef = useRef(false);
  const portsInFlightRef = useRef(false);
  const islandCollapsedRef = useRef(false);
  const isDynamicIslandRef = useRef(false);

  const refreshPorts = useCallback(async () => {
    if (portsInFlightRef.current) return;
    portsInFlightRef.current = true;
    try {
      const list = await invoke<ListeningPort[]>("fetch_listening_ports");
      setPorts(list);
      setPortsError(null);
    } catch (error) {
      setPortsError(String(error));
    } finally {
      portsInFlightRef.current = false;
    }
  }, []);

  const refreshAwake = useCallback(async () => {
    try {
      const status = await invoke<AwakeStatus>("fetch_awake_status");
      setAwakeStatus(status);
      setAwakeError(null);
    } catch (error) {
      setAwakeError(String(error));
    }
  }, []);

  const refreshExtras = useCallback(async () => {
    if (extrasInFlightRef.current) return;
    extrasInFlightRef.current = true;
    try {
      const snap = await invoke<SystemExtrasSnapshot>("fetch_system_extras");
      setExtras(snap);
      setNetHistory((prev) => [
        ...prev.slice(-30),
        { rx: snap.network.rx_bytes_per_sec, tx: snap.network.tx_bytes_per_sec },
      ]);
      setMemHistory((prev) => [...prev.slice(-30), snap.memory.used_gb]);
    } catch (error) {
      console.error("System extras fetch failed:", error);
    } finally {
      extrasInFlightRef.current = false;
    }
  }, []);

  const refreshPillExtras = useCallback(async () => {
    if (extrasInFlightRef.current) return;
    extrasInFlightRef.current = true;
    try {
      const snap = await invoke<SystemExtrasSnapshot>("fetch_pill_extras");
      setExtras(snap);
    } catch (error) {
      console.error("Pill extras fetch failed:", error);
    } finally {
      extrasInFlightRef.current = false;
    }
  }, []);

  const refreshDevices = useCallback(async () => {
    setLoadingDevices(true);
    try {
      const snap = await invoke<DevicesSnapshot>("fetch_devices_snapshot");
      setDevices(snap);
    } catch (error) {
      console.error("Devices fetch failed:", error);
    } finally {
      setLoadingDevices(false);
    }
  }, []);

  useEffect(() => {
    invoke<HardwareTelemetrySnapshot>("fetch_telemetry")
      .then(recordSnapshot)
      .catch((error) => console.error("Hardware telemetry snapshot fetch failed:", error));
    invoke<ApplicationPreferences>("application_preferences")
      .then((prefs) => {
        setPreferences(prefs);
        if (prefs.layout_density === "dynamic_island") {
          setIslandCollapsed(true);
          islandCollapsedRef.current = true;
          isDynamicIslandRef.current = true;
          void refreshPillExtras();
        } else {
          void refreshPorts();
          void refreshAwake();
          void refreshExtras();
          void refreshDevices();
        }
      })
      .catch((error) => console.error("Application preferences fetch failed:", error));
    invoke<ThermalPolicySettings>("thermal_policy_settings")
      .then(setThermalPolicy)
      .catch((error) => console.error("Thermal policy settings fetch failed:", error));

    checkForUpdates()
      .then((res) => {
        if (res.hasUpdate && res.latestRelease) {
          prepareUpdate(res.latestRelease);
        }
      })
      .catch(() => {});

    const unlistenTelemetry = listen<HardwareTelemetrySnapshot>("telemetry-update", (event) => {
      recordSnapshot(event.payload);
    });
    const unlistenPrefs = listen<ApplicationPreferences>("preferences-updated", (event) => {
      setPreferences(event.payload);
    });
    const unlistenPolicy = listen<ThermalPolicySettings>("thermal-policy-updated", (event) => {
      setThermalPolicy(event.payload);
    });
    const unlistenDustBlast = listen<number>("dust-blast-tick", (event) => {
      setDustBlastSecs(event.payload);
    });
    const unlistenIslandSync = listen<boolean>("island-collapsed-sync", (event) => {
      setIslandCollapsing(false);
      setIslandCollapsed(event.payload);
    });
    const unlistenIslandHideReq = listen<boolean>("island-request-hide", () => {
      if (!cmdOpenRef.current) {
        dismissIslandRef.current();
      }
    });

    const onFocusOrVisible = () => {
      if (document.hidden) return;
      if (isDynamicIslandRef.current && islandCollapsedRef.current) {
        void refreshPillExtras();
        return;
      }
      void refreshExtras();
      void refreshPorts();
      void refreshAwake();
    };
    window.addEventListener("focus", onFocusOrVisible);
    document.addEventListener("visibilitychange", onFocusOrVisible);

    const extrasInterval = window.setInterval(() => {
      if (document.hidden) return;
      if (isDynamicIslandRef.current && islandCollapsedRef.current) {
        void refreshPillExtras();
        return;
      }
      void refreshExtras();
      void refreshPorts();
      void refreshAwake();
    }, 3500);

    const devicesInterval = window.setInterval(() => {
      if (document.hidden) return;
      if (isDynamicIslandRef.current && islandCollapsedRef.current) return;
      void refreshDevices();
    }, 18000);

    return () => {
      window.removeEventListener("focus", onFocusOrVisible);
      document.removeEventListener("visibilitychange", onFocusOrVisible);
      unlistenTelemetry.then((unlisten) => unlisten());
      unlistenPrefs.then((unlisten) => unlisten());
      unlistenPolicy.then((unlisten) => unlisten());
      unlistenDustBlast.then((unlisten) => unlisten());
      unlistenIslandSync.then((unlisten) => unlisten());
      unlistenIslandHideReq.then((unlisten) => unlisten());
      window.clearInterval(extrasInterval);
      window.clearInterval(devicesInterval);
    };
  }, [refreshPorts, refreshAwake, refreshExtras, refreshPillExtras, refreshDevices]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey && !e.shiftKey && !e.altKey && (e.key.toLowerCase() === "k" || e.key.toLowerCase() === "p")) {
        e.preventDefault();
        setCmdOpen((v) => !v);
        setCmdQuery("");
        return;
      }
      if (e.key === "Escape") {
        if (cmdOpen) {
          setCmdOpen(false);
          return;
        }
        invoke("toggle_popover").catch(() => {});
        return;
      }
      if (e.metaKey && !e.shiftKey && !e.altKey) {
        const idx = Number(e.key) - 1;
        if (idx >= 0 && idx < 9 && idx < TAB_ORDER.length) {
          e.preventDefault();
          setActiveTab(TAB_ORDER[idx]);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [cmdOpen]);

  const handleEnableHelper = async () => {
    try {
      await invoke("install_fan_actuation_helper");
      const updated = await invoke<HardwareTelemetrySnapshot>("fetch_telemetry");
      recordSnapshot(updated);
    } catch (err) {
      console.error("Enable helper failed:", err);
    }
  };

  const handleSetFanSpeed = async (fanId: number, rpm: number) => {
    try {
      await invoke("set_fan_speed", { fanId, rpm });
    } catch (err) {
      console.error("Set fan speed failed:", err);
    }
  };

  const handleSetFanMode = async (fanId: number, mode: "auto" | "manual", rpm?: number) => {
    try {
      await invoke("set_fan_mode", { fanId, mode, rpm });
    } catch (err) {
      console.error("Set fan mode failed:", err);
    }
  };

  const handleHideWindow = () => {
    invoke("toggle_popover").catch(() => {});
  };

  const handleUpdatePreferences = async (change: ApplicationPreferenceChange) => {
    try {
      const updated = await invoke<ApplicationPreferences>("update_application_preferences", {
        change,
      });
      setPreferences(updated);
      setPreferencesError(null);
    } catch (error) {
      setPreferencesError(String(error));
      invoke<ApplicationPreferences>("application_preferences")
        .then(setPreferences)
        .catch(() => {});
      console.error("Application preferences update failed:", error);
    }
  };

  const handleSelectPolicyMode = async (mode: ThermalPolicyMode) => {
    try {
      const updated = await invoke<ThermalPolicySettings>("select_thermal_policy_mode", { mode });
      setThermalPolicy(updated);
      setPolicyError(null);
    } catch (error) {
      setPolicyError(String(error));
      console.error("Select Thermal policy mode failed:", error);
    }
  };

  const handleCyclePolicy = () => {
    const idx = POLICY_CYCLE.indexOf(thermalPolicy.mode);
    const next = POLICY_CYCLE[(idx + 1) % POLICY_CYCLE.length];
    void handleSelectPolicyMode(next);
  };

  const handleSaveRule = async (rule: ThermalRule) => {
    try {
      const updated = await invoke<ThermalPolicySettings>("upsert_thermal_rule", { rule });
      setThermalPolicy(updated);
      setPolicyError(null);
    } catch (error) {
      setPolicyError(String(error));
      console.error("Save Thermal rule failed:", error);
    }
  };

  const handleDeleteRule = async (ruleId: string) => {
    try {
      const updated = await invoke<ThermalPolicySettings>("delete_thermal_rule", { ruleId });
      setThermalPolicy(updated);
      setPolicyError(null);
    } catch (error) {
      setPolicyError(String(error));
      console.error("Delete Thermal rule failed:", error);
    }
  };

  const handleBundleImported = (bundle: ConfigurationBundle) => {
    setPreferences(bundle.preferences);
    setThermalPolicy(bundle.thermal_policy);
    setPreferencesError(null);
    setPolicyError(null);
  };

  const handleTerminateProcess = async (pid: number, label: string) => {
    if (pid <= 1) {
      setActionFeedback({ kind: "err", text: `Cannot stop ${label}: invalid PID.` });
      return;
    }
    try {
      await invoke("terminate_process", { pid });
      setActionFeedback({ kind: "ok", text: `Sent SIGTERM to ${label} (PID ${pid}).` });
      window.setTimeout(() => {
        void refreshPorts();
        void refreshExtras();
        invoke<HardwareTelemetrySnapshot>("fetch_telemetry")
          .then(recordSnapshot)
          .catch(() => {});
      }, 600);
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    }
  };

  const handleSetKeepAwake = async (enabled: boolean, durationSeconds: number | null) => {
    try {
      const updated = await invoke<AwakeStatus>("set_keep_awake", {
        enabled,
        durationSeconds,
      });
      setAwakeStatus(updated);
      setAwakeError(null);
    } catch (err) {
      setAwakeError(String(err));
    }
  };

  const handleToggleKeepAwakeRule = async (key: keyof KeepAwakeRules) => {
    const nextRules: KeepAwakeRules = {
      ...awakeStatus.rules,
      [key]: !awakeStatus.rules[key],
    };
    try {
      const updated = await invoke<AwakeStatus>("set_keep_awake_rules", { rules: nextRules });
      setAwakeStatus(updated);
      setAwakeError(null);
    } catch (err) {
      setAwakeError(String(err));
    }
  };

  const handleInspectProcess = async (pid: number) => {
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

  const handleCleanupOrphans = async () => {
    try {
      const count = await invoke<number>("cleanup_orphaned_servers");
      setActionFeedback({
        kind: "ok",
        text: `Cleaned up ${count} orphaned dev server process${count === 1 ? "" : "es"}.`,
      });
      window.setTimeout(() => {
        void refreshExtras();
        void refreshPorts();
      }, 450);
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    }
  };

  const handleRevealInFinder = async (path: string) => {
    try {
      await invoke("reveal_in_finder", { path });
    } catch (err) {
      setPortsError(String(err));
    }
  };

  const handleToggleRecording = async () => {
    if (!recordingSession) {
      setRecordedSamples([]);
      setSessionExportMsg(null);
      setRecordingSession(true);
      return;
    }
    setRecordingSession(false);
    if (recordedSamples.length === 0) {
      setSessionExportMsg("No telemetry samples captured.");
      return;
    }
    const maxCpu = Math.max(...recordedSamples.map((s) => s.cpu));
    const avgCpu = recordedSamples.reduce((a, s) => a + s.cpu, 0) / recordedSamples.length;
    const fans = recordedSamples.flatMap((s) => (s.fan !== null ? [s.fan] : []));
    const maxFan = fans.length > 0 ? Math.max(...fans) : 0;
    const watts = recordedSamples.flatMap((s) => (s.watts !== null ? [s.watts] : []));
    const avgWatts = watts.length > 0 ? watts.reduce((a, b) => a + b, 0) / watts.length : 0;
    const csvRows = [
      "timestamp,cpu_c,gpu_c,fan_rpm,package_watts,mem_gb",
      ...recordedSamples.map(
        (s) =>
          `${new Date(s.time).toISOString()},${s.cpu.toFixed(1)},${s.gpu?.toFixed(1) ?? ""},${s.fan ?? ""},${s.watts?.toFixed(1) ?? ""},${s.memGb?.toFixed(1) ?? ""}`,
      ),
    ].join("\n");
    const report = `# StatPulse Thermal Session (${recordedSamples.length} samples)\n- Avg CPU: ${avgCpu.toFixed(1)}°C | Peak CPU: ${maxCpu.toFixed(1)}°C | Max Fan: ${maxFan} RPM | Avg Power: ${avgWatts.toFixed(1)}W\n\n\`\`\`csv\n${csvRows}\n\`\`\``;
    try {
      await navigator.clipboard.writeText(report);
      setSessionExportMsg(`Copied ${recordedSamples.length} samples (Peak ${Math.round(maxCpu)}°C) to clipboard.`);
    } catch {
      setSessionExportMsg(`Recorded ${recordedSamples.length} samples (Peak ${Math.round(maxCpu)}°C).`);
    }
    window.setTimeout(() => setSessionExportMsg(null), 4000);
  };

  const handleUnloadOllama = async (name: string) => {
    setUnloadingModel(name);
    try {
      const remaining = await invoke<SystemExtrasSnapshot["ollama_models"]>("unload_ollama_model", {
        name,
      });
      setExtras((prev) => (prev ? { ...prev, ollama_models: remaining } : prev));
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    } finally {
      setUnloadingModel(null);
    }
  };

  const handleControlContainer = async (id: string, action: "stop" | "restart") => {
    setControllingCtr(id);
    try {
      const updated = await invoke<SystemExtrasSnapshot["containers"]>("control_container", {
        id,
        action,
      });
      setExtras((prev) => (prev ? { ...prev, containers: updated } : prev));
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    } finally {
      setControllingCtr(null);
    }
  };

  const handleInspectNetApp = async (processName: string) => {
    if (expandedNetApp === processName) {
      setExpandedNetApp(null);
      return;
    }
    setExpandedNetApp(processName);
    setLoadingConnApp(processName);
    try {
      const conns = await invoke<AppConnectionItem[]>("fetch_app_connections", { processName });
      setAppConnections((prev) => ({ ...prev, [processName]: conns }));
    } catch {
      setAppConnections((prev) => ({ ...prev, [processName]: [] }));
    } finally {
      setLoadingConnApp((curr) => (curr === processName ? null : curr));
    }
  };

  const handleRunMaintenance = async (action: "flush_dns" | "purge_ram") => {
    try {
      const msg = await invoke<string>("run_system_maintenance", { action });
      setMaintenanceMsg(msg);
      void refreshExtras();
    } catch (err) {
      setMaintenanceMsg(String(err));
    }
    window.setTimeout(() => setMaintenanceMsg(null), 4500);
  };

  const handleRunQuickSwitch = async (
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
      setQuickSwitchMsg(msg);
      void refreshExtras();
    } catch (err) {
      setQuickSwitchMsg(String(err));
    }
    window.setTimeout(() => setQuickSwitchMsg(null), 3800);
  };

  const handleProbeHttpPorts = async () => {
    const targetPorts = filteredPorts.slice(0, 20).map((p) => p.port);
    if (targetPorts.length === 0) return;
    setProbingPorts(true);
    try {
      const results = await invoke<PortHttpProbe[]>("probe_http_ports", { ports: targetPorts });
      const mapped: Record<number, PortHttpProbe> = {};
      for (const item of results) {
        mapped[item.port] = item;
      }
      setHttpProbes((prev) => ({ ...prev, ...mapped }));
    } catch (err) {
      setPortsError(String(err));
    } finally {
      setProbingPorts(false);
    }
  };

  const handleRunDiskBenchmark = async () => {
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

  const handleStartDustBlast = async () => {
    try {
      await invoke("start_fan_dust_blast");
      setDustBlastSecs(15);
      setPolicyError(null);
    } catch (err) {
      setPolicyError(String(err));
    }
  };

  const handleCancelDustBlast = async () => {
    await invoke("cancel_fan_dust_blast").catch(() => {});
    setDustBlastSecs(0);
  };

  const handleToggleIslandCollapsed = async (next?: boolean) => {
    const target = next ?? !islandCollapsed;
    if (hoverTimeoutRef.current !== null) {
      window.clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    if (collapseAnimRef.current !== null) {
      window.clearTimeout(collapseAnimRef.current);
      collapseAnimRef.current = null;
    }
    if (!target) {
      setIslandCollapsing(false);
      setIslandCollapsed(false);
      await invoke("set_island_collapsed", { collapsed: false }).catch(() => {});
    } else {
      justCollapsedAtRef.current = Date.now();
      updateIslandPinned(false);
      setIslandCollapsing(true);
      collapseAnimRef.current = window.setTimeout(() => {
        setIslandCollapsing(false);
        setIslandCollapsed(true);
        void invoke("set_island_collapsed", { collapsed: true }).catch(() => {});
      }, 110);
    }
  };

  const handleStopProjectStack = async (projectPath: string, pids: number[]) => {
    for (const pid of pids) {
      if (pid > 1) {
        await invoke("terminate_process", { pid }).catch(() => {});
      }
    }
    setPortsError(null);
    setActionFeedback({ kind: "ok", text: `Sent SIGTERM to ${pids.length} process(es) in ${projectPath}.` });
    window.setTimeout(() => {
      void refreshPorts();
      void refreshExtras();
    }, 550);
  };

  const handleCopyPortUrl = async (port: number) => {
    try {
      await navigator.clipboard.writeText(`http://localhost:${port}`);
      setCopiedPort(port);
      window.setTimeout(() => setCopiedPort((curr) => (curr === port ? null : curr)), 1500);
    } catch {
      // Ignore clipboard failures
    }
  };

  const handleRunSpeedTest = async () => {
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

  const handleCleanCache = async (cacheId: string, label: string) => {
    setCleaningCacheId(cacheId);
    setCacheMsg(null);
    try {
      const updated = await invoke<SystemExtrasSnapshot["caches"]>("clean_rebuildable_cache", {
        cacheId,
      });
      setExtras((prev) => (prev ? { ...prev, caches: updated } : prev));
      setCacheMsg(`Cleaned ${label}.`);
      void refreshExtras();
    } catch (err) {
      setCacheMsg(`Error cleaning ${label}: ${String(err)}`);
    } finally {
      setCleaningCacheId(null);
    }
  };

  const handleUpdateAudio = async (
    outputVolume: number | null,
    inputVolume: number | null,
    muted: boolean | null,
  ) => {
    try {
      const updated = await invoke<SystemExtrasSnapshot["audio"]>("update_audio_volume", {
        outputVolume,
        inputVolume,
        muted,
      });
      setExtras((prev) => (prev ? { ...prev, audio: updated } : prev));
    } catch (err) {
      console.error("Audio update failed:", err);
    }
  };

  const temperatures = telemetry?.temperatures.status === "available" ? telemetry.temperatures.value : null;
  const sensors: TemperatureReading[] = temperatures?.sensors ?? [];
  const fans: FanReading[] = telemetry?.fans.status === "available" ? telemetry.fans.value : [];
  const battery: BatteryReading | null =
    telemetry?.battery.status === "available" ? telemetry.battery.value : null;
  const processes: ProcessReading[] =
    telemetry?.top_processes.status === "available" ? telemetry.top_processes.value : [];
  const fanActuationStatus = telemetry?.fan_actuation_status ?? "not_registered";
  const hasSmcAccess = telemetry?.temperatures.status === "available";
  const unit = preferences.temperature_unit === "celsius" ? "C" : "F";
  const isDynamicIsland = preferences.layout_density === "dynamic_island";
  const isCollapsedPill = isDynamicIsland && islandCollapsed;

  const cpuCelsius = temperatures?.cpu_celsius ?? null;
  const gpuCelsius = temperatures?.gpu_celsius ?? null;
  const thermalLoad =
    cpuCelsius === null || cpuCelsius < 55
      ? "cool"
      : cpuCelsius < 70
        ? "nominal"
        : cpuCelsius < 85
          ? "warm"
          : "hot";

  const statusMeta =
    cpuCelsius === null
      ? { label: "Waiting", cls: "pp-pill--good" }
      : cpuCelsius >= 85
        ? { label: "● Hot", cls: "pp-pill--bad" }
        : cpuCelsius >= 70
          ? { label: "● Warm", cls: "pp-pill--warn" }
          : { label: "● Cool", cls: "pp-pill--good" };

  const fanPct = (fan: FanReading | undefined): number | null => {
    if (!fan || fan.min_speed_rpm === null || fan.max_speed_rpm === null) return null;
    if (fan.max_speed_rpm <= fan.min_speed_rpm) return null;
    return Math.round(
      Math.min(
        Math.max(((fan.speed_rpm - fan.min_speed_rpm) / (fan.max_speed_rpm - fan.min_speed_rpm)) * 100, 0),
        100,
      ),
    );
  };

  const fan1 = fans[0];
  const peakCore = useMemo(() => {
    if (sensors.length === 0) return null;
    return sensors.reduce((best, s) => (s.celsius > best.celsius ? s : best), sensors[0]);
  }, [sensors]);
  const avgSensorTemp = useMemo(() => {
    if (sensors.length === 0) return null;
    return sensors.reduce((sum, s) => sum + s.celsius, 0) / sensors.length;
  }, [sensors]);

  const totalProcCpu = useMemo(
    () => Math.max(processes.reduce((s, p) => s + p.cpu_percent, 0), 1),
    [processes],
  );
  const packageWatts = extras?.silicon_power.package_watts || battery?.power_watts || 0;

  const filteredProcesses = useMemo(() => {
    if (isCollapsedPill) return [];
    const q = appSearch.trim().toLowerCase();
    const list = q ? processes.filter((p) => p.name.toLowerCase().includes(q)) : [...processes];
    if (appSort === "mem") {
      return [...list].sort((a, b) => b.memory_mb - a.memory_mb);
    }
    if (appSort === "energy") {
      return [...list].sort(
        (a, b) => b.cpu_percent * 2 + b.memory_mb * 0.01 - (a.cpu_percent * 2 + a.memory_mb * 0.01),
      );
    }
    return [...list].sort((a, b) => b.cpu_percent - a.cpu_percent);
  }, [isCollapsedPill, processes, appSearch, appSort]);

  const projectStacks = useMemo(() => {
    if (isCollapsedPill) return [];
    const grouped = new Map<string, { path: string; ports: ListeningPort[]; pids: number[] }>();
    for (const p of ports) {
      if (!p.project_path) continue;
      const existing = grouped.get(p.project_path) ?? { path: p.project_path, ports: [], pids: [] };
      existing.ports.push(p);
      if (!existing.pids.includes(p.pid)) {
        existing.pids.push(p.pid);
      }
      grouped.set(p.project_path, existing);
    }
    return Array.from(grouped.values()).filter((s) => s.ports.length >= 1);
  }, [isCollapsedPill, ports]);

  const filteredPorts = useMemo(() => {
    if (isCollapsedPill) return [];
    const q = portSearch.trim().toLowerCase();
    if (!q) return ports;
    return ports.filter(
      (p) =>
        String(p.port).includes(q) ||
        p.process_name.toLowerCase().includes(q) ||
        p.bind_address.toLowerCase().includes(q) ||
        (p.project_path?.toLowerCase().includes(q) ?? false) ||
        (p.launcher?.toLowerCase().includes(q) ?? false),
    );
  }, [isCollapsedPill, ports, portSearch]);

  const exposedPorts = useMemo(() => ports.filter((p) => p.is_exposed), [ports]);
  const orphanedServers = extras?.orphaned_servers ?? [];
  const hotCulprit = useMemo(
    () => processes.find((p) => p.cpu_percent >= 50 && p.primary_pid > 1) ?? null,
    [processes],
  );
  const highThermalWarning =
    cpuCelsius !== null && cpuCelsius >= 80 && thermalPolicy.mode !== "performance";
  const pressureElevated = Boolean(
    extras?.thermal_pressure && extras.thermal_pressure !== "nominal",
  );
  const lowDiskWarning = Boolean(extras && extras.disk.free_gb > 0 && extras.disk.free_gb < 15);
  const micBusyWarning = Boolean(extras?.audio.mic_in_use_by && hotCulprit);
  const cameraActive = Boolean(extras?.audio.camera_in_use);
  const batteryHeatGuard = Boolean(
    battery?.is_charging && (battery.temperature_celsius ?? 0) >= 36,
  );
  const unsignedAgentsCount =
    extras?.launch_agents.filter((a) => !a.signed || !a.binary_exists).length ?? 0;
  const attentionCount =
    (hotCulprit ? 1 : 0) +
    (highThermalWarning ? 1 : 0) +
    (exposedPorts.length > 0 ? 1 : 0) +
    (orphanedServers.length > 0 ? 1 : 0) +
    (pressureElevated ? 1 : 0) +
    (lowDiskWarning ? 1 : 0) +
    (micBusyWarning ? 1 : 0) +
    (cameraActive ? 1 : 0) +
    (batteryHeatGuard ? 1 : 0);

  const commandItems = useMemo(() => {
    if (!cmdOpen) return [];
    const themeItems: { id: string; label: string; category: string; run: () => void }[] = (
      [
        ["catppuccin", "Theme: Catppuccin Mocha"],
        ["tokyo_night", "Theme: Tokyo Night"],
        ["nord", "Theme: Nord Frost"],
        ["gruvbox", "Theme: Gruvbox Dark"],
        ["rose_pine", "Theme: Rosé Pine"],
        ["dracula", "Theme: Dracula"],
        ["liquid_glass", "Theme: Liquid Glass"],
      ] as [PopoverLook, string][]
    ).map(([id, label]) => ({
      id: `look-${id}`,
      label,
      category: "Theme",
      run: () => void handleUpdatePreferences({ type: "set_popover_look", value: id }),
    }));

    const policyItems = (
      [
        ["system_auto", "Thermal Policy: System Auto"],
        ["quiet", "Thermal Policy: Quiet"],
        ["performance", "Thermal Policy: Performance"],
        ["custom", "Thermal Policy: Custom Rules"],
      ] as [ThermalPolicyMode, string][]
    ).map(([mode, label]) => ({
      id: `pol-${mode}`,
      label,
      category: "Fans",
      run: () => void handleSelectPolicyMode(mode),
    }));

    const tabItems = TAB_ORDER.map((tab) => ({
      id: `tab-${tab}`,
      label: `Open Tab: ${tab.charAt(0).toUpperCase() + tab.slice(1)}`,
      category: "Navigate",
      run: () => setActiveTab(tab),
    }));

    const portKillItems = ports.slice(0, 8).map((p) => ({
      id: `kill-${p.port}-${p.pid}`,
      label: `Stop Port :${p.port} (${p.process_name})`,
      category: "Port",
      run: () => void handleTerminateProcess(p.pid, `${p.process_name} :${p.port}`),
    }));

    const quickOps = [
      {
        id: "op-rec",
        label: recordingSession ? "Stop & Copy Thermal Session" : "Start Thermal Session Recording",
        category: "Action",
        run: () => void handleToggleRecording(),
      },
      {
        id: "op-health",
        label: "Toggle Mac Health Report",
        category: "Action",
        run: () => {
          setActiveTab("home");
          setShowHealthReport(true);
        },
      },
      {
        id: "op-dns",
        label: "Flush DNS Directory Cache",
        category: "Action",
        run: () => void handleRunMaintenance("flush_dns"),
      },
      {
        id: "op-orphans",
        label: "Clean Orphaned Dev Servers",
        category: "Action",
        run: () => void handleCleanupOrphans(),
      },
      {
        id: "op-awake",
        label: awakeStatus.keep_awake_active ? "Disable Keep Awake" : "Keep Awake for 1h",
        category: "Awake",
        run: () => void handleSetKeepAwake(!awakeStatus.keep_awake_active, awakeStatus.keep_awake_active ? null : 3600),
      },
      {
        id: "op-dark-mode",
        label: "Switch: Toggle macOS Dark Mode",
        category: "Switch",
        run: () => void handleRunQuickSwitch("toggle_dark_mode"),
      },
      {
        id: "op-hidden-files",
        label: "Switch: Toggle Hidden Finder Files",
        category: "Switch",
        run: () => void handleRunQuickSwitch("toggle_hidden_files"),
      },
      {
        id: "op-desktop-icons",
        label: "Switch: Toggle Desktop Icons",
        category: "Switch",
        run: () => void handleRunQuickSwitch("toggle_desktop_icons"),
      },
      {
        id: "op-clear-clip",
        label: "Switch: Clear Clipboard",
        category: "Switch",
        run: () => void handleRunQuickSwitch("clear_clipboard"),
      },
      {
        id: "op-empty-trash",
        label: "Switch: Empty macOS Trash",
        category: "Switch",
        run: () => void handleRunQuickSwitch("empty_trash"),
      },
      {
        id: "op-dust-blast",
        label: dustBlastSecs > 0 ? "Stop 15s Fan Dust Blast" : "Run 15s Fan Dust Blast (100% RPM)",
        category: "Fans",
        run: () => void (dustBlastSecs > 0 ? handleCancelDustBlast() : handleStartDustBlast()),
      },
      {
        id: "op-disk-bench",
        label: "Run NVMe Disk Speed Benchmark (64 MB)",
        category: "System",
        run: () => {
          setActiveTab("system");
          void handleRunDiskBenchmark();
        },
      },
      {
        id: "layout-dynamic-island",
        label: "Layout: Dynamic Island Capsule",
        category: "Layout",
        run: () => void handleUpdatePreferences({ type: "set_layout_density", value: "dynamic_island" as LayoutDensity }),
      },
      {
        id: "layout-comfortable",
        label: "Layout: Comfortable Popover",
        category: "Layout",
        run: () => void handleUpdatePreferences({ type: "set_layout_density", value: "comfortable" }),
      },
      {
        id: "layout-compact",
        label: "Layout: Compact Popover",
        category: "Layout",
        run: () => void handleUpdatePreferences({ type: "set_layout_density", value: "compact" }),
      },
      {
        id: "layout-island-pill",
        label: islandCollapsed ? "Expand Dynamic Island" : "Collapse Dynamic Island to Mini Pill",
        category: "Layout",
        run: () => {
          if (preferences.layout_density !== "dynamic_island") {
            void handleUpdatePreferences({ type: "set_layout_density", value: "dynamic_island" as LayoutDensity });
          } else {
            void handleToggleIslandCollapsed(!islandCollapsed);
          }
        },
      },
    ];

    const all = [...quickOps, ...portKillItems, ...policyItems, ...themeItems, ...tabItems];
    const q = cmdQuery.trim().toLowerCase().replace(/^:/, "");
    if (!q) return all;
    return all.filter(
      (item) => item.label.toLowerCase().includes(q) || item.category.toLowerCase().includes(q),
    );
  }, [
    cmdOpen,
    ports,
    recordingSession,
    awakeStatus.keep_awake_active,
    dustBlastSecs,
    preferences.layout_density,
    islandCollapsed,
    cmdQuery,
  ]);

  const windowedHistory = useMemo(() => {
    if (isCollapsedPill) return [];
    if (historyWindow === "2m") return history;
    const now = history[history.length - 1]?.time ?? Date.now();
    const cutoff = now - HISTORY_WINDOW_MS[historyWindow];
    const filtered = longHistory.filter((h) => h.time >= cutoff);
    return filtered.length >= 2 ? filtered : history;
  }, [isCollapsedPill, history, longHistory, historyWindow]);

  const tempHistory = useMemo(
    () => (isCollapsedPill ? [] : windowedHistory.map((h) => ({ time: h.time, cpu: h.cpu, gpu: h.gpu }))),
    [isCollapsedPill, windowedHistory],
  );

  const healthDiagnosis = useMemo(() => {
    if (isCollapsedPill) return [];
    const lines: string[] = [];
    if (cpuCelsius === null) {
      lines.push("• Thermal sensors: waiting for initial SMC sample.");
    } else if (cpuCelsius >= 80) {
      lines.push(
        `• High heat: CPU die is at ${formatTempShort(cpuCelsius, unit)} (throttling risk). Consider switching Thermal Policy to Perf or quitting heavy background tasks.`,
      );
    } else if (cpuCelsius >= 65) {
      lines.push(
        `• Moderate warmth: CPU is ${formatTempShort(cpuCelsius, unit)} under active workload.`,
      );
    } else {
      lines.push(`• Cool & nominal: CPU is ${formatTempShort(cpuCelsius, unit)} — no thermal throttling.`);
    }

    if (fan1) {
      lines.push(
        `• Cooling: ${fan1.label} spinning at ${fan1.speed_rpm} RPM (${fanPct(fan1) ?? 0}% capacity, policy: ${thermalPolicy.mode}).`,
      );
    }

    if (processes[0]) {
      lines.push(
        `• Top workload: ${processes[0].name} using ${processes[0].cpu_percent.toFixed(1)}% CPU & ${formatMemory(processes[0].memory_mb)} RAM.`,
      );
    }

    if (extras) {
      lines.push(
        `• Memory: ${extras.memory.used_gb.toFixed(1)}/${extras.memory.total_gb.toFixed(0)} GB (pressure: ${extras.memory.pressure_level}, swap ${extras.memory.swap_used_mb.toFixed(0)} MB, page-ins ${extras.memory.page_ins_per_sec.toFixed(1)}/s).`,
      );
      if (extras.orphaned_servers.length > 0) {
        lines.push(
          `• Dev clutter: ${extras.orphaned_servers.length} orphaned dev server(s) detached under launchd.`,
        );
      }
    }

    if (exposedPorts.length > 0) {
      lines.push(
        `• Network exposure: ${exposedPorts.length} port(s) bound to 0.0.0.0/* on LAN.`,
      );
    }

    return lines;
  }, [isCollapsedPill, cpuCelsius, unit, fan1, thermalPolicy.mode, processes, extras, exposedPorts.length]);

  const handleCopyHealthReport = async () => {
    const md = [
      `# StatPulse Mac Health Report`,
      `- Timestamp: ${new Date().toISOString()}`,
      ...healthDiagnosis.map((l) => l.replace(/^• /, "- ")),
    ].join("\n");
    try {
      await navigator.clipboard.writeText(md);
      setCopiedHealthReport(true);
      window.setTimeout(() => setCopiedHealthReport(false), 1800);
    } catch {
      // Ignore clipboard failures
    }
  };

  const handleCopyMcpConfig = async () => {
    const cfg = JSON.stringify(
      {
        mcpServers: {
          statpulse: {
            command: "/Applications/StatPulse.app/Contents/MacOS/superfan",
            args: ["--mcp"],
          },
        },
      },
      null,
      2,
    );
    try {
      await navigator.clipboard.writeText(cfg);
      setCopiedMcpConfig(true);
      window.setTimeout(() => setCopiedMcpConfig(false), 1800);
    } catch {
      // Ignore
    }
  };

  const agents = extras?.agents ?? [];
  const islandDotTone =
    dustBlastSecs > 0 || cameraActive || (cpuCelsius ?? 0) >= 80 || highThermalWarning
      ? "pp-dot--bad"
      : (cpuCelsius ?? 0) >= 65 || agents.length > 0
        ? "pp-dot--warn"
        : "pp-dot--good";

  useEffect(() => {
    cmdOpenRef.current = cmdOpen;
  }, [cmdOpen]);

  useEffect(() => {
    islandCollapsedRef.current = islandCollapsed;
    isDynamicIslandRef.current = isDynamicIsland;
    if (isDynamicIsland && !islandCollapsed) {
      const timer = window.setTimeout(() => {
        void refreshExtras();
        void refreshPorts();
        void refreshAwake();
      }, 220);
      return () => window.clearTimeout(timer);
    }
  }, [islandCollapsed, isDynamicIsland, refreshExtras, refreshPorts, refreshAwake]);

  useEffect(() => {
    if (preferences.layout_density !== "dynamic_island") {
      setIslandCollapsed(false);
      updateIslandPinned(false);
      setIslandCollapsing(false);
    }
  }, [preferences.layout_density, updateIslandPinned]);

  const handleIslandMouseEnter = () => {
    if (!isDynamicIsland) return;
    if (hoverTimeoutRef.current !== null) {
      window.clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    if (islandCollapsed && !islandCollapsing && Date.now() - justCollapsedAtRef.current > 450) {
      hoverTimeoutRef.current = window.setTimeout(() => {
        void handleToggleIslandCollapsed(false);
      }, 110);
    }
  };

  const handleIslandMouseLeave = () => {
    // Auto-retract & hide when leaving Dynamic Island is driven by the Rust global cursor tracker
    // (which honors peek timers and hover even when StatPulse is unfocused).
    if (hoverTimeoutRef.current !== null) {
      window.clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
  };

  const liveActivityChip = (
    <button
      type="button"
      onClick={() => {
        const targetTab: AppTab =
          dustBlastSecs > 0
            ? "fans"
            : cameraActive || extras?.audio.mic_in_use_by
              ? "volume"
              : agents.length > 0
                ? "agents"
                : "system";
        setActiveTab(targetTab);
        updateIslandPinned(true);
        if (islandCollapsed) void handleToggleIslandCollapsed(false);
      }}
      className="di-pill"
      title="Live Activity — click to inspect"
    >
      {dustBlastSecs > 0 ? (
        <>
          <Wind className="w-3 h-3 animate-spin text-[var(--acc)]" />
          <span>Blast {dustBlastSecs}s</span>
        </>
      ) : cameraActive || extras?.audio.mic_in_use_by ? (
        <>
          <span className="pp-dot pp-dot--bad" />
          <span>{cameraActive ? "Cam Active" : "Mic Active"}</span>
        </>
      ) : agents.length > 0 ? (
        <>
          <Sparkles className="w-3 h-3 text-[var(--acc)]" />
          <span>
            {agents.length} AI Agent{agents.length > 1 ? "s" : ""}
          </span>
        </>
      ) : (
        <span className="font-mono">
          {fans[0]?.speed_rpm ?? 0} RPM ·{" "}
          {extras ? `${extras.silicon_power.package_watts.toFixed(1)}W` : "--"}
        </span>
      )}
    </button>
  );

  return (
    <div
      data-look={preferences.popover_look}
      data-density={preferences.layout_density}
      data-palette={preferences.chart_palette}
      data-thermal-load={thermalLoad}
      className="w-full h-screen pp-shell"
    >
      {!isDynamicIsland && (
        <NavigationRail
          activeTab={activeTab}
          setActiveTab={setActiveTab}
          fanCount={fans.length}
          appCount={processes.length}
          portCount={ports.length}
          agentCount={agents.length}
          keepAwakeActive={awakeStatus.keep_awake_active}
        />
      )}

      <div
        className={
          isDynamicIsland
            ? `di-frame ${
                islandCollapsing
                  ? "di-frame--collapsing"
                  : islandCollapsed
                    ? "di-frame--pill"
                    : "di-frame--expanded"
              }`
            : "pp-main"
        }
        onMouseEnter={isDynamicIsland ? handleIslandMouseEnter : undefined}
        onMouseLeave={isDynamicIsland ? handleIslandMouseLeave : undefined}
      >
        <div className={isDynamicIsland ? "di-body" : "contents"}>
          {isDynamicIsland ? (
            <>
              <div className="di-notch-row">
                <div
                  className="flex items-center gap-1 min-w-0 cursor-pointer py-0.5"
                  onClick={() => {
                    if (islandCollapsed) {
                      updateIslandPinned(true);
                      void handleToggleIslandCollapsed(false);
                    } else {
                      handleDismissIslandIntoNotch();
                    }
                  }}
                  title={islandCollapsed ? "Click to open detail panel" : "Click to collapse"}
                >
                  <span className={`pp-dot shrink-0 ${islandDotTone}`} />
                  <span className="di-wave shrink-0" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                    <i />
                    <i />
                  </span>
                  <span className="font-mono text-[9px] tracking-tight font-semibold text-[var(--ink)] truncate">
                    {cpuCelsius !== null ? `${Math.round(unit === "C" ? cpuCelsius : cpuCelsius * 1.8 + 32)}` : "--"}/
                    {gpuCelsius !== null ? `${Math.round(unit === "C" ? gpuCelsius : gpuCelsius * 1.8 + 32)}°` : "--°"}{" "}
                    {dustBlastSecs > 0
                      ? `Blast ${dustBlastSecs}s`
                      : fans.length > 0
                        ? `${fans[0]?.speed_rpm ?? 0}rpm`
                        : "Fan --"}
                  </span>
                </div>

                <div
                  className="di-notch-bridge cursor-pointer"
                  onClick={() => {
                    if (islandCollapsed) {
                      updateIslandPinned(true);
                      void handleToggleIslandCollapsed(false);
                    } else {
                      handleDismissIslandIntoNotch();
                    }
                  }}
                  title={islandCollapsed ? "Click to open detail panel" : "Click to collapse"}
                >
                  {cameraActive && <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />}
                  {!cameraActive && extras?.audio.mic_in_use_by && (
                    <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
                  )}
                  <span className="di-notch-glow" />
                </div>

                {islandCollapsed ? (
                  <div
                    className="flex items-center justify-end gap-1 min-w-0 cursor-pointer py-0.5"
                    onClick={() => {
                      updateIslandPinned(true);
                      void handleToggleIslandCollapsed(false);
                    }}
                    title="Click to open detail panel"
                  >
                    <span className="font-mono text-[9px] tracking-tight font-medium text-[var(--sec)] truncate">
                      {(battery?.power_watts ?? extras?.silicon_power.package_watts ?? 0).toFixed(1)}W{" "}
                      {extras ? `${extras.memory.used_gb.toFixed(1)}G` : "--G"} ↓
                      {extras ? formatRate(extras.network.rx_bytes_per_sec) : "0B"}
                    </span>
                    {agents.length > 0 && (
                      <span className="font-mono text-[8px] font-semibold px-1 py-0.5 rounded-full bg-[var(--acc)]/15 text-[var(--acc)] shrink-0">
                        {agents.length}AI
                      </span>
                    )}
                  </div>
                ) : (
                  <div className="flex items-center justify-end gap-1 shrink-0">
                    <button
                      type="button"
                      onClick={() => updateIslandPinned(!islandPinned)}
                      className={`di-pill ${islandPinned ? "border-[var(--acc)] text-[var(--acc)]" : ""}`}
                      title={islandPinned ? "Pinned (hover-collapse paused) — click to unpin" : "Pin open"}
                    >
                      <span>{islandPinned ? "Pinned" : "Pin"}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setCmdQuery("");
                        setCmdOpen((v) => !v);
                      }}
                      className="di-pill px-1.5"
                      title="Command Palette (⌘K)"
                    >
                      <Command className="w-2.5 h-2.5" />
                    </button>
                    <button
                      type="button"
                      onClick={handleDismissIslandIntoNotch}
                      className="di-pill px-1.5"
                      title="Collapse Detail Panel"
                    >
                      <ChevronUp className="w-3 h-3" />
                    </button>
                  </div>
                )}
              </div>

              {!islandCollapsed && (
                <>
                  <div className="flex items-center justify-between gap-2 px-3 py-1 border-b border-white/5">
                    {liveActivityChip}
                    <div className="flex items-center gap-1.5 shrink-0">
                      <button
                        type="button"
                        onClick={handleCyclePolicy}
                        className="pp-btn capitalize"
                        title="Cycle Thermal Policy"
                      >
                        {thermalPolicy.mode === "system_auto"
                          ? "Auto"
                          : thermalPolicy.mode === "performance"
                            ? "Perf"
                            : thermalPolicy.mode}
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          void (dustBlastSecs > 0 ? handleCancelDustBlast() : handleStartDustBlast())
                        }
                        className={`pp-btn ${dustBlastSecs > 0 ? "pp-btn--danger" : "pp-btn--acc"}`}
                        disabled={fanActuationStatus !== "ready" || fans.length === 0}
                      >
                        {dustBlastSecs > 0 ? `${dustBlastSecs}s` : "Blast"}
                      </button>
                      <span className="font-mono text-[9.5px] text-[var(--sec)] truncate">
                        {extras ? `${extras.silicon_power.package_watts.toFixed(1)}W · ` : ""}
                        {battery?.charge_percent !== null && battery?.charge_percent !== undefined
                          ? `${battery.charge_percent}%`
                          : "AC"}
                      </span>
                    </div>
                  </div>
                  <div className="di-nav">
                    {TAB_ORDER.map((tab) => {
                      const badge =
                        tab === "fans" && fans.length > 0
                          ? fans.length
                          : tab === "ports" && ports.length > 0
                            ? ports.length
                            : tab === "agents" && agents.length > 0
                              ? agents.length
                              : null;
                      return (
                        <button
                          key={tab}
                          type="button"
                          aria-pressed={activeTab === tab}
                          onClick={() => {
                            updateIslandPinned(true);
                            setActiveTab(tab);
                          }}
                          className="di-tab"
                        >
                          <span className="capitalize">{tab}</span>
                          {badge !== null && (
                            <span className="font-mono text-[8.5px] opacity-80">{badge}</span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </>
              )}
            </>
          ) : (
          <Header
            activeTab={activeTab}
            hasAccess={hasSmcAccess}
            policyMode={thermalPolicy.mode}
            onCyclePolicy={handleCyclePolicy}
            onOpenCommandPalette={() => {
              setCmdQuery("");
              setCmdOpen((v) => !v);
            }}
            onHideWindow={handleHideWindow}
          />
        )}

        {!(isDynamicIsland && islandCollapsed) && (
          <>
            <div className="pp-scroll">
          {activeTab === "home" && (
            <>
              {/* Top Quick Actions: History Window Switcher, Recorder & 1-Tap Health Report */}
              <div className="flex items-center justify-between gap-1.5">
                <div className="flex items-center gap-0.5 bg-[var(--rail)] p-0.5 rounded-md border border-[var(--card-border)]">
                  {(["2m", "1h", "24h", "7d"] as HistoryWindow[]).map((w) => (
                    <button
                      key={w}
                      type="button"
                      onClick={() => setHistoryWindow(w)}
                      className={`px-1.5 py-0.5 rounded text-[9px] font-mono font-semibold transition-colors cursor-pointer ${
                        historyWindow === w
                          ? "bg-[var(--surface)] text-[var(--acc)]"
                          : "text-[var(--sec)] hover:text-[var(--ink)]"
                      }`}
                    >
                      {w}
                    </button>
                  ))}
                </div>

                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() => void handleToggleRecording()}
                    className={`pp-btn inline-flex items-center gap-1 ${
                      recordingSession ? "pp-btn--danger" : ""
                    }`}
                    title="Record live thermal/power session & export CSV/Markdown"
                  >
                    <span
                      className={`w-1.5 h-1.5 rounded-full ${
                        recordingSession ? "bg-rose-400 animate-pulse" : "bg-rose-400/70"
                      }`}
                    />
                    {recordingSession ? `Rec (${recordedSamples.length})` : "Rec"}
                  </button>

                  <button
                    type="button"
                    onClick={() => setShowHealthReport((v) => !v)}
                    className={`pp-btn inline-flex items-center gap-1 ${
                      showHealthReport ? "pp-btn--active" : ""
                    }`}
                  >
                    <Stethoscope className="w-2.5 h-2.5" />
                    Health
                  </button>
                </div>
              </div>

              {sessionExportMsg && (
                <div className="px-2.5 py-1.5 rounded-lg bg-emerald-500/10 border border-emerald-500/30 text-[10px] font-mono text-emerald-200">
                  {sessionExportMsg}
                </div>
              )}

              {showHealthReport && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Stethoscope className="w-3 h-3 text-[var(--acc)]" />
                      Why is my Mac acting like this?
                    </span>
                    <button
                      type="button"
                      onClick={() => void handleCopyHealthReport()}
                      className="pp-btn inline-flex items-center gap-1"
                    >
                      {copiedHealthReport ? (
                        <>
                          <Check className="w-2.5 h-2.5 text-[var(--good)]" />
                          Copied
                        </>
                      ) : (
                        <>
                          <Copy className="w-2.5 h-2.5" />
                          Copy MD
                        </>
                      )}
                    </button>
                  </div>
                  <div className="flex flex-col gap-1 text-[10px] text-[var(--ter)] leading-snug">
                    {healthDiagnosis.map((line, i) => (
                      <div key={i}>{line}</div>
                    ))}
                  </div>
                </div>
              )}

              {/* 3x2 Stat Space Summary Sparkline Tiles */}
              <div className="pp-tiles">
                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("system")}
                >
                  <div className="pp-tile__top">
                    <span>CPU</span>
                    <span>{sensors.length > 0 ? `${sensors.length} cores` : "SMC"}</span>
                  </div>
                  <span className="pp-tile__val">{formatTempShort(cpuCelsius, unit)}</span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline points={sparkPoints(windowedHistory.map((h) => h.cpu), 30)} />
                  </svg>
                </button>

                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("system")}
                >
                  <div className="pp-tile__top">
                    <span>GPU</span>
                    <span>Cluster</span>
                  </div>
                  <span className="pp-tile__val">{formatTempShort(gpuCelsius, unit)}</span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline
                      points={sparkPoints(
                        windowedHistory.flatMap((h) => (h.gpu === null ? [] : [h.gpu])),
                        30,
                      )}
                    />
                  </svg>
                </button>

                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("system")}
                >
                  <div className="pp-tile__top">
                    <span>MEM</span>
                    <span>{extras ? `of ${extras.memory.total_gb.toFixed(0)}G` : "RAM"}</span>
                  </div>
                  <span className="pp-tile__val">
                    {extras ? `${extras.memory.used_gb.toFixed(1)}G` : "--"}
                  </span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline points={sparkPoints(memHistory, 0)} />
                  </svg>
                </button>

                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("fans")}
                >
                  <div className="pp-tile__top">
                    <span>FANS</span>
                    <span>{fan1 ? `${fanPct(fan1) ?? 0}%` : "None"}</span>
                  </div>
                  <span className="pp-tile__val">
                    {!fan1 ? "--" : fan1.speed_rpm === 0 ? "Silent" : `${fan1.speed_rpm} rpm`}
                  </span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline
                      points={sparkPoints(
                        windowedHistory.flatMap((h) => (h.fan1Rpm === null ? [] : [h.fan1Rpm])),
                        0,
                      )}
                    />
                  </svg>
                </button>

                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("network")}
                >
                  <div className="pp-tile__top">
                    <span>NET</span>
                    <span>{extras ? `↑${formatRate(extras.network.tx_bytes_per_sec)}` : "Link"}</span>
                  </div>
                  <span className="pp-tile__val">
                    {extras ? `↓${formatRate(extras.network.rx_bytes_per_sec)}` : "--"}
                  </span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline points={sparkPoints(netHistory.map((n) => n.rx), 0)} />
                  </svg>
                </button>

                <button
                  type="button"
                  className="pp-tile"
                  onClick={() => setActiveTab("devices")}
                >
                  <div className="pp-tile__top">
                    <span>BAT</span>
                    <span>
                      {battery?.power_watts !== null && battery?.power_watts !== undefined
                        ? `${battery.power_watts.toFixed(1)}W`
                        : "Power"}
                    </span>
                  </div>
                  <span className="pp-tile__val">
                    {battery?.charge_percent !== null && battery?.charge_percent !== undefined
                      ? `${battery.charge_percent}%`
                      : "--"}
                  </span>
                  <svg className="pp-spark" viewBox="0 0 100 14" preserveAspectRatio="none" aria-hidden="true">
                    <polyline
                      points={sparkPoints(
                        windowedHistory.flatMap((h) => (h.batteryPct === null ? [] : [h.batteryPct])),
                        0,
                      )}
                    />
                  </svg>
                </button>
              </div>

              {/* Conditional Stat Space Needs Attention Card */}
              {attentionCount > 0 && (
                <div className="pp-card pp-card--attn">
                  <div className="pp-card__head">
                    <span>Needs attention</span>
                    <em>{attentionCount}</em>
                  </div>

                  {hotCulprit && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--warn" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">{hotCulprit.name}</b>{" "}
                        · {hotCulprit.cpu_percent.toFixed(0)}% CPU
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleTerminateProcess(hotCulprit.primary_pid, hotCulprit.name)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Quit
                      </button>
                    </div>
                  )}

                  {highThermalWarning && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--bad" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">CPU at {formatTempShort(cpuCelsius, unit)}</b>{" "}
                        · boost cooling
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleSelectPolicyMode("performance")}
                        className="pp-btn pp-btn--acc shrink-0"
                      >
                        Perf Mode
                      </button>
                    </div>
                  )}

                  {orphanedServers.length > 0 && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--warn" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">
                          {orphanedServers.length} orphaned dev server{orphanedServers.length > 1 ? "s" : ""}
                        </b>{" "}
                        under launchd
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleCleanupOrphans()}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Clean
                      </button>
                    </div>
                  )}

                  {exposedPorts.length > 0 && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--warn" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">{exposedPorts.length} port{exposedPorts.length > 1 ? "s" : ""}</b>{" "}
                        exposed on LAN
                      </span>
                      <button
                        type="button"
                        onClick={() => setActiveTab("ports")}
                        className="pp-btn shrink-0"
                      >
                        View
                      </button>
                    </div>
                  )}

                  {pressureElevated && extras && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--bad" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">Thermal pressure: {extras.thermal_pressure}</b> · clocks limited
                      </span>
                      <button
                        type="button"
                        onClick={() => setActiveTab("fans")}
                        className="pp-btn pp-btn--acc shrink-0"
                      >
                        Cool
                      </button>
                    </div>
                  )}

                  {lowDiskWarning && extras && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--bad" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)]">Low disk ({extras.disk.free_gb.toFixed(1)} GB free)</b>
                      </span>
                      <button
                        type="button"
                        onClick={() => setActiveTab("system")}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Free Up
                      </button>
                    </div>
                  )}

                  {micBusyWarning && extras?.audio.mic_in_use_by && hotCulprit && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--warn" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        Mic active (<b className="text-[var(--ink)]">{extras.audio.mic_in_use_by}</b>) while{" "}
                        <b className="text-[var(--ink)]">{hotCulprit.name}</b> spikes
                      </span>
                    </div>
                  )}

                  {cameraActive && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--bad" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        <b className="text-[var(--ink)] inline-flex items-center gap-1">
                          <Camera className="w-3 h-3 text-rose-400" />
                          Webcam is active
                        </b>{" "}
                        or streaming
                      </span>
                      <button
                        type="button"
                        onClick={() => setActiveTab("volume")}
                        className="pp-btn shrink-0"
                      >
                        Privacy
                      </button>
                    </div>
                  )}

                  {batteryHeatGuard && battery && (
                    <div className="pp-attn">
                      <span className="pp-dot pp-dot--warn" />
                      <span className="flex-1 min-w-0 truncate text-[var(--ter)]">
                        Battery charging hot (
                        <b className="text-[var(--ink)]">
                          {formatTempShort(battery.temperature_celsius, unit)}
                        </b>
                        )
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleSelectPolicyMode("performance")}
                        className="pp-btn pp-btn--acc shrink-0"
                      >
                        Boost Fans
                      </button>
                    </div>
                  )}
                </div>
              )}

              {/* Card 1: Thermal Policy & Fans */}
              <div className="pp-card">
                <div className="pp-card__head">
                  <span>Thermal policy</span>
                  <em>
                    {fans.length} {fans.length === 1 ? "fan" : "fans"} ·{" "}
                    {fanActuationStatus === "ready" ? "Helper ready" : "Read-only"}
                  </em>
                </div>

                <div className="grid grid-cols-4 gap-1">
                  {(
                    [
                      { id: "system_auto", label: "Auto" },
                      { id: "quiet", label: "Quiet" },
                      { id: "performance", label: "Perf" },
                      { id: "custom", label: "Custom" },
                    ] as { id: ThermalPolicyMode; label: string }[]
                  ).map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => void handleSelectPolicyMode(item.id)}
                      className={`pp-btn py-1 ${thermalPolicy.mode === item.id ? "pp-btn--active" : ""}`}
                    >
                      {item.label}
                    </button>
                  ))}
                </div>

                {fans.map((f) => {
                  const pct = fanPct(f);
                  return (
                    <div key={f.id} className="pp-kv text-[var(--ter)]">
                      <span className="truncate">{f.label}</span>
                      <div className="flex items-center gap-2 shrink-0">
                        <b className="font-mono text-[var(--ink)]">
                          {f.speed_rpm} RPM{pct !== null ? ` · ${pct}%` : ""}
                        </b>
                        <button
                          type="button"
                          onClick={() => setActiveTab("fans")}
                          className="pp-btn"
                        >
                          Tune
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Card 2: Top Apps */}
              <div className="pp-card">
                <div className="pp-card__head">
                  <span>Top apps</span>
                  <button
                    type="button"
                    onClick={() => setActiveTab("apps")}
                    className="text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    CPU · MEM ▸
                  </button>
                </div>

                {processes.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">
                    {telemetry?.top_processes.status === "unavailable"
                      ? telemetry.top_processes.reason
                      : "All background processes idle (< 0.1% CPU)"}
                  </div>
                ) : (
                  processes.slice(0, 4).map((proc) => (
                    <div key={proc.name} className="pp-row3">
                      <span className="truncate text-[var(--ink)]">
                        {proc.name}
                        {proc.process_count > 1 && (
                          <span className="text-[var(--sec)] ml-1 text-[10px]">
                            ×{proc.process_count}
                          </span>
                        )}
                      </span>
                      <b className="text-right font-mono text-[var(--ink)]">
                        {proc.cpu_percent.toFixed(1)}%
                      </b>
                      <span className="text-right font-mono text-[var(--sec)]">
                        {formatMemory(proc.memory_mb)}
                      </span>
                    </div>
                  ))
                )}
              </div>

              {/* Card 3: CPU Cores & Sensors */}
              <div className="pp-card">
                <div className="pp-card__head">
                  <span>CPU cores &amp; sensors</span>
                  <button
                    type="button"
                    onClick={() => setActiveTab("system")}
                    className="text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    {sensors.length} cores · Avg {formatTempShort(avgSensorTemp, unit)} ▸
                  </button>
                </div>

                {sensors.length > 0 && (
                  <div className="pp-cores" title="Per-core thermal spectrum">
                    {sensors.slice(0, 16).map((s) => {
                      const scale = Math.min(Math.max((s.celsius - 25) / 70, 0.15), 1);
                      return (
                        <span key={s.key} className="pp-core" title={`${s.label}: ${Math.round(s.celsius)}°C`}>
                          <i style={{ transform: `scaleY(${scale.toFixed(2)})` }} />
                        </span>
                      );
                    })}
                  </div>
                )}

                <div className="grid grid-cols-2 gap-x-4 gap-y-0.5 pt-0.5">
                  <div className="pp-kv">
                    <span className="text-[var(--sec)]">CPU die</span>
                    <b className="font-mono">{formatTempShort(cpuCelsius, unit)}</b>
                  </div>
                  <div className="pp-kv">
                    <span className="text-[var(--sec)]">GPU</span>
                    <b className="font-mono">{formatTempShort(gpuCelsius, unit)}</b>
                  </div>
                  <div className="pp-kv">
                    <span className="text-[var(--sec)]">Peak core</span>
                    <b className="font-mono">{formatTempShort(peakCore?.celsius ?? null, unit)}</b>
                  </div>
                  <div className="pp-kv">
                    <span className="text-[var(--sec)]">Disk free</span>
                    <b className="font-mono">
                      {extras ? `${extras.disk.free_gb.toFixed(0)} GB` : "--"}
                    </b>
                  </div>
                </div>
              </div>
            </>
          )}

          {activeTab === "fans" && (
            <>
              {policyError && (
                <div className="px-3 py-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  Thermal policy update failed: {policyError}
                </div>
              )}

              <FanRuleManager
                activePreset={thermalPolicy.mode}
                customRules={thermalPolicy.rules}
                sensors={sensors}
                fans={fans}
                currentCpuCelsius={cpuCelsius}
                currentGpuCelsius={gpuCelsius}
                unit={unit}
                defaultShowCurve={true}
                onSelectPreset={handleSelectPolicyMode}
                onSaveRule={handleSaveRule}
                onDeleteRule={handleDeleteRule}
              />

              <div className="space-y-2">
                <div className="flex items-center justify-between gap-2 px-1">
                  <span className="text-[10px] font-bold uppercase tracking-wider text-[var(--sec)]">
                    Fan Speed Controls
                  </span>
                  <div className="flex items-center gap-2">
                    {dustBlastSecs > 0 ? (
                      <button
                        type="button"
                        onClick={() => void handleCancelDustBlast()}
                        className="pp-btn pp-btn--danger inline-flex items-center gap-1 font-mono"
                      >
                        <Wind className="w-2.5 h-2.5 animate-spin" />
                        Blast {dustBlastSecs}s · Stop
                      </button>
                    ) : (
                      <button
                        type="button"
                        disabled={fanActuationStatus !== "ready" || fans.length === 0}
                        onClick={() => void handleStartDustBlast()}
                        className="pp-btn pp-btn--acc inline-flex items-center gap-1 disabled:opacity-40"
                        title="Spin all fans at 100% Max RPM for 15 seconds then auto-restore"
                      >
                        <Wind className="w-2.5 h-2.5" />
                        15s Dust Blast
                      </button>
                    )}
                    <span className="text-[10px] font-mono font-semibold text-[var(--acc)]">
                      {fans.length} Fans
                    </span>
                  </div>
                </div>

                {fans.map((fan) => (
                  <FanCard
                    key={fan.id}
                    fan={fan}
                    fanActuationStatus={fanActuationStatus}
                    thermalPolicyMode={thermalPolicy.mode}
                    onSetSpeed={handleSetFanSpeed}
                    onSetMode={handleSetFanMode}
                    onEnableHelper={handleEnableHelper}
                    onSelectPolicyMode={handleSelectPolicyMode}
                  />
                ))}

                {telemetry && telemetry.fans.status !== "available" && (
                  <div className="pp-card text-[10px] text-[var(--sec)]">
                    {unavailableReason(telemetry.fans, "No hardware fans were reported.")}
                  </div>
                )}
              </div>
            </>
          )}

          {activeTab === "system" && (
            <>
              <div className="flex items-center justify-between gap-2">
                <div className="pp-pillrow">
                  <span className={`pp-pill ${statusMeta.cls}`}>{statusMeta.label}</span>
                  {extras && (
                    <span
                      className={`pp-pill ${
                        extras.thermal_pressure === "nominal" ? "pp-pill--good" : "pp-pill--bad"
                      }`}
                    >
                      Pressure: {extras.thermal_pressure}
                    </span>
                  )}
                  <span className="text-[var(--sec)] text-[10px]">
                    SMC {hasSmcAccess ? "OK" : "Limited"} · {sensors.length} cores
                  </span>
                </div>

                <div className="flex items-center gap-0.5 bg-[var(--rail)] p-0.5 rounded-md border border-[var(--card-border)] shrink-0">
                  {(["2m", "1h", "24h", "7d"] as HistoryWindow[]).map((w) => (
                    <button
                      key={w}
                      type="button"
                      onClick={() => setHistoryWindow(w)}
                      className={`px-1.5 py-0.5 rounded text-[9px] font-mono font-semibold cursor-pointer ${
                        historyWindow === w
                          ? "bg-[var(--surface)] text-[var(--acc)]"
                          : "text-[var(--sec)] hover:text-[var(--ink)]"
                      }`}
                    >
                      {w}
                    </button>
                  ))}
                </div>
              </div>

              {telemetry?.temperatures.status === "unavailable" && (
                <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  <ShieldAlert className="w-3.5 h-3.5 shrink-0" />
                  <span>Temperature telemetry unavailable: {telemetry.temperatures.reason}</span>
                </div>
              )}

              <TemperatureGauge cpuTemp={cpuCelsius} gpuTemp={gpuCelsius} unit={unit} />

              <TemperatureChart history={tempHistory} unit={unit} />

              {/* Apple Silicon Power Breakdown + 1-Tap Maintenance */}
              {extras && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Zap className="w-3 h-3 text-[var(--acc)]" />
                      Silicon Power Rails
                    </span>
                    <em className="font-mono">{extras.silicon_power.package_watts.toFixed(1)}W pkg</em>
                  </div>

                  {extras.silicon_power.package_watts > 0 ? (
                    <>
                      <div className="pp-membar">
                        <i
                          style={{
                            width: `${(extras.silicon_power.cpu_watts / Math.max(extras.silicon_power.package_watts, 0.1)) * 100}%`,
                            background: "var(--acc)",
                          }}
                        />
                        <i
                          style={{
                            width: `${(extras.silicon_power.gpu_watts / Math.max(extras.silicon_power.package_watts, 0.1)) * 100}%`,
                            background: "var(--warn)",
                          }}
                        />
                        <i
                          style={{
                            width: `${(extras.silicon_power.dram_watts / Math.max(extras.silicon_power.package_watts, 0.1)) * 100}%`,
                            background: "var(--good)",
                          }}
                        />
                        <i
                          style={{
                            width: `${(extras.silicon_power.ane_watts / Math.max(extras.silicon_power.package_watts, 0.1)) * 100}%`,
                            background: "#c9a7ff",
                          }}
                        />
                      </div>
                      <div className="pp-legend font-mono">
                        <span><i style={{ background: "var(--acc)" }} />CPU {extras.silicon_power.cpu_watts.toFixed(1)}W</span>
                        <span><i style={{ background: "var(--warn)" }} />GPU {extras.silicon_power.gpu_watts.toFixed(1)}W</span>
                        <span><i style={{ background: "var(--good)" }} />DRAM {extras.silicon_power.dram_watts.toFixed(1)}W</span>
                        <span><i style={{ background: "#c9a7ff" }} />ANE {extras.silicon_power.ane_watts.toFixed(1)}W</span>
                      </div>
                    </>
                  ) : (
                    <div className="text-[10px] text-[var(--sec)]">Package rail telemetry waiting for load.</div>
                  )}

                  <div className="flex items-center justify-between gap-2 pt-1 border-t border-white/5 text-[10px]">
                    <span className="text-[var(--sec)] truncate">
                      SSD SMART:{" "}
                      <b className="text-[var(--good)]">{extras.ssd_health.smart_status || "Verified"}</b>
                      {extras.ssd_health.wear_used_percent !== null &&
                        ` · ${extras.ssd_health.wear_used_percent}% wear`}
                      {extras.ssd_health.data_written_tb !== null &&
                        ` · ${extras.ssd_health.data_written_tb}TBW`}
                    </span>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        type="button"
                        onClick={() => void handleRunMaintenance("flush_dns")}
                        className="pp-btn"
                      >
                        Flush DNS
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleRunMaintenance("purge_ram")}
                        className="pp-btn"
                      >
                        Purge RAM
                      </button>
                    </div>
                  </div>
                  {maintenanceMsg && (
                    <div className="text-[9.5px] font-mono text-[var(--acc)]">{maintenanceMsg}</div>
                  )}
                </div>
              )}

              {/* Memory Pressure Card */}
              {extras && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span>Memory</span>
                    <em>
                      pressure {extras.memory.pressure_level} · {extras.memory.used_gb.toFixed(1)} / {extras.memory.total_gb.toFixed(0)} GB
                    </em>
                  </div>

                  <div className="pp-membar">
                    <i
                      style={{
                        width: `${Math.min((extras.memory.app_gb / Math.max(extras.memory.total_gb, 1)) * 100, 100)}%`,
                        background: "var(--acc)",
                      }}
                    />
                    <i
                      style={{
                        width: `${Math.min((extras.memory.wired_gb / Math.max(extras.memory.total_gb, 1)) * 100, 100)}%`,
                        background: "#83c5be",
                      }}
                    />
                    <i
                      style={{
                        width: `${Math.min((extras.memory.compressed_gb / Math.max(extras.memory.total_gb, 1)) * 100, 100)}%`,
                        background: "#c9a7ff",
                      }}
                    />
                  </div>

                  <div className="pp-legend">
                    <span><i style={{ background: "var(--acc)" }} />App {extras.memory.app_gb.toFixed(1)}G</span>
                    <span><i style={{ background: "#83c5be" }} />Wired {extras.memory.wired_gb.toFixed(1)}G</span>
                    <span><i style={{ background: "#c9a7ff" }} />Comp {extras.memory.compressed_gb.toFixed(1)}G</span>
                    <span>Swap {extras.memory.swap_used_mb.toFixed(0)}M</span>
                  </div>
                  <div className="pp-kv text-[10px] text-[var(--sec)]">
                    <span>Paging rate</span>
                    <b className="font-mono text-[var(--ink)]">
                      Page-ins {extras.memory.page_ins_per_sec.toFixed(1)}/s · Swap-outs{" "}
                      {extras.memory.swap_outs_per_sec.toFixed(1)}/s
                    </b>
                  </div>
                </div>
              )}

              {/* Disk & Free up space (Rebuildable Cache) */}
              {extras && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <HardDrive className="w-3 h-3" />
                      Free up space
                    </span>
                    <em>
                      {extras.disk.free_gb.toFixed(0)} GB free ({extras.disk.used_percent}% used)
                      {extras.disk.apfs_snapshots_count > 0
                        ? ` · ${extras.disk.apfs_snapshots_count} APFS snaps`
                        : ""}
                    </em>
                  </div>

                  {cacheMsg && (
                    <div className="text-[10px] font-mono text-[var(--good)]">{cacheMsg}</div>
                  )}

                  {extras.caches.length === 0 ? (
                    <div className="text-[10px] text-[var(--sec)]">
                      No large rebuildable developer caches detected.
                    </div>
                  ) : (
                    <div className="flex flex-col gap-1">
                      {extras.caches.map((c) => (
                        <div key={c.id} className="pp-kv">
                          <span className="truncate text-[var(--ter)]">{c.label}</span>
                          <div className="flex items-center gap-2 shrink-0">
                            <b className="font-mono text-[var(--ink)]">{formatMemory(c.size_mb)}</b>
                            <button
                              type="button"
                              disabled={cleaningCacheId === c.id}
                              onClick={() => void handleCleanCache(c.id, c.label)}
                              className="pp-btn pp-btn--danger inline-flex items-center gap-1 disabled:opacity-40"
                            >
                              <Trash2 className="w-2.5 h-2.5" />
                              {cleaningCacheId === c.id ? "..." : "Clean"}
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {extras && (
                <>
                  {/* Quick Switches */}
                  <div className="pp-card gap-1.5">
                    <div className="pp-card__head">
                      <span className="flex items-center gap-1">
                        <Moon className="w-3 h-3 text-[var(--acc)]" />
                        Developer Quick Switches
                      </span>
                      <em>macOS</em>
                    </div>
                    <div className="flex flex-wrap gap-1">
                      <button
                        type="button"
                        onClick={() => void handleRunQuickSwitch("toggle_dark_mode")}
                        className={`pp-btn ${extras.quick_switches.dark_mode ? "pp-btn--active" : ""}`}
                      >
                        Dark Mode
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleRunQuickSwitch("toggle_hidden_files")}
                        className={`pp-btn ${extras.quick_switches.show_hidden_files ? "pp-btn--active" : ""}`}
                      >
                        Hidden Files
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleRunQuickSwitch("toggle_desktop_icons")}
                        className={`pp-btn ${extras.quick_switches.show_desktop_icons ? "pp-btn--active" : ""}`}
                      >
                        Desktop Icons
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleRunQuickSwitch("clear_clipboard")}
                        className="pp-btn"
                      >
                        Clear Clipboard
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleRunQuickSwitch("empty_trash")}
                        className="pp-btn pp-btn--danger"
                      >
                        Empty Trash
                      </button>
                    </div>
                    {quickSwitchMsg && (
                      <div className="text-[9.5px] font-mono text-[var(--acc)]">{quickSwitchMsg}</div>
                    )}
                  </div>

                  {/* NVMe Disk Benchmark */}
                  <div className="pp-card gap-1.5">
                    <div className="pp-card__head">
                      <span className="flex items-center gap-1">
                        <Gauge className="w-3 h-3 text-[var(--acc)]" />
                        NVMe Disk Benchmark
                      </span>
                      <button
                        type="button"
                        disabled={runningDiskBench}
                        onClick={() => void handleRunDiskBenchmark()}
                        className="pp-btn pp-btn--acc disabled:opacity-50"
                      >
                        {runningDiskBench ? "Testing 64MB..." : "Run 64MB Bench"}
                      </button>
                    </div>
                    {diskBenchErr && (
                      <div className="text-[9.5px] font-mono text-rose-300">{diskBenchErr}</div>
                    )}
                    {diskBench ? (
                      <div className="pp-speed">
                        <div>
                          <span className="text-[9.5px] text-[var(--sec)]">Seq Write</span>
                          <b className="text-sm font-mono">{diskBench.write_mb_per_sec}</b>
                          <small className="text-[9px] text-[var(--sec)]">MB/s</small>
                        </div>
                        <div>
                          <span className="text-[9.5px] text-[var(--sec)]">Seq Read</span>
                          <b className="text-sm font-mono">{diskBench.read_mb_per_sec}</b>
                          <small className="text-[9px] text-[var(--sec)]">MB/s</small>
                        </div>
                        <div>
                          <span className="text-[9.5px] text-[var(--sec)]">Payload</span>
                          <b className="text-sm font-mono">{diskBench.test_size_mb}</b>
                          <small className="text-[9px] text-[var(--sec)]">MB</small>
                        </div>
                      </div>
                    ) : (
                      <div className="text-[10px] text-[var(--sec)]">
                        Measures cached sequential NVMe write &amp; read throughput in temp storage.
                      </div>
                    )}
                  </div>

                  {/* LaunchAgents Startup Persistence Auditor */}
                  <div className="pp-card gap-1.5">
                    <div className="pp-card__head">
                      <span className="flex items-center gap-1">
                        {unsignedAgentsCount > 0 ? (
                          <ShieldAlert className="w-3 h-3 text-amber-400" />
                        ) : (
                          <ShieldCheck className="w-3 h-3 text-[var(--good)]" />
                        )}
                        LaunchAgents Persistence
                      </span>
                      <button
                        type="button"
                        onClick={() => setShowLaunchAgents((v) => !v)}
                        className="pp-btn inline-flex items-center gap-1"
                      >
                        {extras.launch_agents.length} total
                        {unsignedAgentsCount > 0 ? ` · ${unsignedAgentsCount} flagged` : ""}
                        <ChevronRight
                          className={`w-2.5 h-2.5 transition-transform ${showLaunchAgents ? "rotate-90" : ""}`}
                        />
                      </button>
                    </div>
                    {showLaunchAgents && (
                      <div className="flex flex-col gap-1 max-h-48 overflow-y-auto pt-0.5">
                        {extras.launch_agents.map((a) => (
                          <div
                            key={a.plist_path}
                            className="pp-kv text-[9.5px] border-t border-white/5 pt-1"
                          >
                            <div className="min-w-0">
                              <div className="font-mono text-[var(--ink)] truncate" title={a.plist_path}>
                                {a.label}
                              </div>
                              <div className="text-[8.5px] font-mono text-[var(--sec)] truncate">
                                {a.scope} · {a.authority ?? (a.signed ? "Signed" : "Unsigned")}
                                {!a.binary_exists ? " · Missing binary" : ""}
                              </div>
                            </div>
                            <button
                              type="button"
                              onClick={() => void handleRevealInFinder(a.plist_path)}
                              className="pp-btn shrink-0"
                            >
                              Reveal
                            </button>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </>
              )}

              {sensors.length > 0 && (
                <CoreBreakdown sensors={sensors} unit={unit} defaultExpanded={true} />
              )}

              <BatteryCard battery={battery} unit={unit} />
            </>
          )}

          {activeTab === "apps" && (
            <>
              <div className="pp-search">
                <Search className="w-3.5 h-3.5 shrink-0" />
                <input
                  type="text"
                  value={appSearch}
                  onChange={(e) => setAppSearch(e.target.value)}
                  placeholder="Search apps"
                  aria-label="Search apps"
                />
                {appSearch && (
                  <button
                    type="button"
                    onClick={() => setAppSearch("")}
                    className="text-[9.5px] text-[var(--sec)] hover:text-[var(--ink)]"
                  >
                    Clear
                  </button>
                )}
              </div>

              {actionFeedback && (
                <div
                  className={`px-2.5 py-1.5 rounded-lg border text-[10px] font-mono flex items-center justify-between gap-2 ${
                    actionFeedback.kind === "err"
                      ? "bg-rose-500/10 border-rose-500/30 text-rose-200"
                      : "bg-emerald-500/10 border-emerald-500/30 text-emerald-200"
                  }`}
                >
                  <span className="truncate">{actionFeedback.text}</span>
                  <button
                    type="button"
                    onClick={() => setActionFeedback(null)}
                    className="text-[9px] underline shrink-0"
                  >
                    Dismiss
                  </button>
                </div>
              )}

              {orphanedServers.length > 0 && (
                <div className="pp-card pp-card--attn gap-1.5">
                  <div className="pp-card__head">
                    <span>Orphaned Dev Servers (ppid 1)</span>
                    <button
                      type="button"
                      onClick={() => void handleCleanupOrphans()}
                      className="pp-btn pp-btn--danger"
                    >
                      Clean All ({orphanedServers.length})
                    </button>
                  </div>
                  {orphanedServers.slice(0, 4).map((o) => (
                    <div key={o.pid} className="pp-kv text-[10px]">
                      <span className="truncate text-[var(--ter)] font-mono">
                        {o.name} (pid {o.pid}){o.port ? ` :${o.port}` : ""}
                      </span>
                      <div className="flex items-center gap-2 shrink-0">
                        <span className="font-mono text-[var(--sec)]">{formatMemory(o.memory_mb)}</span>
                        <button
                          type="button"
                          onClick={() => void handleTerminateProcess(o.pid, o.name)}
                          className="pp-btn pp-btn--danger"
                        >
                          Quit
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="flex items-center justify-between gap-2 pb-1 border-b border-[var(--card-border)]">
                <span className="text-[9.5px] font-semibold uppercase tracking-wider text-[var(--sec)]">
                  App / process
                </span>
                <div className="flex items-center gap-1 text-[9.5px] font-mono">
                  {(
                    [
                      { id: "cpu", label: "CPU" },
                      { id: "mem", label: "MEM" },
                      { id: "energy", label: "ENERGY (mW)" },
                    ] as const
                  ).map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      onClick={() => setAppSort(s.id)}
                      className={`px-1.5 py-0.5 rounded cursor-pointer ${
                        appSort === s.id
                          ? "bg-[var(--surface)] text-[var(--acc)] font-bold"
                          : "text-[var(--sec)] hover:text-[var(--ink)]"
                      }`}
                    >
                      {s.label}
                      {appSort === s.id ? " ▾" : ""}
                    </button>
                  ))}
                </div>
              </div>

              {telemetry?.top_processes.status === "unavailable" ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  Process telemetry unavailable: {telemetry.top_processes.reason}
                </div>
              ) : filteredProcesses.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  {appSearch ? `No processes matching "${appSearch}".` : "All apps idle (< 0.1% CPU)."}
                </div>
              ) : (
                <div className="flex flex-col gap-1">
                  {filteredProcesses.map((proc) => {
                    const isOpen = expandedAppName === proc.name;
                    const inspection = processInspections[proc.primary_pid];
                    const estMilliwatts =
                      packageWatts > 0
                        ? Math.round((proc.cpu_percent / totalProcCpu) * packageWatts * 1000)
                        : null;
                    return (
                      <div key={proc.name} className="flex flex-col gap-1">
                        <button
                          type="button"
                          onClick={() => {
                            const willOpen = expandedAppName !== proc.name;
                            setExpandedAppName(willOpen ? proc.name : null);
                            if (willOpen && proc.primary_pid > 1) {
                              void handleInspectProcess(proc.primary_pid);
                            }
                          }}
                          aria-expanded={isOpen}
                          className="pp-row4 text-left hover:border-[var(--border)] transition-colors cursor-pointer"
                        >
                          <span className="flex items-center gap-1 min-w-0 font-semibold text-[var(--ink)]">
                            <ChevronRight
                              className={`w-3 h-3 text-[var(--sec)] shrink-0 transition-transform ${
                                isOpen ? "rotate-90 text-[var(--acc)]" : ""
                              }`}
                            />
                            <span className="truncate">{proc.name}</span>
                          </span>
                          <b className="text-right font-mono text-[var(--acc)]">
                            {appSort === "energy" && estMilliwatts !== null
                              ? `${estMilliwatts}mW`
                              : `${proc.cpu_percent.toFixed(1)}%`}
                          </b>
                          <span className="text-right font-mono text-[var(--sec)]">
                            {formatMemory(proc.memory_mb)}
                          </span>
                          <span className="text-right font-mono text-[var(--sec)]">
                            ×{proc.process_count}
                          </span>
                        </button>

                        {isOpen && (
                          <div className="px-2.5 py-2 rounded-lg bg-black/25 border border-[var(--card-border)] flex flex-col gap-1.5 text-[10px]">
                            <div className="flex items-center justify-between gap-2">
                              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[var(--sec)] font-mono">
                                <span>
                                  PID:{" "}
                                  <b className="text-[var(--ink)]">
                                    {proc.primary_pid > 0 ? proc.primary_pid : "--"}
                                  </b>
                                </span>
                                <span>
                                  Workers: <b className="text-[var(--ink)]">{proc.process_count}</b>
                                </span>
                                {inspection && (
                                  <span
                                    className={`px-1.5 py-0.5 rounded text-[9px] border ${
                                      inspection.arch.includes("Rosetta")
                                        ? "bg-amber-500/15 border-amber-500/30 text-amber-300"
                                        : "bg-emerald-500/10 border-emerald-500/25 text-emerald-300"
                                    }`}
                                  >
                                    {inspection.arch}
                                  </span>
                                )}
                              </div>

                              <button
                                type="button"
                                disabled={proc.primary_pid <= 1}
                                onClick={() => void handleTerminateProcess(proc.primary_pid, proc.name)}
                                className="pp-btn pp-btn--danger disabled:opacity-40 shrink-0"
                              >
                                Quit
                              </button>
                            </div>

                            {inspectingPid === proc.primary_pid && !inspection && (
                              <div className="text-[9.5px] font-mono text-[var(--sec)]">
                                Inspecting signature &amp; open sockets...
                              </div>
                            )}

                            {inspection && (
                              <div className="flex flex-col gap-1 pt-1 border-t border-white/5 font-mono text-[9.5px]">
                                <div className="flex items-center justify-between gap-2 text-[var(--sec)]">
                                  <span className="truncate">
                                    Parent: <b className="text-[var(--ter)]">{inspection.parent_name}</b> (ppid{" "}
                                    {inspection.parent_pid})
                                  </span>
                                  <span className="shrink-0">CPU {inspection.cpu_time}</span>
                                </div>

                                <div className="flex flex-wrap items-center gap-1">
                                  <span
                                    className={`px-1.5 py-0.5 rounded border ${
                                      inspection.notarized
                                        ? "bg-emerald-500/10 border-emerald-500/25 text-emerald-300"
                                        : "bg-white/5 border-white/10 text-[var(--sec)]"
                                    }`}
                                  >
                                    {inspection.authority ?? "Unsigned / Ad-hoc"}
                                  </span>
                                  {inspection.team_id && (
                                    <span className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[var(--sec)]">
                                      Team {inspection.team_id}
                                    </span>
                                  )}
                                  {inspection.hardened_runtime && (
                                    <span className="px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/20 text-cyan-300">
                                      Hardened
                                    </span>
                                  )}
                                  {inspection.sandboxed && (
                                    <span className="px-1.5 py-0.5 rounded bg-purple-500/10 border border-purple-500/20 text-purple-300">
                                      Sandboxed
                                    </span>
                                  )}
                                </div>

                                {inspection.cwd && (
                                  <div className="flex items-center justify-between gap-2 text-[var(--sec)]">
                                    <span className="truncate" title={inspection.cwd}>
                                      cwd: {inspection.cwd}
                                    </span>
                                    <button
                                      type="button"
                                      onClick={() => void handleRevealInFinder(inspection.cwd!)}
                                      className="pp-btn inline-flex items-center gap-1 shrink-0"
                                    >
                                      <FolderOpen className="w-2.5 h-2.5" />
                                      Reveal
                                    </button>
                                  </div>
                                )}

                                {inspection.open_files.length > 0 && (
                                  <div className="flex flex-col gap-0.5 text-[9px] text-[var(--sec)] max-h-20 overflow-y-auto bg-black/25 p-1.5 rounded">
                                    {inspection.open_files.map((f, fi) => (
                                      <div key={fi} className="truncate">
                                        {f}
                                      </div>
                                    ))}
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </>
          )}

          {activeTab === "network" && (
            <>
              <div className="pp-pillrow">
                <span className={`pp-pill ${extras?.network.local_ip !== "Offline" ? "pp-pill--good" : "pp-pill--bad"}`}>
                  ● {extras?.network.local_ip !== "Offline" ? "Online" : "Offline"}
                </span>
                {extras?.network.vpn_active && (
                  <span className="pp-pill" style={{ background: "rgba(126, 200, 227, 0.16)", color: "#7ec8e3" }}>
                    VPN · {extras.network.vpn_ip}
                  </span>
                )}
                <span className="text-[var(--sec)] font-mono text-[10px]">
                  {extras ? `${extras.network.interface} · ${extras.network.local_ip}` : "Reading..."}
                </span>
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Wifi className="w-3 h-3" />
                    Throughput
                  </span>
                  <em>
                    {extras
                      ? `↓${formatRate(extras.network.rx_bytes_per_sec)}/s · ↑${formatRate(extras.network.tx_bytes_per_sec)}/s`
                      : "--"}
                  </em>
                </div>
                <svg className="w-full h-14 bg-[var(--surface)] rounded-md p-1" viewBox="0 0 100 14" preserveAspectRatio="none">
                  <polyline
                    fill="none"
                    stroke="var(--acc)"
                    strokeWidth="1.5"
                    vectorEffect="non-scaling-stroke"
                    points={sparkPoints(netHistory.map((n) => n.rx), 0)}
                  />
                  <polyline
                    fill="none"
                    stroke="var(--good)"
                    strokeWidth="1.1"
                    strokeOpacity="0.7"
                    vectorEffect="non-scaling-stroke"
                    points={sparkPoints(netHistory.map((n) => n.tx), 0)}
                  />
                </svg>
                {extras && (
                  <div className="pp-kv text-[10px] text-[var(--sec)]">
                    <span>Session interface total</span>
                    <b className="font-mono text-[var(--ink)]">
                      ↓ {extras.network.total_rx_gb.toFixed(2)} GB · ↑ {extras.network.total_tx_gb.toFixed(2)} GB
                    </b>
                  </div>
                )}
              </div>

              {extras && extras.network.top_net_apps.length > 0 && (
                <div className="pp-card gap-1">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Globe className="w-3 h-3 text-[var(--acc)]" />
                      Network by app (click for sockets)
                    </span>
                    <em>live nettop delta</em>
                  </div>
                  {extras.network.top_net_apps.map((app) => {
                    const isOpen = expandedNetApp === app.name;
                    const conns = appConnections[app.name] ?? [];
                    return (
                      <div key={app.name} className="flex flex-col gap-1">
                        <button
                          type="button"
                          onClick={() => void handleInspectNetApp(app.name)}
                          className="pp-row3 text-[10px] w-full text-left hover:bg-white/5 rounded px-1 py-0.5 cursor-pointer"
                        >
                          <span className="truncate text-[var(--ink)] font-medium">{app.name}</span>
                          <span className="text-right font-mono text-[var(--acc)]">
                            ↓{formatRate(app.rx_bytes_per_sec)}/s
                          </span>
                          <span className="text-right font-mono text-[var(--good)]">
                            ↑{formatRate(app.tx_bytes_per_sec)}/s
                          </span>
                        </button>

                        {isOpen && (
                          <div className="px-2 py-1.5 rounded bg-black/25 border border-white/5 font-mono text-[9px] text-[var(--sec)] max-h-24 overflow-y-auto">
                            {loadingConnApp === app.name ? (
                              <div>Inspecting active connections...</div>
                            ) : conns.length === 0 ? (
                              <div>No open TCP/UDP sockets reported.</div>
                            ) : (
                              conns.map((c, ci) => (
                                <div key={ci} className="flex items-center justify-between gap-2 truncate">
                                  <span className="truncate text-[var(--ter)]">
                                    {c.protocol} {c.remote_endpoint}
                                  </span>
                                  <span className="text-[var(--acc)] shrink-0">{c.state}</span>
                                </div>
                              ))
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span>Latency (1.1.1.1)</span>
                  <em>ICMP probe</em>
                </div>
                <div className="pp-kv">
                  <span>
                    Latency{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.latency_ms !== null && extras?.network.latency_ms !== undefined
                        ? `${extras.network.latency_ms.toFixed(1)} ms`
                        : "--"}
                    </b>
                  </span>
                  <span>
                    Loss{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.packet_loss_percent !== null &&
                      extras?.network.packet_loss_percent !== undefined
                        ? `${extras.network.packet_loss_percent.toFixed(0)}%`
                        : "--"}
                    </b>
                  </span>
                  <span>
                    Jitter{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.jitter_ms !== null && extras?.network.jitter_ms !== undefined
                        ? `${extras.network.jitter_ms.toFixed(1)} ms`
                        : "--"}
                    </b>
                  </span>
                </div>
              </div>

              <div className="pp-card gap-2">
                <div className="pp-card__head">
                  <span>Speed test</span>
                  <button
                    type="button"
                    disabled={runningSpeedTest}
                    onClick={() => void handleRunSpeedTest()}
                    className="pp-btn pp-btn--acc disabled:opacity-50"
                  >
                    {runningSpeedTest ? "Testing..." : "Run networkQuality"}
                  </button>
                </div>

                {speedTestError && (
                  <div className="text-[10px] text-rose-300 font-mono">{speedTestError}</div>
                )}

                {speedTest ? (
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Download</span>
                      <b className="text-sm font-mono">{speedTest.download_mbps}</b>
                      <small className="text-[9px] text-[var(--sec)]">Mbps</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Upload</span>
                      <b className="text-sm font-mono">{speedTest.upload_mbps}</b>
                      <small className="text-[9px] text-[var(--sec)]">Mbps</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Responsive</span>
                      <b className="text-sm font-mono">{speedTest.responsiveness_rpm}</b>
                      <small className="text-[9px] text-[var(--sec)]">RPM</small>
                    </div>
                  </div>
                ) : (
                  <div className="text-[10px] text-[var(--sec)]">
                    Runs Apple&apos;s native <code className="text-[var(--ter)]">networkQuality</code> benchmark on demand.
                  </div>
                )}
              </div>

              {extras && (extras.network.wifi.channel || extras.network.wifi.rssi_dbm !== null) && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Wifi className="w-3 h-3 text-[var(--acc)]" />
                      Wi-Fi Radio
                    </span>
                    <em className="font-mono">
                      {extras.network.wifi.phy_mode || "Wi-Fi"} · Ch {extras.network.wifi.channel || "--"}
                    </em>
                  </div>
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">RSSI</span>
                      <b className="text-xs font-mono">{extras.network.wifi.rssi_dbm ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">dBm</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">SNR</span>
                      <b className="text-xs font-mono">{extras.network.wifi.snr_db ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        dB ({extras.network.wifi.noise_dbm ?? "--"})
                      </small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Tx Rate</span>
                      <b className="text-xs font-mono">{extras.network.wifi.tx_rate_mbps ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        Mbps{extras.network.wifi.mcs_index !== null ? ` · MCS${extras.network.wifi.mcs_index}` : ""}
                      </small>
                    </div>
                  </div>
                  {extras.network.wifi.security && (
                    <div className="text-[9.5px] font-mono text-[var(--sec)]">
                      Security: <span className="text-[var(--ter)]">{extras.network.wifi.security}</span>
                    </div>
                  )}
                </div>
              )}

              {extras && extras.network.tailscale.installed && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Globe className="w-3 h-3 text-[var(--acc)]" />
                      Tailscale Mesh
                    </span>
                    <em className={extras.network.tailscale.running ? "text-[var(--good)]" : ""}>
                      {extras.network.tailscale.running
                        ? `Running · ${extras.network.tailscale.online_peers} online peer(s)`
                        : "Stopped"}
                    </em>
                  </div>
                  <div className="pp-kv text-[10px]">
                    <span className="text-[var(--ter)] truncate">
                      {extras.network.tailscale.dns_name ?? extras.network.tailscale.host_name ?? "Tailnet"}
                    </span>
                    <b className="font-mono text-[var(--ink)]">
                      {extras.network.tailscale.tailscale_ip ?? "--"}
                    </b>
                  </div>
                </div>
              )}

              {extras && extras.network.ssh_tunnels.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Terminal className="w-3 h-3 text-[var(--acc)]" />
                      Active SSH Tunnels
                    </span>
                    <em>{extras.network.ssh_tunnels.length}</em>
                  </div>
                  {extras.network.ssh_tunnels.map((t) => (
                    <div key={t.pid} className="pp-kv text-[10px]">
                      <span className="truncate font-mono text-[var(--ter)]" title={`${t.kind} ${t.spec} ${t.target}`}>
                        <b className="text-[var(--acc)]">{t.kind}</b> {t.spec} → {t.target || `pid ${t.pid}`}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleTerminateProcess(t.pid, `SSH ${t.kind} ${t.spec}`)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Disconnect
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {extras && extras.network.custom_hosts.length > 0 && (
                <div className="pp-card gap-1">
                  <div className="pp-card__head">
                    <span>Custom /etc/hosts Overrides</span>
                    <em>{extras.network.custom_hosts.length}</em>
                  </div>
                  <div className="flex flex-col gap-0.5 max-h-24 overflow-y-auto font-mono text-[9.5px] text-[var(--ter)]">
                    {extras.network.custom_hosts.map((line, idx) => (
                      <div key={idx} className="truncate">
                        {line}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}

          {activeTab === "devices" && (
            <>
              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <BatteryCharging className="w-3 h-3" />
                    Charger &amp; Power
                  </span>
                  <button
                    type="button"
                    onClick={() => void refreshDevices()}
                    className="inline-flex items-center gap-1 text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    <RefreshCw className={`w-2.5 h-2.5 ${loadingDevices ? "animate-spin" : ""}`} />
                    Refresh
                  </button>
                </div>

                {devices?.charger.connected ? (
                  <div className="pp-kv">
                    <span className="text-[var(--ter)]">
                      {devices.charger.description ?? "USB-C Power Adapter"}
                    </span>
                    <b className="font-mono text-[var(--ink)]">
                      {devices.charger.watts ?? "--"}W
                      {devices.charger.voltage_v ? ` · ${devices.charger.voltage_v}V` : ""}
                      {devices.charger.current_a ? ` · ${devices.charger.current_a}A` : ""}
                    </b>
                  </div>
                ) : (
                  <div className="text-[10px] text-[var(--sec)]">Running on battery (no AC adapter).</div>
                )}

                {battery && (
                  <div className="pp-kv text-[10px]">
                    <span className="text-[var(--sec)]">System draw / battery</span>
                    <b className="font-mono">
                      {battery.power_watts !== null ? `${battery.power_watts.toFixed(1)}W` : "--"} ·{" "}
                      {battery.charge_percent ?? "--"}% ({battery.cycle_count ?? "--"} cyc)
                    </b>
                  </div>
                )}
              </div>

              {devices && devices.battery_health.design_capacity_mah !== null && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <BatteryCharging className="w-3 h-3 text-[var(--acc)]" />
                      Battery Health Vitals
                    </span>
                    <em className="font-mono text-[var(--good)]">
                      {devices.battery_health.health_percent ?? "--"}% Health
                    </em>
                  </div>
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Full Max</span>
                      <b className="text-xs font-mono">
                        {devices.battery_health.raw_max_capacity_mah ??
                          devices.battery_health.nominal_capacity_mah ??
                          "--"}
                      </b>
                      <small className="text-[9px] text-[var(--sec)]">
                        / {devices.battery_health.design_capacity_mah} mAh
                      </small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Voltage</span>
                      <b className="text-xs font-mono">{devices.battery_health.voltage_v ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">V</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Current</span>
                      <b className="text-xs font-mono">{devices.battery_health.amperage_ma ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        mA
                        {devices.battery_health.time_remaining_min
                          ? ` · ${Math.floor(devices.battery_health.time_remaining_min / 60)}h${devices.battery_health.time_remaining_min % 60}m`
                          : ""}
                      </small>
                    </div>
                  </div>
                </div>
              )}

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Bluetooth className="w-3 h-3" />
                    Bluetooth
                  </span>
                  <em>{devices?.bluetooth.filter((b) => b.connected).length ?? 0} connected</em>
                </div>

                {!devices || devices.bluetooth.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">No Bluetooth devices reported.</div>
                ) : (
                  <div className="flex flex-col gap-1">
                    {devices.bluetooth.slice(0, 8).map((bt, idx) => (
                      <div key={`${bt.name}-${idx}`} className="pp-kv">
                        <span className="flex items-center gap-1.5 min-w-0 truncate">
                          <span
                            className={`pp-dot ${bt.connected ? "pp-dot--good" : ""}`}
                            style={!bt.connected ? { background: "#52555d" } : undefined}
                          />
                          <span className="truncate text-[var(--ink)]">{bt.name}</span>
                          <span className="text-[9.5px] text-[var(--sec)]">{bt.device_type}</span>
                        </span>
                        <b className="font-mono text-[var(--ter)] shrink-0">
                          {bt.battery_percent !== null
                            ? `${bt.battery_percent}%`
                            : bt.connected
                              ? "Connected"
                              : bt.rssi !== null
                                ? `${bt.rssi} dBm`
                                : "Paired"}
                        </b>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Usb className="w-3 h-3" />
                    USB Tree
                  </span>
                  <em>{devices?.usb.length ?? 0} devices</em>
                </div>

                {!devices || devices.usb.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">No external USB devices attached.</div>
                ) : (
                  <div className="flex flex-col gap-1 font-mono text-[10px]">
                    {devices.usb.map((u, idx) => (
                      <div
                        key={`${u.name}-${idx}`}
                        className="truncate text-[var(--ter)]"
                        style={{ paddingLeft: `${Math.min(u.depth, 4) * 10}px` }}
                      >
                        ↳ {u.name}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}

          {activeTab === "volume" && (
            <>
              {extras?.audio.camera_in_use && (
                <div className="pp-card pp-card--attn">
                  <div className="pp-attn">
                    <Camera className="w-3.5 h-3.5 text-rose-400 shrink-0" />
                    <span className="text-[var(--ink)]">
                      <b>Webcam is currently active</b> or streaming
                    </span>
                  </div>
                </div>
              )}

              {extras?.audio.mic_in_use_by && (
                <div className="pp-card pp-card--attn">
                  <div className="pp-attn">
                    <Mic className="w-3.5 h-3.5 text-[var(--acc)] shrink-0" />
                    <span className="text-[var(--ink)]">
                      Mic in use by <b>{extras.audio.mic_in_use_by}</b>
                    </span>
                  </div>
                </div>
              )}

              <div className="pp-card gap-2.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    {extras?.audio.output_muted ? (
                      <VolumeX className="w-3.5 h-3.5 text-rose-400" />
                    ) : (
                      <Volume2 className="w-3.5 h-3.5 text-[var(--acc)]" />
                    )}
                    Output Volume
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      void handleUpdateAudio(null, null, !(extras?.audio.output_muted ?? false))
                    }
                    className={`pp-btn ${extras?.audio.output_muted ? "pp-btn--danger" : ""}`}
                  >
                    {extras?.audio.output_muted ? "Muted" : "Mute"}
                  </button>
                </div>

                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={extras?.audio.output_volume ?? 50}
                    onChange={(e) => void handleUpdateAudio(Number(e.target.value), null, null)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-cyan-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras?.audio.output_volume ?? "--"}%
                  </b>
                </div>
              </div>

              <div className="pp-card gap-2.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Mic className="w-3.5 h-3.5 text-[var(--good)]" />
                    Input Gain
                  </span>
                  <em>{extras?.audio.input_volume ?? "--"}%</em>
                </div>

                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={extras?.audio.input_volume ?? 50}
                    onChange={(e) => void handleUpdateAudio(null, Number(e.target.value), null)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-emerald-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras?.audio.input_volume ?? "--"}%
                  </b>
                </div>
              </div>
            </>
          )}

          {activeTab === "ports" && (
            <>
              <div className="flex items-center gap-1.5">
                <div className="pp-search flex-1">
                  <Search className="w-3.5 h-3.5 shrink-0" />
                  <input
                    type="text"
                    value={portSearch}
                    onChange={(e) => setPortSearch(e.target.value)}
                    placeholder="Filter port, process, or bind"
                    aria-label="Filter listening ports"
                  />
                </div>
                <button
                  type="button"
                  disabled={probingPorts || filteredPorts.length === 0}
                  onClick={() => void handleProbeHttpPorts()}
                  className="pp-btn py-1.5 px-2 flex items-center gap-1 disabled:opacity-40"
                  title="Probe HTTP status & <title> on listening localhost ports"
                >
                  <Globe className={`w-3 h-3 ${probingPorts ? "animate-spin" : ""}`} />
                  HTTP
                </button>
                <button
                  type="button"
                  onClick={() => void refreshPorts()}
                  className="pp-btn py-1.5 px-2 flex items-center gap-1"
                  title="Refresh listening ports"
                >
                  <RefreshCw className="w-3 h-3" />
                </button>
              </div>

              {portsError && (
                <div className="px-2.5 py-1.5 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  {portsError}
                </div>
              )}

              {exposedPorts.length > 0 && (
                <div className="pp-card pp-card--attn text-[10px]">
                  <div className="pp-attn">
                    <span className="pp-dot pp-dot--warn" />
                    <span className="text-[var(--ter)]">
                      <b className="text-[var(--ink)]">{exposedPorts.length} port{exposedPorts.length > 1 ? "s" : ""}</b>{" "}
                      bound to all interfaces (<code className="text-[var(--acc)]">0.0.0.0 / *</code>) — reachable on your LAN.
                    </span>
                  </div>
                </div>
              )}

              {projectStacks.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span>Project Stacks (by workspace)</span>
                    <em>{projectStacks.length} active</em>
                  </div>
                  {projectStacks.slice(0, 4).map((stack) => (
                    <div key={stack.path} className="pp-kv text-[10px]">
                      <div className="min-w-0">
                        <div className="font-mono text-[var(--ink)] truncate" title={stack.path}>
                          {stack.path.split("/").slice(-2).join("/")}
                        </div>
                        <div className="font-mono text-[9px] text-[var(--acc)]">
                          {stack.ports.map((p) => `:${p.port}`).join(" ")}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => void handleStopProjectStack(stack.path, stack.pids)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Stop Stack
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {filteredPorts.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  {portSearch
                    ? `No listening TCP ports matching "${portSearch}".`
                    : "No listening TCP ports found."}
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {filteredPorts.map((item) => (
                    <div key={`${item.port}-${item.pid}`} className="pp-card gap-1">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-1.5 min-w-0">
                          <b className="font-mono text-xs text-[var(--acc)]">:{item.port}</b>
                          <span className="font-semibold text-[var(--ink)] truncate">
                            {item.process_name}
                          </span>
                          <span className="text-[9.5px] font-mono text-[var(--sec)]">
                            pid {item.pid}
                          </span>
                          {item.launcher && (
                            <span className="px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/20 text-[9px] text-cyan-300 shrink-0">
                              {item.launcher}
                            </span>
                          )}
                          {httpProbes[item.port]?.status_code && (
                            <span
                              className="px-1.5 py-0.5 rounded bg-emerald-500/10 border border-emerald-500/25 text-[9px] font-mono text-emerald-300 truncate max-w-[140px]"
                              title={httpProbes[item.port].title ?? undefined}
                            >
                              {httpProbes[item.port].status_code}
                              {httpProbes[item.port].title ? ` · ${httpProbes[item.port].title}` : ""}
                            </span>
                          )}
                        </div>

                        <span
                          className={`px-1.5 py-0.5 rounded text-[9px] font-mono shrink-0 ${
                            item.is_exposed
                              ? "bg-amber-500/15 text-amber-300 border border-amber-500/30"
                              : "bg-white/5 text-[var(--sec)] border border-white/10"
                          }`}
                        >
                          {item.is_exposed ? `${item.bind_address} · exposed` : item.bind_address}
                        </span>
                      </div>

                      {item.project_path && (
                        <div
                          className="text-[9.5px] font-mono text-[var(--sec)] truncate"
                          title={item.project_path}
                        >
                          📁 {item.project_path}
                        </div>
                      )}

                      <div className="flex items-center justify-end gap-1 pt-0.5">
                        {item.project_path && (
                          <button
                            type="button"
                            onClick={() => void handleRevealInFinder(item.project_path!)}
                            className="pp-btn inline-flex items-center gap-1"
                            title="Reveal project folder in Finder"
                          >
                            <FolderOpen className="w-2.5 h-2.5" />
                            Folder
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => void openUrl(`http://localhost:${item.port}`)}
                          className="pp-btn inline-flex items-center gap-1"
                        >
                          <ExternalLink className="w-2.5 h-2.5" />
                          Open
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleCopyPortUrl(item.port)}
                          className="pp-btn inline-flex items-center gap-1"
                        >
                          {copiedPort === item.port ? (
                            <>
                              <Check className="w-2.5 h-2.5 text-[var(--good)]" />
                              Copied
                            </>
                          ) : (
                            <>
                              <Copy className="w-2.5 h-2.5" />
                              Copy
                            </>
                          )}
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void handleTerminateProcess(item.pid, `${item.process_name} :${item.port}`)
                          }
                          className="pp-btn pp-btn--danger inline-flex items-center gap-1"
                        >
                          <Square className="w-2.5 h-2.5" />
                          Stop
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {activeTab === "agents" && (
            <>
              <div className="pp-card gap-1">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Sparkles className="w-3 h-3 text-[var(--acc)]" />
                    AI Agent Sessions
                  </span>
                  <em>{agents.length} running</em>
                </div>
                <p className="text-[10px] text-[var(--sec)]">
                  Tracks Claude Code, Codex, Cursor, Antigravity, and Ollama sessions along with child processes and opened ports.
                </p>
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Terminal className="w-3 h-3 text-[var(--acc)]" />
                    Built-in CLI &amp; MCP Server
                  </span>
                  <button
                    type="button"
                    onClick={() => void handleCopyMcpConfig()}
                    className="pp-btn inline-flex items-center gap-1"
                  >
                    {copiedMcpConfig ? (
                      <>
                        <Check className="w-2.5 h-2.5 text-[var(--good)]" />
                        Copied
                      </>
                    ) : (
                      <>
                        <Copy className="w-2.5 h-2.5" />
                        Copy MCP Config
                      </>
                    )}
                  </button>
                </div>
                <div className="text-[9.5px] font-mono text-[var(--sec)] leading-snug">
                  Run <code className="text-[var(--ter)]">statpulse --mcp</code> to expose{" "}
                  <code className="text-[var(--acc)]">get_system_status</code>,{" "}
                  <code className="text-[var(--acc)]">list_ports</code>,{" "}
                  <code className="text-[var(--acc)]">kill_port</code> &amp;{" "}
                  <code className="text-[var(--acc)]">unload_ollama_model</code>.
                </div>
              </div>

              {/* Local LLM / Ollama VRAM Unloader */}
              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Cpu className="w-3 h-3 text-[var(--acc)]" />
                    Local LLM &amp; Ollama VRAM
                  </span>
                  <em>{extras?.ollama_models.length ?? 0} loaded</em>
                </div>
                {!extras || extras.ollama_models.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">
                    No local LLM models currently resident in Unified Memory.
                  </div>
                ) : (
                  <div className="flex flex-col gap-1">
                    {extras.ollama_models.map((m) => (
                      <div key={m.id} className="pp-kv text-[10px]">
                        <div className="min-w-0">
                          <div className="font-mono font-semibold text-[var(--ink)] truncate">{m.name}</div>
                          <div className="text-[9px] text-[var(--sec)] font-mono">
                            {m.size_gb.toFixed(1)} GB · {m.processor} · {m.until}
                          </div>
                        </div>
                        <button
                          type="button"
                          disabled={unloadingModel === m.name}
                          onClick={() => void handleUnloadOllama(m.name)}
                          className="pp-btn pp-btn--danger shrink-0"
                        >
                          {unloadingModel === m.name ? "..." : "Unload"}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Docker / OrbStack Containers */}
              {extras && extras.containers.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Box className="w-3 h-3 text-[var(--acc)]" />
                      Containers ({extras.containers[0]?.runtime})
                    </span>
                    <em>{extras.containers.length} running</em>
                  </div>
                  <div className="flex flex-col gap-1">
                    {extras.containers.slice(0, 6).map((c) => (
                      <div key={c.id} className="pp-kv text-[10px]">
                        <div className="min-w-0">
                          <div className="font-mono font-semibold text-[var(--ink)] truncate">{c.name}</div>
                          <div className="text-[9px] text-[var(--sec)] truncate">
                            {c.image} · {c.status}
                          </div>
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          <button
                            type="button"
                            disabled={controllingCtr === c.id}
                            onClick={() => void handleControlContainer(c.id, "restart")}
                            className="pp-btn inline-flex items-center"
                            title="Restart container"
                          >
                            <RotateCw className="w-2.5 h-2.5" />
                          </button>
                          <button
                            type="button"
                            disabled={controllingCtr === c.id}
                            onClick={() => void handleControlContainer(c.id, "stop")}
                            className="pp-btn pp-btn--danger"
                          >
                            Stop
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {agents.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  No active AI coding agent processes detected right now.
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {agents.map((ag) => (
                    <div key={ag.agent_name} className="pp-card gap-1.5">
                      <div className="flex items-center justify-between gap-2">
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5">
                            <b className="text-xs text-[var(--ink)]">{ag.agent_name}</b>
                            <span className="text-[9.5px] font-mono text-[var(--sec)]">
                              up {ag.uptime} · {ag.process_count} proc
                            </span>
                          </div>
                          {ag.children.length > 0 && (
                            <div className="text-[9.5px] font-mono text-[var(--sec)] truncate mt-0.5">
                              Children: {ag.children.join(", ")}
                            </div>
                          )}
                        </div>

                        <div className="text-right font-mono shrink-0">
                          <div className="text-[11px] font-bold text-[var(--acc)]">
                            {ag.cpu_percent.toFixed(1)}%
                          </div>
                          <div className="text-[9.5px] text-[var(--sec)]">
                            {formatMemory(ag.memory_mb)}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center justify-between gap-2 pt-0.5">
                        <div className="flex items-center gap-1 flex-wrap">
                          {ag.ports.map((p) => (
                            <button
                              key={p}
                              type="button"
                              onClick={() => setActiveTab("ports")}
                              className="px-1.5 py-0.5 rounded bg-[var(--surface)] border border-[var(--card-border)] font-mono text-[9.5px] text-[var(--acc)]"
                            >
                              :{p}
                            </button>
                          ))}
                        </div>
                        <button
                          type="button"
                          disabled={ag.primary_pid <= 1}
                          onClick={() => void handleTerminateProcess(ag.primary_pid, ag.agent_name)}
                          className="pp-btn pp-btn--danger shrink-0 disabled:opacity-40"
                        >
                          Stop Session
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {activeTab === "awake" && (
            <>
              {awakeError && (
                <div className="px-2.5 py-1.5 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  {awakeError}
                </div>
              )}

              <div className="pp-card gap-2">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1.5">
                    <Coffee className="w-3.5 h-3.5 text-[var(--acc)]" />
                    Keep awake
                  </span>
                  <em>
                    {awakeStatus.keep_awake_active
                      ? awakeStatus.remaining_seconds !== null
                        ? `On · ${formatDuration(awakeStatus.remaining_seconds)} left`
                        : "On · Always"
                      : "Off"}
                  </em>
                </div>

                <div className="grid grid-cols-5 gap-1">
                  {[
                    { label: "30m", secs: 1800 },
                    { label: "1h", secs: 3600 },
                    { label: "2h", secs: 7200 },
                    { label: "Always", secs: null },
                  ].map((opt) => (
                    <button
                      key={opt.label}
                      type="button"
                      onClick={() => void handleSetKeepAwake(true, opt.secs)}
                      className={`pp-btn py-1 ${
                        awakeStatus.keep_awake_active &&
                        ((opt.secs === null && awakeStatus.remaining_seconds === null) ||
                          (opt.secs !== null && awakeStatus.remaining_seconds !== null))
                          ? "pp-btn--active"
                          : ""
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                  <button
                    type="button"
                    onClick={() => void handleSetKeepAwake(false, null)}
                    className={`pp-btn py-1 ${!awakeStatus.keep_awake_active ? "pp-btn--active" : ""}`}
                  >
                    Off
                  </button>
                </div>

                <div className="flex items-center justify-between gap-2 pt-1 border-t border-white/5">
                  <span className="text-[10px] text-[var(--sec)]">
                    <Activity className="w-3 h-3 inline mr-1 text-[var(--acc)]" />
                    {awakeStatus.auto_rule_reason ?? "Smart Auto Rules"}
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_on_ac")}
                      className={`pp-btn ${awakeStatus.rules.while_on_ac ? "pp-btn--active" : ""}`}
                    >
                      On AC
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_ports_active")}
                      className={`pp-btn ${awakeStatus.rules.while_ports_active ? "pp-btn--active" : ""}`}
                    >
                      Dev Ports
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_ai_agents_active")}
                      className={`pp-btn ${awakeStatus.rules.while_ai_agents_active ? "pp-btn--active" : ""}`}
                    >
                      AI Agents
                    </button>
                  </div>
                </div>

                <p className="text-[10px] text-[var(--sec)]">
                  Uses macOS <code className="text-[var(--ter)]">caffeinate -di</code> to prevent idle system &amp; display sleep while StatPulse is running.
                </p>
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span>Why won&apos;t my Mac sleep?</span>
                  <button
                    type="button"
                    onClick={() => void refreshAwake()}
                    className="inline-flex items-center gap-1 text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    <RefreshCw className="w-2.5 h-2.5" />
                    {awakeStatus.assertions.length} active
                  </button>
                </div>

                {awakeStatus.assertions.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)] py-1">
                    No active power assertions blocking sleep.
                  </div>
                ) : (
                  <div className="flex flex-col gap-1.5">
                    {awakeStatus.assertions.map((a, idx) => (
                      <div
                        key={`${a.pid}-${idx}`}
                        className="px-2.5 py-1.5 rounded-lg bg-[var(--surface)] border border-[var(--card-border)] flex flex-col gap-0.5"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="font-semibold text-[var(--ink)] truncate">
                            {a.process_name}{" "}
                            <span className="text-[9.5px] font-mono text-[var(--sec)]">
                              pid {a.pid}
                            </span>
                          </span>
                          <span className="text-[9px] font-mono text-[var(--acc)] shrink-0">
                            {a.assertion_type}
                          </span>
                        </div>
                        <span className="text-[10px] text-[var(--sec)] truncate" title={a.reason}>
                          {a.reason}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}

          {activeTab === "design" && (
            <>
              {preferencesError && (
                <div className="px-3 py-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  Application preferences update failed: {preferencesError}
                </div>
              )}
              <DesignStudioModal
                preferences={preferences}
                telemetry={telemetry}
                history={tempHistory}
                onUpdatePreferences={handleUpdatePreferences}
              />
            </>
          )}

          {activeTab === "settings" && (
            <>
              {preferencesError && (
                <div className="px-3 py-2 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  Application preferences update failed: {preferencesError}
                </div>
              )}
              <SettingsModal
                preferences={preferences}
                fanActuationStatus={fanActuationStatus}
                onUpdatePreferences={handleUpdatePreferences}
                onHelperInstalled={async () => {
                  const updated = await invoke<HardwareTelemetrySnapshot>("fetch_telemetry");
                  recordSnapshot(updated);
                }}
                onBundleImported={handleBundleImported}
                onShowUpdateModal={(release) => {
                  prepareUpdate(release);
                  setShowUpdateModal(true);
                }}
              />
            </>
          )}
        </div>

            <div className="pp-foot">
              <span className="flex items-center gap-1 shrink-0">
                <ShieldCheck className={`w-3 h-3 ${hasSmcAccess ? "text-[var(--good)]" : "text-[var(--bad)]"}`} />
                SMC: {telemetry === null ? "Waiting" : hasSmcAccess ? "OK" : "Limited"}
              </span>

              {extras && (
                <span className="font-mono text-[9.5px] text-[var(--sec)] truncate">
                  self {extras.self_cost.cpu_percent.toFixed(1)}% · {Math.round(extras.self_cost.memory_mb)} MB
                </span>
              )}

              {hasUpdateAvailable && updateRelease ? (
                <button
                  type="button"
                  onClick={() => setShowUpdateModal(true)}
                  className="flex items-center gap-1 font-mono text-[var(--warn)] font-bold hover:underline shrink-0"
                >
                  <Sparkles className="w-3 h-3 animate-pulse" />
                  v{updateRelease.version}
                </button>
              ) : (
                <span className="font-mono shrink-0">v{CURRENT_VERSION}</span>
              )}
            </div>
          </>
        )}
        </div>
      </div>

      {showUpdateModal && updateRelease && (
        <UpdateModal
          currentVersion={CURRENT_VERSION}
          release={updateRelease}
          updateReady={updateReady}
          preparationError={updatePreparationError}
          onClose={() => setShowUpdateModal(false)}
        />
      )}

      {cmdOpen && (
        <div
          className="fixed inset-0 z-50 bg-black/65 backdrop-blur-sm flex items-start justify-center pt-12 p-3"
          onClick={() => setCmdOpen(false)}
        >
          <div
            className="w-full max-w-[360px] rounded-xl border border-[var(--border)] bg-[var(--bg)] shadow-2xl overflow-hidden flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="pp-search rounded-none border-0 border-b border-[var(--card-border)] px-3 py-2">
              <Search className="w-3.5 h-3.5 shrink-0 text-[var(--acc)]" />
              <input
                type="text"
                autoFocus
                value={cmdQuery}
                onChange={(e) => setCmdQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && commandItems[0]) {
                    commandItems[0].run();
                    setCmdOpen(false);
                  }
                }}
                placeholder="Type command, theme, policy, or :port..."
                aria-label="Quick command palette"
              />
            </div>

            <div className="max-h-72 overflow-y-auto p-1.5 flex flex-col gap-0.5">
              {commandItems.length === 0 ? (
                <div className="text-[10px] text-[var(--sec)] px-2 py-3 text-center">
                  No matching commands.
                </div>
              ) : (
                commandItems.slice(0, 14).map((cmd) => (
                  <button
                    key={cmd.id}
                    type="button"
                    onClick={() => {
                      cmd.run();
                      setCmdOpen(false);
                    }}
                    className="w-full px-2.5 py-1.5 rounded-lg text-left flex items-center justify-between gap-2 text-[10.5px] hover:bg-[var(--surface)] text-[var(--ink)] cursor-pointer transition-colors"
                  >
                    <span className="truncate">{cmd.label}</span>
                    <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-[var(--rail)] text-[var(--sec)] border border-[var(--card-border)] shrink-0">
                      {cmd.category}
                    </span>
                  </button>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
