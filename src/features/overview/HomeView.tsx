import { useMemo } from "react";
import { Camera, Check, Copy, Stethoscope } from "lucide-react";
import { HISTORY_WINDOWS, type HistorySample, type HistoryWindow } from "../../services/thermalHistory";
import { formatMemory, formatRate, sparkPoints } from "../../services/format";
import { fanSpeedPercent, formatTempShort } from "../thermals/display";
import { POLICY_MODES } from "../thermals/policyModes";
import { useTransientValue } from "../useTransient";
import type { AppTab } from "../../navigation";
import type {
  BatteryReading,
  FanActuationStatus,
  FanReading,
  HardwareTelemetrySnapshot,
  ProcessReading,
  SystemExtrasSnapshot,
  TemperatureReading,
  ThermalPolicyMode,
  ThermalPolicySettings,
} from "../../types";

interface HomeViewProps {
  telemetry: HardwareTelemetrySnapshot | null;
  extras: SystemExtrasSnapshot | null;
  ports: { is_exposed: boolean }[];
  cpuCelsius: number | null;
  gpuCelsius: number | null;
  sensors: TemperatureReading[];
  fans: FanReading[];
  battery: BatteryReading | null;
  processes: ProcessReading[];
  unit: "C" | "F";
  thermalPolicy: ThermalPolicySettings;
  fanActuationStatus: FanActuationStatus;
  historyWindow: HistoryWindow;
  windowedHistory: HistorySample[];
  netHistory: { rx: number; tx: number }[];
  memHistory: number[];
  recordedCount: number | null;
  sessionExportMsg: string | null;
  showHealthReport: boolean;
  setShowHealthReport: (next: boolean | ((v: boolean) => boolean)) => void;
  setHistoryWindow: (window: HistoryWindow) => void;
  handleToggleRecording: () => Promise<void>;
  setActiveTab: (tab: AppTab) => void;
  handleSelectPolicyMode: (mode: ThermalPolicyMode) => Promise<void>;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
  handleCleanupOrphans: () => Promise<void>;
}

export const HomeView = ({
  telemetry,
  extras,
  ports,
  cpuCelsius,
  gpuCelsius,
  sensors,
  fans,
  battery,
  processes,
  unit,
  thermalPolicy,
  fanActuationStatus,
  historyWindow,
  windowedHistory,
  netHistory,
  memHistory,
  recordedCount,
  sessionExportMsg,
  showHealthReport,
  setShowHealthReport,
  setHistoryWindow,
  handleToggleRecording,
  setActiveTab,
  handleSelectPolicyMode,
  handleTerminateProcess,
  handleCleanupOrphans,
}: HomeViewProps) => {
  const fan1 = fans[0];
  const recordingSession = recordedCount !== null;
  const peakCore = useMemo(() => {
    if (sensors.length === 0) return null;
    return sensors.reduce((best, s) => (s.celsius > best.celsius ? s : best), sensors[0]);
  }, [sensors]);
  const avgSensorTemp = useMemo(() => {
    if (sensors.length === 0) return null;
    return sensors.reduce((sum, s) => sum + s.celsius, 0) / sensors.length;
  }, [sensors]);
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

  const healthDiagnosis = useMemo(() => {
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
        `• Cooling: ${fan1.label} spinning at ${fan1.speed_rpm} RPM (${Math.round(fanSpeedPercent(fan1) ?? 0)}% capacity, policy: ${thermalPolicy.mode}).`,
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
  }, [cpuCelsius, unit, fan1, thermalPolicy.mode, processes, extras, exposedPorts.length]);

  const [copiedHealthReport, flashCopiedHealthReport] = useTransientValue<boolean>(1800);

  const handleCopyHealthReport = async () => {
    const md = [
      `# StatPulse Mac Health Report`,
      `- Timestamp: ${new Date().toISOString()}`,
      ...healthDiagnosis.map((l) => l.replace(/^• /, "- ")),
    ].join("\n");
    try {
      await navigator.clipboard.writeText(md);
      flashCopiedHealthReport(true);
    } catch {
    }
  };

  return (
    <>
              <div className="flex items-center justify-between gap-1.5">
                <div className="flex items-center gap-0.5 bg-[var(--rail)] p-0.5 rounded-md border border-[var(--card-border)]">
                  {HISTORY_WINDOWS.map((w) => (
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
                    {recordingSession ? `Rec (${recordedCount ?? 0})` : "Rec"}
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
                    <span>{fan1 ? `${Math.round(fanSpeedPercent(fan1) ?? 0)}%` : "None"}</span>
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
              <div className="pp-card">
                <div className="pp-card__head">
                  <span>Thermal policy</span>
                  <em>
                    {fans.length} {fans.length === 1 ? "fan" : "fans"} ·{" "}
                    {fanActuationStatus === "ready" ? "Helper ready" : "Read-only"}
                  </em>
                </div>

                <div className="grid grid-cols-4 gap-1">
                  {POLICY_MODES.map(({ id, short }) => (
                    <button
                      key={id}
                      type="button"
                      onClick={() => void handleSelectPolicyMode(id)}
                      className={`pp-btn py-1 ${thermalPolicy.mode === id ? "pp-btn--active" : ""}`}
                    >
                      {short}
                    </button>
                  ))}
                </div>

                {fans.map((f) => {
                  const pct = fanSpeedPercent(f);
                  return (
                    <div key={f.id} className="pp-kv text-[var(--ter)]">
                      <span className="truncate">{f.label}</span>
                      <div className="flex items-center gap-2 shrink-0">
                        <b className="font-mono text-[var(--ink)]">
                          {f.speed_rpm} RPM{pct !== null ? ` · ${Math.round(pct)}%` : ""}
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
  );
};
