import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { displayTemperature } from "../features/thermals/display";
import { formatRate } from "../services/format";
import {
  ApplicationPreferences,
  BatteryReading,
  FanReading,
  HardwareTelemetrySnapshot,
  SystemExtrasSnapshot,
} from "../types";

type IslandAlertPayload = {
  id: number;
  title: string;
  detail?: string | null;
  tone: "info" | "warn" | "danger" | "good";
  duration_ms: number;
};

type IslandDisplayContext = {
  has_notch: boolean;
  display_index: number;
};

export function IslandMirrorPill() {
  const params = new URLSearchParams(window.location.search);
  const displayIndex = Number(params.get("display") ?? "0") || 0;
  const initialHasNotch = params.get("notch") === "1";

  const [hasNotch, setHasNotch] = useState(initialHasNotch);
  const [telemetry, setTelemetry] = useState<HardwareTelemetrySnapshot | null>(null);
  const [extras, setExtras] = useState<SystemExtrasSnapshot | null>(null);
  const [dustBlastSecs, setDustBlastSecs] = useState(0);
  const [islandAlert, setIslandAlert] = useState<IslandAlertPayload | null>(null);
  const [preferences, setPreferences] = useState<ApplicationPreferences>({
    temperature_unit: "celsius",
    telemetry_interval_ms: 1500,
    popover_look: "liquid_glass",
    layout_density: "dynamic_island",
    dynamic_island_display: "all_displays",
    menu_bar_style: "compact_text",
    chart_palette: "default",
    traffic_light_menu_bar: false,
    launch_at_login: false,
  });

  const alertTimerRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<HardwareTelemetrySnapshot>("fetch_telemetry")
      .then((snap) => {
        if (!cancelled) setTelemetry(snap);
      })
      .catch(() => {});
    invoke<SystemExtrasSnapshot>("fetch_pill_extras")
      .then((snap) => {
        if (!cancelled) setExtras(snap);
      })
      .catch(() => {});
    invoke<ApplicationPreferences>("application_preferences")
      .then((prefs) => {
        if (!cancelled) setPreferences(prefs);
      })
      .catch(() => {});

    const unlistenTel = listen<HardwareTelemetrySnapshot>("telemetry-update", (e) => {
      if (!cancelled) setTelemetry(e.payload);
    });
    const unlistenExtras = listen<SystemExtrasSnapshot>("pill-extras-sync", (e) => {
      if (!cancelled) setExtras(e.payload);
    });
    const unlistenPrefs = listen<ApplicationPreferences>("preferences-updated", (e) => {
      if (!cancelled) setPreferences(e.payload);
    });
    const unlistenBlast = listen<number>("dust-blast-tick", (e) => {
      if (!cancelled) setDustBlastSecs(e.payload);
    });
    const unlistenCtx = listen<IslandDisplayContext>("island-display-context", (e) => {
      if (cancelled || e.payload.display_index !== displayIndex) return;
      setHasNotch(e.payload.has_notch);
    });
    const unlistenAlert = listen<IslandAlertPayload>("island-alert", (e) => {
      if (cancelled) return;
      if (alertTimerRef.current !== null) {
        window.clearTimeout(alertTimerRef.current);
        alertTimerRef.current = null;
      }
      setIslandAlert(e.payload);
      alertTimerRef.current = window.setTimeout(() => {
        alertTimerRef.current = null;
        setIslandAlert((curr) => (curr?.id === e.payload.id ? null : curr));
      }, e.payload.duration_ms + 150);
    });
    const unlistenAlertClear = listen<number>("island-alert-clear", (e) => {
      if (cancelled) return;
      if (e.payload === 0) {
        if (alertTimerRef.current !== null) {
          window.clearTimeout(alertTimerRef.current);
          alertTimerRef.current = null;
        }
        setIslandAlert(null);
        return;
      }
      setIslandAlert((curr) => {
        if (curr?.id === e.payload) {
          if (alertTimerRef.current !== null) {
            window.clearTimeout(alertTimerRef.current);
            alertTimerRef.current = null;
          }
          return null;
        }
        return curr;
      });
    });

    return () => {
      cancelled = true;
      if (alertTimerRef.current !== null) window.clearTimeout(alertTimerRef.current);
      unlistenTel.then((u) => u()).catch(() => {});
      unlistenExtras.then((u) => u()).catch(() => {});
      unlistenPrefs.then((u) => u()).catch(() => {});
      unlistenBlast.then((u) => u()).catch(() => {});
      unlistenCtx.then((u) => u()).catch(() => {});
      unlistenAlert.then((u) => u()).catch(() => {});
      unlistenAlertClear.then((u) => u()).catch(() => {});
    };
  }, []);

  const temperatures =
    telemetry?.temperatures.status === "available" ? telemetry.temperatures.value : null;
  const fans: FanReading[] = telemetry?.fans.status === "available" ? telemetry.fans.value : [];
  const battery: BatteryReading | null =
    telemetry?.battery.status === "available" ? telemetry.battery.value : null;
  const unit = preferences.temperature_unit === "celsius" ? "C" : "F";
  const cpuCelsius = temperatures?.cpu_celsius ?? null;
  const gpuCelsius = temperatures?.gpu_celsius ?? null;
  const agents = extras?.agents ?? [];
  const cameraActive = Boolean(extras?.audio.camera_in_use);
  const islandDotTone =
    dustBlastSecs > 0 || cameraActive || (cpuCelsius ?? 0) >= 80
      ? "pp-dot--bad"
      : (cpuCelsius ?? 0) >= 65 || agents.length > 0
        ? "pp-dot--warn"
        : "pp-dot--good";

  const handleOpenPanelHere = () => {
    setIslandAlert(null);
    void invoke("expand_island_on_display", { displayIndex }).catch(() => {});
  };

  return (
    <div
      data-look={preferences.popover_look}
      data-density="dynamic_island"
      data-palette={preferences.chart_palette}
      className="w-full h-screen pp-shell"
    >
      <div
        data-has-notch={hasNotch ? "true" : "false"}
        className={`di-frame di-frame--pill${islandAlert ? " di-frame--alert" : ""}`}
      >
        <div className="di-body">
          <div className="di-notch-row" onClick={handleOpenPanelHere} title="Click to open detail panel">
            <div className="flex items-center gap-1 min-w-0 cursor-pointer py-0.5">
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

            <div className="di-notch-bridge cursor-pointer">
              {cameraActive && <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />}
              {!cameraActive && extras?.audio.mic_in_use_by && (
                <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
              )}
              <span className="di-notch-glow" />
            </div>

            <div className="flex items-center justify-end gap-1 min-w-0 cursor-pointer py-0.5">
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
          </div>

          {islandAlert && (
            <button
              type="button"
              data-tone={islandAlert.tone}
              className="di-alert-strip"
              onClick={handleOpenPanelHere}
              title="Click to inspect details"
            >
              <span className="flex items-center gap-1.5 min-w-0">
                <span
                  className={`pp-dot shrink-0 ${
                    islandAlert.tone === "danger"
                      ? "pp-dot--bad"
                      : islandAlert.tone === "warn"
                        ? "pp-dot--warn"
                        : islandAlert.tone === "good"
                          ? "pp-dot--good"
                          : ""
                  }`}
                  style={islandAlert.tone === "info" ? { background: "var(--acc)" } : undefined}
                />
                <span className="font-semibold text-[10px] text-[var(--ink)] truncate">
                  {islandAlert.title}
                </span>
                {islandAlert.detail && (
                  <span className="font-mono text-[9.5px] text-[var(--sec)] truncate">
                    {islandAlert.detail}
                  </span>
                )}
              </span>
              <span className="text-[9.5px] font-medium text-[var(--sec)] shrink-0">Open ›</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
