import { Wind, Zap } from "lucide-react";
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
                    {autoThermal.performance_on_heavy_load || autoThermal.quiet_on_battery
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
