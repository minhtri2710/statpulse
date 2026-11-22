import React, { useState } from "react";
import { Flame, ChevronDown, AlertCircle } from "lucide-react";
import { Availability, ProcessReading } from "../types";

interface ThermalCulpritsCardProps {
  topProcesses: Availability<ProcessReading[]>;
  compact?: boolean;
}

function formatMemory(memoryMb: number): string {
  if (memoryMb >= 1024) {
    return `${(memoryMb / 1024).toFixed(1)} GB`;
  }
  return `${Math.round(memoryMb)} MB`;
}

function cpuTone(cpuPercent: number): string {
  if (cpuPercent >= 50) return "bg-rose-400 text-rose-300";
  if (cpuPercent >= 20) return "bg-amber-400 text-amber-300";
  return "bg-cyan-400 text-cyan-300";
}

export const ThermalCulpritsCard: React.FC<ThermalCulpritsCardProps> = ({
  topProcesses,
}) => {
  const [expanded, setExpanded] = useState(false);

  if (topProcesses.status === "not_present") {
    return null;
  }

  if (topProcesses.status === "unavailable") {
    return (
      <div className="glass-card p-2.5 rounded-xl flex items-center gap-2 text-[10px] text-slate-400">
        <AlertCircle className="w-3.5 h-3.5 text-slate-500 shrink-0" />
        <span className="truncate">Process telemetry unavailable: {topProcesses.reason}</span>
      </div>
    );
  }

  const processes = topProcesses.value;
  const summary =
    processes.length > 0
      ? processes
          .slice(0, 2)
          .map((p) => `${p.name} ${p.cpu_percent.toFixed(1)}%`)
          .join(" · ")
      : "Idle (< 0.1%)";

  return (
    <div className="glass-card p-2.5 rounded-xl flex flex-col gap-2">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        className="w-full flex items-center justify-between gap-2 text-left outline-none group"
      >
        <div className="flex items-center gap-1.5 text-[11px] font-semibold text-slate-300 shrink-0">
          <Flame className="w-3.5 h-3.5 text-amber-400 shrink-0" />
          <span>Top Thermal Load</span>
        </div>

        <div className="flex items-center gap-1.5 min-w-0">
          {!expanded && (
            <span className="text-[10px] font-mono text-slate-400 truncate">
              {summary}
            </span>
          )}
          <span className="text-[9px] font-mono text-slate-400 px-1.5 py-0.5 rounded bg-white/5 border border-white/10 shrink-0">
            {processes.length} apps
          </span>
          <ChevronDown
            className={`w-3.5 h-3.5 text-slate-400 group-hover:text-white transition-transform shrink-0 ${
              expanded ? "rotate-180 text-cyan-400" : ""
            }`}
          />
        </div>
      </button>

      {expanded && (
        processes.length === 0 ? (
          <div className="text-[10px] text-slate-400 py-0.5">
            All background apps are idle (&lt; 0.1% CPU).
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {processes.map((proc) => {
              const tone = cpuTone(proc.cpu_percent);
              const [barColor, textColor] = tone.split(" ");
              const barWidth = Math.min(Math.max(proc.cpu_percent, 4), 100);

              return (
                <div
                  key={proc.name}
                  className="px-2.5 py-1.5 rounded-lg bg-slate-900/55 border border-white/5 flex items-center justify-between gap-2"
                >
                  <div className="flex items-center gap-1.5 min-w-0 flex-1">
                    <span className="text-[11px] font-semibold text-slate-200 truncate">
                      {proc.name}
                    </span>
                    {proc.process_count > 1 && (
                      <span className="px-1 py-0.2 rounded bg-white/5 border border-white/10 text-[9px] font-mono text-slate-400 shrink-0">
                        ×{proc.process_count}
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-2.5 shrink-0">
                    <div className="w-12 h-1.5 rounded-full bg-slate-800/90 overflow-hidden">
                      <div
                        className={`h-full rounded-full ${barColor}`}
                        style={{ width: `${barWidth}%` }}
                      />
                    </div>
                    <span className={`text-[10px] font-mono font-bold w-11 text-right ${textColor}`}>
                      {proc.cpu_percent.toFixed(1)}%
                    </span>
                    <span className="text-[10px] font-mono text-slate-400 w-13 text-right">
                      {formatMemory(proc.memory_mb)}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        )
      )}
    </div>
  );
};
