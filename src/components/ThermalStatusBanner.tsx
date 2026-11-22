import React from "react";
import { FanReading, ThermalPolicyMode } from "../types";

interface ThermalStatusBannerProps {
  cpuCelsius: number | null;
  fans: FanReading[];
  powerWatts: number | null;
  policyMode: ThermalPolicyMode;
  onCyclePolicy?: () => void;
}

const POLICY_LABELS: Record<ThermalPolicyMode, string> = {
  system_auto: "Auto",
  quiet: "Quiet",
  performance: "Perf",
  custom: "Custom",
};

export const ThermalStatusBanner: React.FC<ThermalStatusBannerProps> = ({
  cpuCelsius,
  fans,
  powerWatts,
  policyMode,
  onCyclePolicy,
}) => {
  const status = (() => {
    if (cpuCelsius === null) {
      return {
        label: "Reading sensors",
        dotClass: "bg-slate-400",
        textClass: "text-slate-300",
      };
    }
    if (cpuCelsius >= 85) {
      return {
        label: "Heavy thermal load",
        dotClass: "bg-rose-400 animate-pulse",
        textClass: "text-rose-300",
      };
    }
    if (cpuCelsius >= 70) {
      return {
        label: "Warm · active",
        dotClass: "bg-amber-400",
        textClass: "text-amber-300",
      };
    }
    if (cpuCelsius >= 55) {
      return {
        label: "Nominal",
        dotClass: "bg-cyan-400",
        textClass: "text-cyan-300",
      };
    }
    return {
      label: "Cool",
      dotClass: "bg-emerald-400",
      textClass: "text-emerald-300",
    };
  })();

  const fanSummary = (() => {
    if (fans.length === 0) return "Passive cooling";
    const maxRpm = Math.max(...fans.map((f) => f.speed_rpm));
    if (maxRpm === 0) return "Silent · 0 RPM";
    const pcts = fans
      .map((f) => {
        if (f.min_speed_rpm === null || f.max_speed_rpm === null || f.max_speed_rpm <= f.min_speed_rpm) {
          return null;
        }
        return Math.round(
          Math.min(
            Math.max(((f.speed_rpm - f.min_speed_rpm) / (f.max_speed_rpm - f.min_speed_rpm)) * 100, 0),
            100,
          ),
        );
      })
      .filter((v): v is number => v !== null);
    const maxPct = pcts.length > 0 ? Math.max(...pcts) : null;
    return maxPct !== null ? `${maxRpm} RPM (${maxPct}%)` : `${maxRpm} RPM`;
  })();

  return (
    <div className="px-3.5 py-1.5 border-b border-white/10 bg-slate-950/35 flex items-center justify-between gap-2 text-[10px] shrink-0">
      <div className="flex items-center gap-1.5 min-w-0">
        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${status.dotClass}`} />
        <span className={`font-semibold truncate ${status.textClass}`}>{status.label}</span>
        <span className="text-slate-600">·</span>
        <span className="text-slate-300 font-mono truncate">{fanSummary}</span>
        {powerWatts !== null && (
          <>
            <span className="text-slate-600">·</span>
            <span className="text-amber-300/90 font-mono shrink-0">{powerWatts.toFixed(1)}W</span>
          </>
        )}
      </div>

      {onCyclePolicy ? (
        <button
          type="button"
          onClick={onCyclePolicy}
          title="Click to cycle Thermal Policy"
          className="px-1.5 py-0.5 rounded bg-white/5 hover:bg-white/10 border border-white/10 text-slate-300 hover:text-white font-mono text-[9px] font-semibold transition-colors shrink-0"
        >
          {POLICY_LABELS[policyMode]}
        </button>
      ) : (
        <span className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-slate-300 font-mono text-[9px] font-semibold shrink-0">
          {POLICY_LABELS[policyMode]}
        </span>
      )}
    </div>
  );
};
