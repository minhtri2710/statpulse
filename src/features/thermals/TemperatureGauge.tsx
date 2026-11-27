import { formatTempShort, gaugeTier, gaugeWidth } from "./display";
import React from "react";
import { Cpu, Zap, Activity } from "lucide-react";

interface TemperatureGaugeProps {
  cpuTemp: number | null;
  gpuTemp: number | null;
  unit: "C" | "F";
}

export const TemperatureGauge: React.FC<TemperatureGaugeProps> = ({ cpuTemp, gpuTemp, unit }) => {
  const getTempColors = (val: number | null) => {
    if (val === null) return { bar: "from-slate-600 to-slate-700", text: "text-slate-400" };
    switch (gaugeTier(val)) {
      case "cool":
        return { bar: "from-emerald-500 to-teal-400", text: "text-emerald-400" };
      case "nominal":
        return { bar: "from-cyan-500 to-blue-500", text: "text-cyan-400" };
      case "warm":
        return { bar: "from-amber-500 to-orange-400", text: "text-amber-400" };
      default:
        return { bar: "from-rose-600 to-red-500", text: "text-rose-400 animate-pulse" };
    }
  };

  const cpuColors = getTempColors(cpuTemp);
  const gpuColors = getTempColors(gpuTemp);
  const getProgressWidth = (val: number | null) => (val === null ? 0 : gaugeWidth(val));

  return (
    <div className="grid grid-cols-2 gap-2.5">
      <div className="glass-card glass-card-hover p-2.5 rounded-xl flex flex-col justify-between">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold text-slate-300">
            <Cpu className="w-3.5 h-3.5 text-cyan-400" />
            CPU Temp
          </div>
          <Activity className="w-3 h-3 text-slate-500" />
        </div>

        <div className="my-1 flex items-baseline justify-between gap-1.5">
          <div className={`text-xl font-extrabold tracking-tight leading-none ${cpuColors.text}`}>
            {formatTempShort(cpuTemp, unit)}
          </div>
          <div className="text-[9.5px] text-slate-400 truncate">Apple Silicon / Intel</div>
        </div>
        <div className="w-full bg-slate-800/80 rounded-full h-1.5 overflow-hidden border border-white/5">
          <div
            className={`h-full rounded-full bg-gradient-to-r ${cpuColors.bar} transition-all duration-500`}
            style={{ width: `${getProgressWidth(cpuTemp)}%` }}
          />
        </div>
      </div>
      <div className="glass-card glass-card-hover p-2.5 rounded-xl flex flex-col justify-between">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold text-slate-300">
            <Zap className="w-3.5 h-3.5 text-amber-400" />
            GPU Temp
          </div>
          <Activity className="w-3 h-3 text-slate-500" />
        </div>

        <div className="my-1 flex items-baseline justify-between gap-1.5">
          <div className={`text-xl font-extrabold tracking-tight leading-none ${gpuColors.text}`}>
            {formatTempShort(gpuTemp, unit)}
          </div>
          <div className="text-[9.5px] text-slate-400 truncate">Graphics Cluster</div>
        </div>
        <div className="w-full bg-slate-800/80 rounded-full h-1.5 overflow-hidden border border-white/5">
          <div
            className={`h-full rounded-full bg-gradient-to-r ${gpuColors.bar} transition-all duration-500`}
            style={{ width: `${getProgressWidth(gpuTemp)}%` }}
          />
        </div>
      </div>
    </div>
  );
};
