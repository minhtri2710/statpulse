import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ApplicationPreferences,
  AwakeStatus,
  HardwareTelemetrySnapshot,
  SystemExtrasSnapshot,
} from "../../types";
import { DEFAULT_PREFERENCES } from "../preferences/defaults";
import { computeIslandDotTone, unwrapTelemetry } from "../thermals/display";
import { IslandAlertStrip } from "./IslandAlertStrip";
import { IslandNotchRow } from "./IslandNotchRow";
import type { IslandDisplayContext } from "./types";
import { useIslandAlert } from "./useIslandAlert";

export function IslandMirrorPill() {
  const params = new URLSearchParams(window.location.search);
  const displayIndex = Number(params.get("display") ?? "0") || 0;
  const initialHasNotch = params.get("notch") === "1";

  const [hasNotch, setHasNotch] = useState(initialHasNotch);
  const [telemetry, setTelemetry] = useState<HardwareTelemetrySnapshot | null>(null);
  const [extras, setExtras] = useState<SystemExtrasSnapshot | null>(null);
  const [awakeStatus, setAwakeStatus] = useState<AwakeStatus | null>(null);
  const [dustBlastSecs, setDustBlastSecs] = useState<number>(0);
  const [preferences, setPreferences] = useState<ApplicationPreferences>(DEFAULT_PREFERENCES);
  const { islandAlert, clearAlert } = useIslandAlert();

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
    invoke<AwakeStatus>("fetch_awake_status")
      .then((status) => {
        if (!cancelled) setAwakeStatus(status);
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
    const unlistenAwake = listen<AwakeStatus>("awake-status-updated", (e) => {
      if (!cancelled) setAwakeStatus(e.payload);
    });
    const unlistenBlast = listen<number>("dust-blast-tick", (e) => {
      if (!cancelled) setDustBlastSecs(e.payload);
    });
    const unlistenCtx = listen<IslandDisplayContext>("island-display-context", (e) => {
      if (cancelled || e.payload.display_index !== displayIndex) return;
      setHasNotch(e.payload.has_notch);
    });

    return () => {
      cancelled = true;
      unlistenTel.then((u) => u()).catch(() => {});
      unlistenExtras.then((u) => u()).catch(() => {});
      unlistenPrefs.then((u) => u()).catch(() => {});
      unlistenAwake.then((u) => u()).catch(() => {});
      unlistenBlast.then((u) => u()).catch(() => {});
      unlistenCtx.then((u) => u()).catch(() => {});
    };
  }, [displayIndex]);

  const temperatures = unwrapTelemetry(telemetry?.temperatures);
  const fans = unwrapTelemetry(telemetry?.fans) ?? [];
  const battery = unwrapTelemetry(telemetry?.battery);
  const unit = preferences.temperature_unit === "celsius" ? "C" : "F";
  const cpuCelsius = temperatures?.cpu_celsius ?? null;
  const gpuCelsius = temperatures?.gpu_celsius ?? null;
  const agents = extras?.agents ?? [];
  const cameraActive = Boolean(extras?.audio.camera_in_use);
  const islandDotTone = computeIslandDotTone(
    dustBlastSecs,
    cameraActive,
    cpuCelsius,
    agents.length,
  );

  const handleOpenPanelHere = () => {
    clearAlert();
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
          <IslandNotchRow
            islandDotTone={islandDotTone}
            cpuCelsius={cpuCelsius}
            gpuCelsius={gpuCelsius}
            unit={unit}
            dustBlastSecs={dustBlastSecs}
            fans={fans}
            cameraActive={cameraActive}
            micInUseBy={extras?.audio.mic_in_use_by ?? null}
            micMuted={extras?.audio.input_muted ?? false}
            watts={battery?.power_watts ?? extras?.silicon_power.package_watts ?? 0}
            usedGb={extras?.memory.used_gb ?? null}
            rxBytesPerSec={extras?.network.rx_bytes_per_sec ?? null}
            agentCount={agents.length}
            awakeStatus={awakeStatus}
            onClick={handleOpenPanelHere}
          />

          {islandAlert && <IslandAlertStrip alert={islandAlert} onClick={handleOpenPanelHere} />}
        </div>
      </div>
    </div>
  );
}
