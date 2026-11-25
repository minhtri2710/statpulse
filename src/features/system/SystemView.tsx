import { useMemo } from "react";
import {
  ChevronRight,
  Gauge,
  HardDrive,
  Moon,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Zap,
} from "lucide-react";
import { TemperatureGauge } from "../thermals/TemperatureGauge";
import { TemperatureChart } from "../thermals/TemperatureChart";
import { CoreBreakdown } from "../thermals/CoreBreakdown";
import { BatteryCard } from "../devices/BatteryCard";
import { HISTORY_WINDOWS, type HistorySample, type HistoryWindow } from "../../services/thermalHistory";
import { formatMemory } from "../../services/format";
import type {
  BatteryReading,
  DiskBenchmarkResult,
  HardwareTelemetrySnapshot,
  SystemExtrasSnapshot,
  TemperatureReading,
} from "../../types";

interface SystemViewProps {
  telemetry: HardwareTelemetrySnapshot | null;
  extras: SystemExtrasSnapshot | null;
  sensors: TemperatureReading[];
  cpuCelsius: number | null;
  gpuCelsius: number | null;
  unit: "C" | "F";
  battery: BatteryReading | null;
  hasSmcAccess: boolean;
  historyWindow: HistoryWindow;
  windowedHistory: HistorySample[];
  setHistoryWindow: (window: HistoryWindow) => void;
  maintenanceMsg: string | null;
  handleRunMaintenance: (action: "flush_dns" | "purge_ram") => Promise<void>;
  cacheMsg: string | null;
  confirmingCacheId: string | null;
  cleaningCacheId: string | null;
  setConfirmingCacheId: (id: string | null) => void;
  handleCleanCache: (cacheId: string, label: string) => Promise<void>;
  handleCleanStaleCaches: (minAgeDays?: number) => Promise<void>;
  quickSwitchMsg: string | null;
  handleRunQuickSwitch: (
    switchId:
      | "toggle_dark_mode"
      | "toggle_hidden_files"
      | "toggle_desktop_icons"
      | "clear_clipboard"
      | "empty_trash",
  ) => Promise<void>;
  diskBench: DiskBenchmarkResult | null;
  runningDiskBench: boolean;
  diskBenchErr: string | null;
  handleRunDiskBenchmark: () => Promise<void>;
  showLaunchAgents: boolean;
  setShowLaunchAgents: (next: boolean | ((v: boolean) => boolean)) => void;
  handleRevealInFinder: (path: string) => Promise<void>;
  batteryHealthPercent?: number | null;
}

export const SystemView = ({
  telemetry,
  extras,
  sensors,
  cpuCelsius,
  gpuCelsius,
  unit,
  battery,
  hasSmcAccess,
  historyWindow,
  windowedHistory,
  setHistoryWindow,
  maintenanceMsg,
  handleRunMaintenance,
  cacheMsg,
  confirmingCacheId,
  cleaningCacheId,
  setConfirmingCacheId,
  handleCleanCache,
  handleCleanStaleCaches,
  quickSwitchMsg,
  handleRunQuickSwitch,
  diskBench,
  runningDiskBench,
  diskBenchErr,
  handleRunDiskBenchmark,
  showLaunchAgents,
  setShowLaunchAgents,
  handleRevealInFinder,
  batteryHealthPercent = null,
}: SystemViewProps) => {
  const statusMeta =
    cpuCelsius === null
      ? { label: "Waiting", cls: "pp-pill--good" }
      : cpuCelsius >= 85
        ? { label: "● Hot", cls: "pp-pill--bad" }
        : cpuCelsius >= 70
          ? { label: "● Warm", cls: "pp-pill--warn" }
          : { label: "● Cool", cls: "pp-pill--good" };
  const unsignedAgentsCount = useMemo(
    () => extras?.launch_agents.filter((a) => !a.signed || !a.binary_exists).length ?? 0,
    [extras],
  );

  return (
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
                  {HISTORY_WINDOWS.map((w) => (
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

              <TemperatureChart history={windowedHistory} windowLabel={historyWindow} unit={unit} />
              {extras && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Zap className="w-3 h-3 text-[var(--acc)]" />
                      Silicon Power Rails
                    </span>
                    <em className="font-mono">
                      {extras.silicon_power.package_watts.toFixed(1)}W pkg
                      {extras.silicon_power.adapter_watts > 0 &&
                        ` · ${extras.silicon_power.adapter_watts.toFixed(1)}W DC`}
                    </em>
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
                      {(extras.silicon_power.e_cluster_mhz > 0 || extras.silicon_power.p_cluster_mhz > 0) && (
                        <div className="grid grid-cols-3 gap-1.5 pt-1 border-t border-white/5 text-[9.5px] font-mono">
                          <div className="px-1.5 py-1 rounded bg-white/[0.03] border border-white/5 flex items-center justify-between">
                            <span className="text-[var(--sec)]">E-Core</span>
                            <span className="text-[var(--ink)]">
                              {extras.silicon_power.e_cluster_mhz}MHz · {extras.silicon_power.e_cluster_active_percent}%
                            </span>
                          </div>
                          <div className="px-1.5 py-1 rounded bg-white/[0.03] border border-white/5 flex items-center justify-between">
                            <span className="text-[var(--sec)]">P-Core</span>
                            <span className="text-[var(--ink)]">
                              {extras.silicon_power.p_cluster_mhz}MHz · {extras.silicon_power.p_cluster_active_percent}%
                            </span>
                          </div>
                          <div className="px-1.5 py-1 rounded bg-white/[0.03] border border-white/5 flex items-center justify-between">
                            <span className="text-[var(--sec)]">GPU</span>
                            <span className="text-[var(--ink)]">
                              {extras.silicon_power.gpu_mhz}MHz · {extras.silicon_power.gpu_active_percent}%
                            </span>
                          </div>
                        </div>
                      )}
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
              {extras && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <HardDrive className="w-3 h-3" />
                      Free up space
                    </span>
                    <div className="flex items-center gap-1.5">
                      {extras.caches.some((c) => c.age_days >= 30) && (
                        <button
                          type="button"
                          disabled={cleaningCacheId !== null}
                          onClick={() => void handleCleanStaleCaches(30)}
                          className="pp-btn pp-btn--danger text-[9px] py-0.5 px-1.5 disabled:opacity-40"
                          title="Clean all rebuildable caches untouched for 30+ days"
                        >
                          {cleaningCacheId === "__stale__" ? "Sweeping..." : "Clean Stale >30d"}
                        </button>
                      )}
                      <em>
                        {extras.disk.free_gb.toFixed(0)} GB free ({extras.disk.used_percent}% used)
                        {extras.disk.apfs_snapshots_count > 0
                          ? ` · ${extras.disk.apfs_snapshots_count} APFS snaps`
                          : ""}
                      </em>
                    </div>
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
                          <span className="truncate text-[var(--ter)] flex items-center gap-1.5">
                            <span className="truncate">{c.label}</span>
                            {c.age_days >= 1 && (
                              <span className="text-[8.5px] font-mono px-1 rounded bg-[var(--rail)] text-[var(--sec)] border border-[var(--card-border)] shrink-0">
                                {c.age_days}d
                              </span>
                            )}
                          </span>
                          <div className="flex items-center gap-2 shrink-0">
                            <b className="font-mono text-[var(--ink)]">{formatMemory(c.size_mb)}</b>
                            {confirmingCacheId === c.id ? (
                              <div className="flex flex-col items-end gap-1">
                                <span className="max-w-[220px] whitespace-normal break-all text-right font-mono text-[9px] text-[var(--sec)]" title={c.path}>
                                  {c.path}
                                </span>
                                <div className="flex items-center gap-1">
                                  <button
                                    type="button"
                                    disabled={cleaningCacheId !== null}
                                    onClick={() => setConfirmingCacheId(null)}
                                    className="pp-btn inline-flex items-center gap-1 disabled:opacity-40"
                                  >
                                    Cancel
                                  </button>
                                  <button
                                    type="button"
                                    disabled={cleaningCacheId !== null}
                                    onClick={() => void handleCleanCache(c.id, c.label)}
                                    className="pp-btn pp-btn--danger inline-flex items-center gap-1 disabled:opacity-40"
                                  >
                                    {cleaningCacheId === c.id ? "..." : "Confirm"}
                                  </button>
                                </div>
                              </div>
                            ) : (
                              <button
                                type="button"
                                disabled={cleaningCacheId !== null || confirmingCacheId !== null}
                                onClick={() => setConfirmingCacheId(c.id)}
                                className="pp-btn pp-btn--danger inline-flex items-center gap-1 disabled:opacity-40"
                              >
                                <Trash2 className="w-2.5 h-2.5" />
                                Clean
                              </button>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {extras && (
                <>
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

              <BatteryCard battery={battery} unit={unit} healthPercent={batteryHealthPercent} />
    </>
  );
};
