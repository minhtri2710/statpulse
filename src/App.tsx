import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Header, NavigationRail } from "./features/shell/Header";
import { SettingsModal } from "./features/preferences/SettingsModal";
import { DesignStudioModal } from "./features/preferences/DesignStudioModal";
import { DEFAULT_PREFERENCES } from "./features/preferences/defaults";
import { UpdateModal } from "./features/updates/UpdateModal";
import { CURRENT_VERSION } from "./features/updates/updater";
import { AppTab, TABS } from "./navigation";
import { POLICY_MODES } from "./features/thermals/policyModes";
import { formatDuration } from "./services/format";
import { revealInFinder } from "./services/finder";
import { computeIslandDotTone, unwrapTelemetry } from "./features/thermals/display";
import { useTelemetryHistory } from "./features/thermals/useTelemetryHistory";
import { useAppUpdate } from "./features/updates/useAppUpdate";
import { useProcessList } from "./features/processes/useProcessList";
import { usePortList } from "./features/ports/usePortList";
import { useSystemTools } from "./features/system/useSystemTools";
import { CommandPalette } from "./features/command-palette/CommandPalette";
import { IslandAlertStrip } from "./features/island/IslandAlertStrip";
import { IslandNotchRow } from "./features/island/IslandNotchRow";
import { useIslandAlert } from "./features/island/useIslandAlert";
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
  AlertPreferences,
  ApplicationPreferenceChange,
  ApplicationPreferences,
  AutoThermalTriggers,
  AwakeStatus,
  DevicesSnapshot,
  HardwareTelemetrySnapshot,
  KeepAwakeRules,
  NowPlayingItem,
  SystemExtrasSnapshot,
  ThermalPolicyMode,
  ThermalPolicySettings,
  ThermalRule,
} from "./types";
import {
  ChevronUp,
  Coffee,
  Command,
  Music,
  ShieldCheck,
  Sparkles,
  Timer,
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

  const [thermalPolicy, setThermalPolicy] = useState<ThermalPolicySettings>({
    mode: "system_auto",
    rules: [],
  });
  const [activeTab, setActiveTab] = useState<AppTab>("home");

  const [islandCollapsed, setIslandCollapsed] = useState(false);
  const [islandCollapsing, setIslandCollapsing] = useState(false);
  const [islandHasNotch, setIslandHasNotch] = useState(true);
  const hoverTimeoutRef = useRef<number | null>(null);
  const collapseAnimRef = useRef<number | null>(null);
  const justCollapsedAtRef = useRef<number>(0);
  const cmdOpenRef = useRef(false);
  const dismissIslandRef = useRef<() => void>(() => {});

  const [awakeStatus, setAwakeStatus] = useState<AwakeStatus>({
    keep_awake_active: false,
    duration_seconds: null,
    remaining_seconds: null,
    assertions: [],
    rules: { while_on_ac: false, while_ports_active: false, while_ai_agents_active: false },
    auto_rule_reason: null,
  });
  const [awakeError, setAwakeError] = useState<string | null>(null);

  const [policyError, setPolicyError] = useState<string | null>(null);
  const [dustBlastSecs, setDustBlastSecs] = useState<number>(0);
  const [preferences, setPreferences] = useState<ApplicationPreferences>(DEFAULT_PREFERENCES);
  const [focusRemainingSecs, setFocusRemainingSecs] = useState<number | null>(null);
  const [focusTargetSecs, setFocusTargetSecs] = useState<number | null>(null);
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
  const activeTabRef = useRef<AppTab>(activeTab);
  const keepAwakeActiveRef = useRef(awakeStatus.keep_awake_active);

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

  const systemTools = useSystemTools(setExtras, refreshExtras, extras);
  const { islandAlert, clearAlert: clearIslandAlert } = useIslandAlert(islandCollapsedRef);

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
      clearIslandAlert();
      justCollapsedAtRef.current = Date.now();
      setIslandCollapsing(true);
      collapseAnimRef.current = window.setTimeout(() => {
        setIslandCollapsing(false);
        setIslandCollapsed(true);
        nativeOp();
      }, 110);
    },
    [clearIslandAlert],
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
          void invoke("set_island_collapsed", { collapsed: true }).catch(() => {});
          void refreshPillExtras();
          void refreshAwake();
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
    const unlistenAwake = listen<AwakeStatus>("awake-status-updated", (event) => {
      if (!cancelled) setAwakeStatus(event.payload);
    });
    const unlistenDustBlast = listen<number>("dust-blast-tick", (event) => {
      if (!cancelled) setDustBlastSecs(event.payload);
    });
    const unlistenIslandSync = listen<boolean>("island-collapsed-sync", (event) => {
      if (cancelled) return;
      setIslandCollapsing(false);
      setIslandCollapsed(event.payload);
      if (!event.payload) {
        clearIslandAlert();
      }
    });
    const unlistenDisplayCtx = listen<{ has_notch: boolean; display_index: number }>(
      "island-display-context",
      (event) => {
        if (!cancelled) setIslandHasNotch(event.payload.has_notch);
      },
    );
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

    let slowTick = 0;
    const extrasInterval = window.setInterval(() => {
      if (document.hidden) return;
      if (isDynamicIslandRef.current && islandCollapsedRef.current) {
        void refreshPillExtras();
        return;
      }
      slowTick = (slowTick + 1) % 4;
      void refreshExtras();
      if (activeTabRef.current === "home" || activeTabRef.current === "ports" || slowTick === 0) {
        void refreshPorts();
      }
      if (
        activeTabRef.current === "awake" ||
        keepAwakeActiveRef.current ||
        slowTick === 0
      ) {
        void refreshAwake();
      }
    }, 3500);

    const devicesInterval = window.setInterval(() => {
      if (document.hidden) return;
      if (isDynamicIslandRef.current && islandCollapsedRef.current) return;
      if (activeTabRef.current === "devices") {
        void refreshDevices();
      }
    }, 18000);

    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocusOrVisible);
      document.removeEventListener("visibilitychange", onFocusOrVisible);
      unlistenPrefs.then((unlisten) => unlisten()).catch(() => {});
      unlistenPolicy.then((unlisten) => unlisten()).catch(() => {});
      unlistenAwake.then((unlisten) => unlisten()).catch(() => {});
      unlistenDustBlast.then((unlisten) => unlisten()).catch(() => {});
      unlistenIslandSync.then((unlisten) => unlisten()).catch(() => {});
      unlistenDisplayCtx.then((unlisten) => unlisten()).catch(() => {});
      unlistenIslandHideReq.then((unlisten) => unlisten()).catch(() => {});
      window.clearInterval(extrasInterval);
      window.clearInterval(devicesInterval);
    };
  }, [refreshPorts, refreshAwake, refreshExtras, refreshPillExtras, refreshDevices, clearIslandAlert]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey && !e.shiftKey && !e.altKey && (e.key.toLowerCase() === "k" || e.key.toLowerCase() === "p")) {
        e.preventDefault();
        setCmdOpen((v) => !v);
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
      await revealInFinder(path);
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

  const handleStartFocusTimer = useCallback((seconds: number | null) => {
    setFocusTargetSecs(seconds);
    setFocusRemainingSecs(seconds);
  }, []);

  useEffect(() => {
    if (focusRemainingSecs === null) return;
    if (focusRemainingSecs <= 0) {
      setFocusRemainingSecs(null);
      setFocusTargetSecs(null);
      void invoke("trigger_custom_alert", {
        title: "Focus Timer Complete",
        detail: "Session finished",
        tone: "good",
      }).catch(() => {});
      return;
    }
    const id = window.setTimeout(() => {
      setFocusRemainingSecs((prev) => (prev !== null ? prev - 1 : null));
    }, 1000);
    return () => window.clearTimeout(id);
  }, [focusRemainingSecs]);

  const handleMediaControl = useCallback(
    async (action: "play_pause" | "next" | "previous") => {
      try {
        const nextNowPlaying = await invoke<NowPlayingItem | null>("control_media_playback", {
          action,
        });
        setExtras((prev) =>
          prev
            ? {
                ...prev,
                audio: {
                  ...prev.audio,
                  now_playing: nextNowPlaying,
                },
              }
            : prev,
        );
      } catch (err) {
        console.error("Media control failed:", err);
      }
    },
    [],
  );

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
      clearIslandAlert();
      setIslandCollapsing(false);
      setIslandCollapsed(false);
      await invoke("set_island_collapsed", { collapsed: false }).catch(() => {});
    } else {
      finishCollapse(() =>
        void invoke("set_island_collapsed", { collapsed: true }).catch(() => {}),
      );
    }
  };

  const temperatures = unwrapTelemetry(telemetry?.temperatures);
  const sensors = temperatures?.sensors ?? [];
  const fans = unwrapTelemetry(telemetry?.fans) ?? [];
  const battery = unwrapTelemetry(telemetry?.battery);
  const processes = unwrapTelemetry(telemetry?.top_processes) ?? [];
  const fanActuationStatus = telemetry?.fan_actuation_status ?? "not_registered";
  const hasSmcAccess = temperatures !== null;
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

  const islandDotTone = computeIslandDotTone(
    dustBlastSecs,
    cameraActive,
    cpuCelsius,
    agents.length,
    highThermalWarning,
  );

  useEffect(() => {
    cmdOpenRef.current = cmdOpen;
  }, [cmdOpen]);

  useEffect(() => {
    activeTabRef.current = activeTab;
    if (activeTab === "ports") void refreshPorts();
    else if (activeTab === "awake") void refreshAwake();
    else if (activeTab === "devices") void refreshDevices();
  }, [activeTab, refreshPorts, refreshAwake, refreshDevices]);

  useEffect(() => {
    keepAwakeActiveRef.current = awakeStatus.keep_awake_active;
  }, [awakeStatus.keep_awake_active]);

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
      setIslandCollapsing(false);
      clearIslandAlert();
    }
  }, [preferences.layout_density, clearIslandAlert]);

  const handleIslandMouseEnter = () => {
    if (!isDynamicIsland) return;
    if (hoverTimeoutRef.current !== null) {
      window.clearTimeout(hoverTimeoutRef.current);
      hoverTimeoutRef.current = null;
    }
    if (islandCollapsed && !islandCollapsing && Date.now() - justCollapsedAtRef.current > 400) {
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
                : awakeStatus.keep_awake_active
                  ? "awake"
                  : "system";
        setActiveTab(targetTab);
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
      ) : focusRemainingSecs !== null ? (
        <>
          <Timer className="w-3 h-3 text-cyan-400" />
          <span>Focus · {formatDuration(focusRemainingSecs)}</span>
        </>
      ) : extras?.audio.now_playing?.playing ? (
        <>
          <Music className="w-3 h-3 text-[var(--acc)]" />
          <span className="truncate max-w-[110px]">{extras.audio.now_playing.title}</span>
        </>
      ) : awakeStatus.keep_awake_active ? (
        <>
          <Coffee className="w-3 h-3 text-amber-400" />
          <span>
            Awake ·{" "}
            {awakeStatus.remaining_seconds !== null
              ? formatDuration(awakeStatus.remaining_seconds)
              : "Always"}
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
        data-has-notch={islandHasNotch ? "true" : "false"}
        className={
          isDynamicIsland
            ? `di-frame ${
                islandCollapsing
                  ? "di-frame--collapsing"
                  : islandCollapsed
                    ? `di-frame--pill${islandAlert ? " di-frame--alert" : ""}`
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
              <IslandNotchRow
                islandDotTone={islandDotTone}
                cpuCelsius={cpuCelsius}
                gpuCelsius={gpuCelsius}
                unit={unit}
                dustBlastSecs={dustBlastSecs}
                fans={fans}
                cameraActive={cameraActive}
                micInUseBy={extras?.audio.mic_in_use_by ?? null}
                watts={battery?.power_watts ?? extras?.silicon_power.package_watts ?? 0}
                usedGb={extras?.memory.used_gb ?? null}
                rxBytesPerSec={extras?.network.rx_bytes_per_sec ?? null}
                agentCount={agents.length}
                awakeStatus={awakeStatus}
                focusRemainingSecs={focusRemainingSecs}
                onClick={() => {
                  if (islandCollapsed) {
                    void handleToggleIslandCollapsed(false);
                  } else {
                    handleDismissIslandIntoNotch();
                  }
                }}
                title={islandCollapsed ? "Click to open detail panel" : "Click to collapse"}
                rightContent={
                  islandCollapsed ? undefined : (
                    <div className="flex items-center justify-end gap-1 shrink-0">
                      <button
                        type="button"
                        onClick={() => setCmdOpen((v) => !v)}
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
                  )
                }
              />

              {islandCollapsed && !islandCollapsing && islandAlert && (
                <IslandAlertStrip
                  alert={islandAlert}
                  onClick={() => {
                    const targetTab = islandAlert.tab;
                    clearIslandAlert();
                    if (targetTab) setActiveTab(targetTab);
                    void handleToggleIslandCollapsed(false);
                  }}
                />
              )}

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
                          onClick={() => setActiveTab(tab)}
                          className="di-tab"
                        >
                          <span className="capitalize">{tab}</span>
                          {tab === "awake" && awakeStatus.keep_awake_active && (
                            <span
                              className="w-1.5 h-1.5 rounded-full bg-amber-400"
                              title="Keep Awake Active"
                            />
                          )}
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
            keepAwakeActive={awakeStatus.keep_awake_active}
            keepAwakeRemainingSeconds={awakeStatus.remaining_seconds}
            focusRemainingSecs={focusRemainingSecs}
            onOpenAwake={() => setActiveTab("awake")}
            onCyclePolicy={handleCyclePolicy}
            onOpenCommandPalette={() => setCmdOpen((v) => !v)}
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
              alerts={preferences.alerts}
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
              autoThermal={preferences.auto_thermal}
              onUpdateAutoThermal={(next: AutoThermalTriggers) =>
                handleUpdatePreferences({ type: "set_auto_thermal", value: next })
              }
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
              batteryHealthPercent={devices?.battery_health.health_percent ?? null}
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
              handleTerminateProcess={terminateProcess}
            />
          )}

          {activeTab === "devices" && (
            <DevicesView
              devices={devices}
              battery={battery}
              alerts={preferences.alerts}
              onUpdateAlerts={(next: AlertPreferences) =>
                handleUpdatePreferences({ type: "set_alerts", value: next })
              }
              loadingDevices={loadingDevices}
              refreshDevices={refreshDevices}
            />
          )}

          {activeTab === "volume" && (
            <VolumeView
              extras={extras}
              handleUpdateAudio={handleUpdateAudio}
              handleMediaControl={handleMediaControl}
            />
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
              setExtras={setExtras}
              setActiveTab={setActiveTab}
              handleTerminateProcess={terminateProcess}
            />
          )}

          {activeTab === "awake" && (
            <AwakeView
              awakeStatus={awakeStatus}
              awakeError={awakeError}
              focusRemainingSecs={focusRemainingSecs}
              focusTargetSecs={focusTargetSecs}
              handleStartFocusTimer={handleStartFocusTimer}
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

      <CommandPalette
        open={cmdOpen}
        onClose={() => setCmdOpen(false)}
        ports={ports}
        recordedCount={recordedCount}
        keepAwakeActive={awakeStatus.keep_awake_active}
        focusRemainingSecs={focusRemainingSecs}
        dustBlastSecs={dustBlastSecs}
        layoutDensity={preferences.layout_density}
        islandCollapsed={islandCollapsed}
        onUpdatePreferences={handleUpdatePreferences}
        onSelectPolicyMode={handleSelectPolicyMode}
        onSelectTab={setActiveTab}
        onOpenHealthReport={() => setShowHealthReport(true)}
        onTerminateProcess={terminateProcess}
        onToggleRecording={toggleRecording}
        onRunMaintenance={systemTools.runMaintenance}
        onCleanupOrphans={cleanupOrphans}
        onSetKeepAwake={handleSetKeepAwake}
        onStartFocusTimer={handleStartFocusTimer}
        onMediaControl={handleMediaControl}
        onRunQuickSwitch={systemTools.runQuickSwitch}
        onStartDustBlast={handleStartDustBlast}
        onCancelDustBlast={handleCancelDustBlast}
        onRunDiskBenchmark={systemTools.runDiskBenchmark}
        onToggleIslandCollapsed={handleToggleIslandCollapsed}
      />
    </div>
  );
}

export default App;
