import { useState } from "react";
import { Plus, Wind, X, Zap } from "lucide-react";
import { FanRuleManager } from "./FanRuleManager";
import { FanCard } from "./FanCard";
import { unavailableReason } from "../../services/format";
import type {
  AutoThermalTriggers,
  FanReading,
  HardwareTelemetrySnapshot,
  TemperatureReading,
  ThermalPolicyMode,
  ThermalPolicySettings,
  ThermalRule,
  FanActuationStatus,
} from "../../types";

interface FansViewProps {
  telemetry: HardwareTelemetrySnapshot | null;
  policyError: string | null;
  thermalPolicy: ThermalPolicySettings;
  autoThermal: AutoThermalTriggers;
  onUpdateAutoThermal: (next: AutoThermalTriggers) => Promise<void>;
  sensors: TemperatureReading[];
  fans: FanReading[];
  cpuCelsius: number | null;
  gpuCelsius: number | null;
  unit: "C" | "F";
  fanActuationStatus: FanActuationStatus;
  dustBlastSecs: number;
  handleSelectPolicyMode: (mode: ThermalPolicyMode) => Promise<void>;
  handleSaveRule: (rule: ThermalRule) => Promise<void>;
  handleDeleteRule: (ruleId: string) => Promise<void>;
  handleSetFanSpeed: (fanId: number, rpm: number) => Promise<void>;
  handleSetFanMode: (fanId: number, mode: "auto" | "manual", rpm?: number) => Promise<void>;
  handleEnableHelper: () => Promise<void>;
  handleStartDustBlast: () => Promise<void>;
  handleCancelDustBlast: () => Promise<void>;
}

export const FansView = ({
  telemetry,
  policyError,
  thermalPolicy,
  autoThermal,
  onUpdateAutoThermal,
  sensors,
  fans,
  cpuCelsius,
  gpuCelsius,
  unit,
  fanActuationStatus,
  dustBlastSecs,
  handleSelectPolicyMode,
  handleSaveRule,
  handleDeleteRule,
  handleSetFanSpeed,
  handleSetFanMode,
  handleEnableHelper,
  handleStartDustBlast,
  handleCancelDustBlast,
}: FansViewProps) => {
  const [newBoostApp, setNewBoostApp] = useState("");
  const perfApps = autoThermal.performance_apps ?? [];
  const quickBoostPresets = ["Xcode", "cargo", "Blender", "Ollama", "Docker"];

  const addBoostApp = (rawName: string) => {
    const name = rawName.trim();
    if (!name || perfApps.some((a) => a.toLowerCase() === name.toLowerCase())) return;
    void onUpdateAutoThermal({
      ...autoThermal,
      performance_apps: [...perfApps, name],
    });
    setNewBoostApp("");
  };

  const removeBoostApp = (name: string) => {
    void onUpdateAutoThermal({
      ...autoThermal,
      performance_apps: perfApps.filter((a) => a !== name),
    });
  };

  return (
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

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Zap className="w-3 h-3 text-[var(--acc)]" />
                    Auto Thermal Triggers
                  </span>
                  <em>
                    {autoThermal.performance_on_heavy_load ||
                    autoThermal.quiet_on_battery ||
                    perfApps.length > 0
                      ? "Armed"
                      : "Off"}
                  </em>
                </div>
                <div className="grid grid-cols-2 gap-1.5">
                  <button
                    type="button"
                    onClick={() =>
                      void onUpdateAutoThermal({
                        ...autoThermal,
                        performance_on_heavy_load: !autoThermal.performance_on_heavy_load,
                      })
                    }
                    className={`pp-btn py-1.5 text-left px-2 ${
                      autoThermal.performance_on_heavy_load ? "pp-btn--active" : ""
                    }`}
                  >
                    Boost Perf ≥74°C
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void onUpdateAutoThermal({
                        ...autoThermal,
                        quiet_on_battery: !autoThermal.quiet_on_battery,
                      })
                    }
                    className={`pp-btn py-1.5 text-left px-2 ${
                      autoThermal.quiet_on_battery ? "pp-btn--active" : ""
                    }`}
                  >
                    Quiet on Battery
                  </button>
                </div>

                <div className="pt-1.5 border-t border-white/5 flex flex-col gap-1.5">
                  <div className="flex items-center justify-between text-[9.5px] text-[var(--sec)]">
                    <span>Auto-Perf when active (&ge;5% CPU)</span>
                    <span className="font-mono">{perfApps.length} apps</span>
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {perfApps.map((app) => (
                      <span
                        key={app}
                        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[var(--surface)] border border-[var(--acc)]/35 text-[9.5px] font-mono text-[var(--ink)]"
                      >
                        {app}
                        <button
                          type="button"
                          onClick={() => removeBoostApp(app)}
                          className="text-[var(--sec)] hover:text-rose-300 cursor-pointer"
                          aria-label={`Remove ${app}`}
                        >
                          <X className="w-2.5 h-2.5" />
                        </button>
                      </span>
                    ))}
                    {quickBoostPresets
                      .filter((p) => !perfApps.some((a) => a.toLowerCase() === p.toLowerCase()))
                      .map((preset) => (
                        <button
                          key={preset}
                          type="button"
                          onClick={() => addBoostApp(preset)}
                          className="px-1.5 py-0.5 rounded border border-dashed border-white/15 text-[9px] font-mono text-[var(--sec)] hover:text-[var(--ink)] hover:border-[var(--acc)]/40 cursor-pointer"
                        >
                          + {preset}
                        </button>
                      ))}
                  </div>
                  <div className="flex items-center gap-1">
                    <input
                      type="text"
                      value={newBoostApp}
                      onChange={(e) => setNewBoostApp(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          addBoostApp(newBoostApp);
                        }
                      }}
                      placeholder="Add app/process name (e.g. HandBrake)..."
                      className="flex-1 bg-[var(--rail)] border border-[var(--card-border)] rounded px-2.5 py-1.5 text-[10px] font-mono text-[var(--ink)] outline-none focus:border-[var(--acc)]"
                    />
                    <button
                      type="button"
                      onClick={() => addBoostApp(newBoostApp)}
                      className="pp-btn inline-flex items-center gap-0.5"
                    >
                      <Plus className="w-2.5 h-2.5" />
                      Add
                    </button>
                  </div>
                </div>
              </div>

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
  );
};
