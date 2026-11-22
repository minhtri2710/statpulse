import { displayTemperature } from "../features/thermals/display";
import React from "react";
import { TrendingUp } from "lucide-react";
import type { HistorySample, HistoryWindow } from "../services/thermalHistory";

interface TemperatureChartProps {
  history: HistorySample[];
  windowLabel: HistoryWindow;
  unit: "C" | "F";
}

export const TemperatureChart: React.FC<TemperatureChartProps> = ({ history, windowLabel, unit }) => {
  if (history.length < 2) return null;

  const data = history;

  const formatVal = (v: number) => Math.round(displayTemperature(v, unit));

  // Scale contains every retained point: bounds come from the data itself
  // with padding, never a clamp that could invert the range.
  const cpuVals = data.map((d) => d.cpu);
  const gpuVals = data.flatMap((d) => (d.gpu === null ? [] : [d.gpu]));
  const allVals = [...cpuVals, ...gpuVals];
  const minTemp = Math.min(...allVals) - 5;
  const maxTemp = Math.max(...allVals) + 5;
  const range = maxTemp - minTemp || 1;

  const width = 340;
  const height = 64;

  const firstTime = data[0].time;
  const lastTime = data[data.length - 1].time;
  const duration = Math.max(1, lastTime - firstTime);
  const getX = (idx: number) => ((data[idx].time - firstTime) / duration) * width;
  const getY = (val: number) => height - ((val - minTemp) / range) * (height - 8) - 4;

  const cpuPoints = data.map((d, i) => `${getX(i)},${getY(d.cpu)}`).join(" ");
  const gpuPoints = data
    .map((d, i) => (d.gpu === null ? null : `${getX(i)},${getY(d.gpu)}`))
    .filter((point): point is string => point !== null)
    .join(" ");

  const latestCpu = data[data.length - 1]?.cpu || 0;
  const latestGpu = data[data.length - 1]?.gpu ?? null;

  return (
    <div className="glass-card p-2.5 rounded-xl flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-2 whitespace-nowrap">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold text-slate-300 shrink-0">
          <TrendingUp className="w-3.5 h-3.5 text-cyan-400" />
          Temp History ({windowLabel})
        </div>
        <div className="flex items-center gap-2.5 text-[10px] shrink-0">
          <span className="flex items-center gap-1 text-cyan-400 font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 inline-block" />
            CPU: {formatVal(latestCpu)}°{unit}
          </span>
          <span className="flex items-center gap-1 text-amber-400 font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 inline-block" />
            GPU: {latestGpu === null ? "--" : `${formatVal(latestGpu)}°${unit}`}
          </span>
        </div>
      </div>

      <div className="relative w-full h-11 bg-slate-950/60 rounded-lg overflow-hidden border border-white/5 pt-0.5">
        <svg
          viewBox={`0 0 ${width} ${height}`}
          preserveAspectRatio="none"
          className="w-full h-full overflow-visible"
        >
          <defs>
            <linearGradient id="cpuGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#22d3ee" stopOpacity="0.3" />
              <stop offset="100%" stopColor="#22d3ee" stopOpacity="0.0" />
            </linearGradient>
            <linearGradient id="gpuGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="#fbbf24" stopOpacity="0.25" />
              <stop offset="100%" stopColor="#fbbf24" stopOpacity="0.0" />
            </linearGradient>
          </defs>
          <line
            x1="0"
            y1={height * 0.25}
            x2={width}
            y2={height * 0.25}
            stroke="rgba(255,255,255,0.05)"
            strokeDasharray="3 3"
            vectorEffect="non-scaling-stroke"
          />
          <line
            x1="0"
            y1={height * 0.75}
            x2={width}
            y2={height * 0.75}
            stroke="rgba(255,255,255,0.05)"
            strokeDasharray="3 3"
            vectorEffect="non-scaling-stroke"
          />
          <polygon
            points={`0,${height} ${cpuPoints} ${width},${height}`}
            fill="url(#cpuGradient)"
          />
          <polyline
            fill="none"
            stroke="#22d3ee"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
            strokeLinecap="round"
            strokeLinejoin="round"
            points={cpuPoints}
          />

          {gpuPoints && (
            <>
              <polygon
                points={`0,${height} ${gpuPoints} ${width},${height}`}
                fill="url(#gpuGradient)"
              />
              <polyline
                fill="none"
                stroke="#fbbf24"
                strokeWidth="1.5"
                vectorEffect="non-scaling-stroke"
                strokeLinecap="round"
                strokeLinejoin="round"
                points={gpuPoints}
              />
            </>
          )}
        </svg>
      </div>
    </div>
  );
};
