import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Header, NavigationRail } from "./components/Header";
import { SettingsModal } from "./components/SettingsModal";
import { DesignStudioModal } from "./components/DesignStudioModal";
import { UpdateModal } from "./components/UpdateModal";
import { CURRENT_VERSION } from "./services/updater";
import { AppTab, TABS } from "./navigation";
import { POLICY_MODES } from "./features/thermals/policyModes";
import { PALETTE_LOOK_IDS, lookName } from "./features/looks";
import { formatRate } from "./services/format";
import { displayTemperature } from "./features/thermals/display";
import { useTelemetryHistory } from "./features/thermals/useTelemetryHistory";
import { useAppUpdate } from "./features/updates/useAppUpdate";
import { useProcessList } from "./features/processes/useProcessList";
import { usePortList } from "./features/ports/usePortList";
import { useNetworkTools } from "./features/network/useNetworkTools";
import { useSystemTools } from "./features/system/useSystemTools";
import { useAgentTools } from "./features/agents/useAgentTools";
import { HomeView } from "./features/overview/HomeView";
import { FansView } from "./features/thermals/FansView";
import { SystemView } from "./features/system/SystemView";
import { AppsView } from "./features/processes/AppsView";
import { NetworkView } from "./features/network/NetworkView";
import { DevicesView } from "./features/devices/DevicesView";
import { VolumeView } from "./features/audio/VolumeView";
import { PortsView } from "./features/ports/PortsView";
import { AgentsView } from "./features/agents/AgentsView";
import { AwakeView } from "./features/awake/AwakeView";
import {
  ApplicationPreferenceChange,
  ApplicationPreferences,
  AwakeStatus,
  DevicesSnapshot,
  HardwareTelemetrySnapshot,
  KeepAwakeRules,
  LayoutDensity,
  SystemExtrasSnapshot,
  ThermalPolicyMode,
  ThermalPolicySettings,
  ThermalRule,
  BatteryReading,
  FanReading,
  ProcessReading,
  TemperatureReading,
} from "./types";
import {
  ChevronUp,
  Command,
  Search,
  ShieldCheck,
  Sparkles,
  Wind,
} from "lucide-react";

function App() {
  const [extras, setExtras] = useState<SystemExtrasSnapshot | null>(null);
  const [netHistory, setNetHistory] = useState<{ rx: number; tx: number }[]>([]);
  const [memHistory, setMemHistory] = useState<number[]>([]);
  const [devices, setDevices] = useState<DevicesSnapshot | null>(null);
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [showHealthReport, setShowHealthReport] = useState(false);

  const [cmdOpen, setCmdOpen] = useState(false);
  const [cmdQuery, setCmdQuery] = useState("");

  const [thermalPolicy, setThermalPolicy] = useState<ThermalPolicySettings>({
    mode: "system_auto",
    rules: [],
  });
  const [activeTab, setActiveTab] = useState<AppTab>("home");

  const [islandCollapsed, setIslandCollapsed] = useState(false);
  const [islandPinned, setIslandPinned] = useState(false);
  const [islandCollapsing, setIslandCollapsing] = useState(false);
  const hoverTimeoutRef = useRef<number | null>(null);
  const collapseAnimRef = useRef<number | null>(null);
  const justCollapsedAtRef = useRef<number>(0);
  const cmdOpenRef = useRef(false);
  const dismissIslandRef = useRef<() => void>(() => {});

  const [awakeStatus, setAwakeStatus] = useState<AwakeStatus>({
    keep_awake_active: false,
    remaining_seconds: null,
    assertions: [],
    rules: { while_on_ac: false, while_ports_active: false, while_ai_agents_active: false },
    auto_rule_reason: null,
  });
  const [awakeError, setAwakeError] = useState<string | null>(null);

  const [policyError, setPolicyError] = useState<string | null>(null);
  const [dustBlastSecs, setDustBlastSecs] = useState<number>(0);
  const [preferences, setPreferences] = useState<ApplicationPreferences>({
    temperature_unit: "celsius",
    telemetry_interval_ms: 1500,
    popover_look: "liquid_glass",
    layout_density: "comfortable",
    menu_bar_style: "compact_text",
    chart_palette: "default",
    traffic_light_menu_bar: false,
    launch_at_login: false,
  });
  const [preferencesError, setPreferencesError] = useState<string | null>(null);
  const prefsGenerationRef = useRef(0);

  const update = useAppUpdate();

  const isDynamicIsland = preferences.layout_density === "dynamic_island";
  const isCollapsedPill = isDynamicIsland && islandCollapsed;

  const {
    telemetry,
    windowedHistory,
    historyWindow,
    setHistoryWindow,
    recordSnapshot,
    recordedCount,
    toggleRecording,
    sessionExportMsg,
  } = useTelemetryHistory(extras?.memory.used_gb ?? null, isCollapsedPill);

  const extrasInFlightRef = useRef(false);
  const pillInFlightRef = useRef(false);
  const pendingFullExtrasRef = useRef(false);
  // Whether the dashboard (rather than the mini pill) owns extras data; a late
  // pill reply must never overwrite expanded full-history state.
  const expandedExtrasRef = useRef(true);
  const islandCollapsedRef = useRef(false);
  const isDynamicIslandRef = useRef(false);

  const refreshExtras = useCallback(async () => {
    if (pillInFlightRef.current) {
      // A pill fetch is outstanding; run at most one full refresh after it
      // settles instead of dropping the expanded request forever.
      pendingFullExtrasRef.current = true;
      return;
    }
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
    if (extrasInFlightRef.current || pillInFlightRef.current) return;
    pillInFlightRef.current = true;
    try {
      const snap = await invoke<SystemExtrasSnapshot>("fetch_pill_extras");
      if (!expandedExtrasRef.current) {
        setExtras(snap);
      }
    } catch (error) {
      console.error("Pill extras fetch failed:", error);
    } finally {
      pillInFlightRef.current = false;
      if (pendingFullExtrasRef.current) {
        pendingFullExtrasRef.current = false;
        if (expandedExtrasRef.current && !document.hidden) {
          void refreshExtras();
        }
      }
    }
  }, [refreshExtras]);

  const {
    ports,
    portSearch,
    setPortSearch,
    copiedPort,
    portsError,
    setPortsError,
    stackFeedback,
    httpProbes,
    probingPorts,
    refreshPorts,
    probeHttpPorts,
    copyPortUrl,
    stopProjectStack,
  } = usePortList(refreshExtras);

  const refreshAfterSignal = useCallback(() => {
    void refreshPorts();
    void refreshExtras();
    invoke<HardwareTelemetrySnapshot>("fetch_telemetry")
      .then(recordSnapshot)
      .catch(() => {});
  }, [refreshPorts, refreshExtras, recordSnapshot]);

  const {
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
  } = useProcessList(refreshAfterSignal);

  const networkTools = useNetworkTools();
  const systemTools = useSystemTools(setExtras, refreshExtras, extras);
  const agentTools = useAgentTools(setExtras, setActionFeedback);

  const awakeInFlightRef = useRef(false);

  const refreshAwake = useCallback(async () => {
    if (awakeInFlightRef.current) return;
    awakeInFlightRef.current = true;
    try {
      const status = await invoke<AwakeStatus>("fetch_awake_status");
      setAwakeStatus(status);
      setAwakeError(null);
    } catch (error) {
      setAwakeError(String(error));
    } finally {
      awakeInFlightRef.current = false;
    }
  }, []);

  const devicesInFlightRef = useRef(false);

  const refreshDevices = useCallback(async () => {
    if (devicesInFlightRef.current) return;
    devicesInFlightRef.current = true;
    setLoadingDevices(true);
    try {
      const snap = await invoke<DevicesSnapshot>("fetch_devices_snapshot");
      setDevices(snap);
    } catch (error) {
      console.error("Devices fetch failed:", error);
    } finally {
      devicesInFlightRef.current = false;
      setLoadingDevices(false);
    }
  }, []);

  const updateIslandPinned = useCallback((next: boolean) => {
    setIslandPinned(next);
    void invoke("set_island_pinned", { pinned: next }).catch(() => {});
  }, []);

  const finishCollapse = useCallback(
    (nativeOp: () => void) => {
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
        nativeOp();
      }, 110);
    },
    [updateIslandPinned],
  );

  const handleDismissIslandIntoNotch = useCallback(() => {
    finishCollapse(() => void invoke("dismiss_dynamic_island").catch(() => {}));
  }, [finishCollapse]);

  useEffect(() => {
    return () => {
      if (hoverTimeoutRef.current !== null) window.clearTimeout(hoverTimeoutRef.current);
      if (collapseAnimRef.current !== null) window.clearTimeout(collapseAnimRef.current);
    };
  }, []);

  useEffect(() => {
    dismissIslandRef.current = handleDismissIslandIntoNotch;
  }, [handleDismissIslandIntoNotch]);

  useEffect(() => {
    let cancelled = false;
    // Any preferences-updated event that lands before the bootstrap reply is
    // newer than that reply; the fetch result must not roll it back.
    let preferencesEventSeen = false;
    const unlistenPrefs = listen<ApplicationPreferences>("preferences-updated", (event) => {
      preferencesEventSeen = true;
      prefsGenerationRef.current += 1;
      if (!cancelled) setPreferences(event.payload);
    });
    invoke<ApplicationPreferences>("application_preferences")
      .then((prefs) => {
        if (cancelled || preferencesEventSeen) return;
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
      .then((settings) => {
        if (!cancelled) setThermalPolicy(settings);
      })
      .catch((error) => console.error("Thermal policy settings fetch failed:", error));
    const unlistenPolicy = listen<ThermalPolicySettings>("thermal-policy-updated", (event) => {
      if (!cancelled) setThermalPolicy(event.payload);
    });
    const unlistenDustBlast = listen<number>("dust-blast-tick", (event) => {
      if (!cancelled) setDustBlastSecs(event.payload);
    });
    const unlistenIslandSync = listen<boolean>("island-collapsed-sync", (event) => {
      if (cancelled) return;
      setIslandCollapsing(false);
      setIslandCollapsed(event.payload);
    });
    const unlistenIslandHideReq = listen<boolean>("island-request-hide", () => {
      if (!cancelled && !cmdOpenRef.current) {
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
      cancelled = true;
      window.removeEventListener("focus", onFocusOrVisible);
      document.removeEventListener("visibilitychange", onFocusOrVisible);
      unlistenPrefs.then((unlisten) => unlisten()).catch(() => {});
      unlistenPolicy.then((unlisten) => unlisten()).catch(() => {});
      unlistenDustBlast.then((unlisten) => unlisten()).catch(() => {});
      unlistenIslandSync.then((unlisten) => unlisten()).catch(() => {});
      unlistenIslandHideReq.then((unlisten) => unlisten()).catch(() => {});
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
        if (idx >= 0 && idx < 9 && idx < TABS.length) {
          e.preventDefault();
          setActiveTab(TABS[idx].id);
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
      setPolicyError(String(err));
    }
  };

  const handleSetFanMode = async (fanId: number, mode: "auto" | "manual", rpm?: number) => {
    try {
      await invoke("set_fan_mode", { fanId, mode, rpm });
    } catch (err) {
      setPolicyError(String(err));
    }
  };

  const handleHideWindow = () => {
    invoke("toggle_popover").catch(() => {});
  };

  const handleUpdatePreferences = async (change: ApplicationPreferenceChange) => {
    try {
      await invoke<ApplicationPreferences>("update_application_preferences", {
        change,
      });
      setPreferencesError(null);
    } catch (error) {
      setPreferencesError(String(error));
      const generation = prefsGenerationRef.current;
      invoke<ApplicationPreferences>("application_preferences")
        .then((prefs) => {
          if (prefsGenerationRef.current === generation) setPreferences(prefs);
        })
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
    const idx = POLICY_MODES.findIndex((mode) => mode.id === thermalPolicy.mode);
    const next = POLICY_MODES[(idx + 1) % POLICY_MODES.length].id;
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

  const handleBundleImported = () => {
    setPreferencesError(null);
    setPolicyError(null);
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

  const handleRevealInFinder = async (path: string) => {
    try {
      await invoke("reveal_in_finder", { path });
    } catch (err) {
      setPortsError(String(err));
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
    try {
      await invoke("cancel_fan_dust_blast");
      setDustBlastSecs(0);
      setPolicyError(null);
    } catch (err) {
      setPolicyError(String(err));
    }
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
      finishCollapse(() =>
        void invoke("set_island_collapsed", { collapsed: true }).catch(() => {}),
      );
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

  const packageWatts = extras?.silicon_power.package_watts || battery?.power_watts || 0;
  const agents = extras?.agents ?? [];
  const cameraActive = Boolean(extras?.audio.camera_in_use);
  const highThermalWarning =
    cpuCelsius !== null && cpuCelsius >= 80 && thermalPolicy.mode !== "performance";

  const commandItems = useMemo(() => {
    if (!cmdOpen) return [];
    const themeItems = PALETTE_LOOK_IDS.map((id) => ({
      id: `look-${id}`,
      label: `Theme: ${lookName(id)}`,
      category: "Theme",
      run: () => void handleUpdatePreferences({ type: "set_popover_look", value: id }),
    }));

    const policyItems = POLICY_MODES.map((mode) => ({
      id: `pol-${mode.id}`,
      label: `Thermal Policy: ${"commandLabel" in mode ? mode.commandLabel : mode.label}`,
      category: "Fans",
      run: () => void handleSelectPolicyMode(mode.id),
    }));

    const tabItems = TABS.map((tab) => ({
      id: `tab-${tab.id}`,
      label: `Open Tab: ${tab.label}`,
      category: "Navigate",
      run: () => setActiveTab(tab.id),
    }));

    const portKillItems = ports.slice(0, 8).map((p) => ({
      id: `kill-${p.port}-${p.pid}`,
      label: `Stop Port :${p.port} (${p.process_name})`,
      category: "Port",
      run: () => void terminateProcess(p.pid, `${p.process_name} :${p.port}`),
    }));

    const quickOps = [
      {
        id: "op-rec",
        label: recordedCount !== null ? "Stop & Copy Thermal Session" : "Start Thermal Session Recording",
        category: "Action",
        run: () => void toggleRecording(),
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
        run: () => void systemTools.runMaintenance("flush_dns"),
      },
      {
        id: "op-orphans",
        label: "Clean Orphaned Dev Servers",
        category: "Action",
        run: () => void cleanupOrphans(),
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
        run: () => void systemTools.runQuickSwitch("toggle_dark_mode"),
      },
      {
        id: "op-hidden-files",
        label: "Switch: Toggle Hidden Finder Files",
        category: "Switch",
        run: () => void systemTools.runQuickSwitch("toggle_hidden_files"),
      },
      {
        id: "op-desktop-icons",
        label: "Switch: Toggle Desktop Icons",
        category: "Switch",
        run: () => void systemTools.runQuickSwitch("toggle_desktop_icons"),
      },
      {
        id: "op-clear-clip",
        label: "Switch: Clear Clipboard",
        category: "Switch",
        run: () => void systemTools.runQuickSwitch("clear_clipboard"),
      },
      {
        id: "op-empty-trash",
        label: "Switch: Empty macOS Trash",
        category: "Switch",
        run: () => void systemTools.runQuickSwitch("empty_trash"),
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
          void systemTools.runDiskBenchmark();
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
    recordedCount,
    awakeStatus.keep_awake_active,
    dustBlastSecs,
    preferences.layout_density,
    islandCollapsed,
    cmdQuery,
  ]);

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
    expandedExtrasRef.current = !(isDynamicIsland && islandCollapsed);
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
                    {cpuCelsius !== null ? `${Math.round(displayTemperature(cpuCelsius, unit))}` : "--"}/
                    {gpuCelsius !== null ? `${Math.round(displayTemperature(gpuCelsius, unit))}°` : "--°"}{" "}
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
                        {POLICY_MODES.find((m) => m.id === thermalPolicy.mode)?.short}
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
                    {TABS.map(({ id: tab }) => {
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
            <HomeView
              telemetry={telemetry}
              extras={extras}
              ports={ports}
              cpuCelsius={cpuCelsius}
              gpuCelsius={gpuCelsius}
              sensors={sensors}
              fans={fans}
              battery={battery}
              processes={processes}
              unit={unit}
              thermalPolicy={thermalPolicy}
              fanActuationStatus={fanActuationStatus}
              historyWindow={historyWindow}
              windowedHistory={windowedHistory}
              netHistory={netHistory}
              memHistory={memHistory}
              recordedCount={recordedCount}
              sessionExportMsg={sessionExportMsg}
              showHealthReport={showHealthReport}
              setShowHealthReport={setShowHealthReport}
              setHistoryWindow={setHistoryWindow}
              handleToggleRecording={toggleRecording}
              setActiveTab={setActiveTab}
              handleSelectPolicyMode={handleSelectPolicyMode}
              handleTerminateProcess={terminateProcess}
              handleCleanupOrphans={cleanupOrphans}
            />
          )}

          {activeTab === "fans" && (
            <FansView
              telemetry={telemetry}
              policyError={policyError}
              thermalPolicy={thermalPolicy}
              sensors={sensors}
              fans={fans}
              cpuCelsius={cpuCelsius}
              gpuCelsius={gpuCelsius}
              unit={unit}
              fanActuationStatus={fanActuationStatus}
              dustBlastSecs={dustBlastSecs}
              handleSelectPolicyMode={handleSelectPolicyMode}
              handleSaveRule={handleSaveRule}
              handleDeleteRule={handleDeleteRule}
              handleSetFanSpeed={handleSetFanSpeed}
              handleSetFanMode={handleSetFanMode}
              handleEnableHelper={handleEnableHelper}
              handleStartDustBlast={handleStartDustBlast}
              handleCancelDustBlast={handleCancelDustBlast}
            />
          )}

          {activeTab === "system" && (
            <SystemView
              telemetry={telemetry}
              extras={extras}
              sensors={sensors}
              cpuCelsius={cpuCelsius}
              gpuCelsius={gpuCelsius}
              unit={unit}
              battery={battery}
              hasSmcAccess={hasSmcAccess}
              historyWindow={historyWindow}
              windowedHistory={windowedHistory}
              setHistoryWindow={setHistoryWindow}
              maintenanceMsg={systemTools.maintenanceMsg}
              handleRunMaintenance={systemTools.runMaintenance}
              cacheMsg={systemTools.cacheMsg}
              confirmingCacheId={systemTools.confirmingCacheId}
              cleaningCacheId={systemTools.cleaningCacheId}
              setConfirmingCacheId={systemTools.setConfirmingCacheId}
              handleCleanCache={systemTools.cleanCache}
              quickSwitchMsg={systemTools.quickSwitchMsg}
              handleRunQuickSwitch={systemTools.runQuickSwitch}
              diskBench={systemTools.diskBench}
              runningDiskBench={systemTools.runningDiskBench}
              diskBenchErr={systemTools.diskBenchErr}
              handleRunDiskBenchmark={systemTools.runDiskBenchmark}
              showLaunchAgents={systemTools.showLaunchAgents}
              setShowLaunchAgents={systemTools.setShowLaunchAgents}
              handleRevealInFinder={handleRevealInFinder}
            />
          )}

          {activeTab === "apps" && (
            <AppsView
              telemetry={telemetry}
              processes={processes}
              extras={extras}
              packageWatts={packageWatts}
              appSearch={appSearch}
              setAppSearch={setAppSearch}
              appSort={appSort}
              setAppSort={setAppSort}
              expandedAppName={expandedAppName}
              setExpandedAppName={setExpandedAppName}
              processInspections={processInspections}
              inspectingPid={inspectingPid}
              actionFeedback={actionFeedback}
              setActionFeedback={setActionFeedback}
              handleTerminateProcess={terminateProcess}
              handleInspectProcess={inspectProcess}
              handleCleanupOrphans={cleanupOrphans}
              handleRevealInFinder={handleRevealInFinder}
            />
          )}

          {activeTab === "network" && (
            <NetworkView
              extras={extras}
              netHistory={netHistory}
              expandedNetApp={networkTools.expandedNetApp}
              appConnections={networkTools.appConnections}
              appConnectionErrors={networkTools.appConnectionErrors}
              loadingConnApp={networkTools.loadingConnApp}
              handleInspectNetApp={networkTools.inspectNetApp}
              speedTest={networkTools.speedTest}
              runningSpeedTest={networkTools.runningSpeedTest}
              speedTestError={networkTools.speedTestError}
              handleRunSpeedTest={networkTools.runSpeedTest}
              handleTerminateProcess={terminateProcess}
            />
          )}

          {activeTab === "devices" && (
            <DevicesView
              devices={devices}
              battery={battery}
              loadingDevices={loadingDevices}
              refreshDevices={refreshDevices}
            />
          )}

          {activeTab === "volume" && (
            <VolumeView extras={extras} handleUpdateAudio={handleUpdateAudio} />
          )}

          {activeTab === "ports" && (
            <PortsView
              ports={ports}
              portSearch={portSearch}
              setPortSearch={setPortSearch}
              httpProbes={httpProbes}
              probingPorts={probingPorts}
              handleProbeHttpPorts={probeHttpPorts}
              copiedPort={copiedPort}
              handleCopyPortUrl={copyPortUrl}
              portsError={portsError}
              refreshPorts={refreshPorts}
              handleStopProjectStack={stopProjectStack}
              stackFeedback={stackFeedback}
              handleRevealInFinder={handleRevealInFinder}
              handleTerminateProcess={terminateProcess}
            />
          )}

          {activeTab === "agents" && (
            <AgentsView
              extras={extras}
              unloadingModel={agentTools.unloadingModel}
              controllingCtr={agentTools.controllingCtr}
              copiedMcpConfig={agentTools.copiedMcpConfig}
              handleUnloadOllama={agentTools.unloadOllama}
              handleControlContainer={agentTools.controlContainer}
              handleCopyMcpConfig={agentTools.copyMcpConfig}
              setActiveTab={setActiveTab}
              handleTerminateProcess={terminateProcess}
            />
          )}

          {activeTab === "awake" && (
            <AwakeView
              awakeStatus={awakeStatus}
              awakeError={awakeError}
              handleSetKeepAwake={handleSetKeepAwake}
              handleToggleKeepAwakeRule={handleToggleKeepAwakeRule}
              refreshAwake={refreshAwake}
            />
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
                history={windowedHistory}
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
                onShowUpdateModal={update.open}
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

              {update.release ? (
                <button
                  type="button"
                  onClick={() => update.open(update.release!)}
                  className="flex items-center gap-1 font-mono text-[var(--warn)] font-bold hover:underline shrink-0"
                >
                  <Sparkles className="w-3 h-3 animate-pulse" />
                  v{update.release.version}
                </button>
              ) : (
                <span className="font-mono shrink-0">v{CURRENT_VERSION}</span>
              )}
            </div>
          </>
        )}
        </div>
      </div>


      {update.visible && update.release && (
        <UpdateModal
          currentVersion={CURRENT_VERSION}
          release={update.release}
          updateReady={update.ready}
          preparationError={update.preparationError}
          onClose={update.dismiss}
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
