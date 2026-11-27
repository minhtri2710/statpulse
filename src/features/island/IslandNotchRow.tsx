import { memo, type ReactNode } from "react";
import { Coffee, MicOff, Timer } from "lucide-react";
import { formatRate } from "../../services/format";
import type { AwakeStatus } from "../../types";
import { formatAwakeBadge, formatFocusCountdown } from "../awake/awakeFormat";
import { displayTemperature } from "../thermals/display";

interface IslandNotchRowProps {
  islandDotTone: "pp-dot--bad" | "pp-dot--warn" | "pp-dot--good";
  cpuCelsius: number | null;
  gpuCelsius: number | null;
  unit: "C" | "F";
  dustBlastSecs: number;
  primaryFanRpm: number | null;
  cameraActive: boolean;
  micInUseBy: string | null;
  micMuted?: boolean;
  watts: number;
  usedGb: number | null;
  rxBytesPerSec: number | null;
  agentCount: number;
  awakeStatus: AwakeStatus | null;
  focusRemainingSecs?: number | null;
  rightContent?: ReactNode;
  onClick: () => void;
  title?: string;
}

export const IslandNotchRow = memo(function IslandNotchRow({
  islandDotTone,
  cpuCelsius,
  gpuCelsius,
  unit,
  dustBlastSecs,
  primaryFanRpm,
  cameraActive,
  micInUseBy,
  micMuted = false,
  watts,
  usedGb,
  rxBytesPerSec,
  agentCount,
  awakeStatus,
  focusRemainingSecs = null,
  rightContent,
  onClick,
  title = "Click to open detail panel",
}: IslandNotchRowProps) {
  const awakeBadge = formatAwakeBadge(awakeStatus);

  return (
    <div className="di-notch-row" onClick={onClick} title={title}>
      <div className="flex items-center gap-1 min-w-0 cursor-pointer py-0.5">
        <span className={`pp-dot shrink-0 ${islandDotTone}`} />
        <span className="di-wave shrink-0" aria-hidden="true">
          <i />
          <i />
          <i />
          <i />
          <i />
        </span>
        <span className="font-mono tabular-nums text-[9px] tracking-tight font-semibold text-[var(--ink)] truncate">
          {cpuCelsius !== null ? `${Math.round(displayTemperature(cpuCelsius, unit))}` : "--"}/
          {gpuCelsius !== null ? `${Math.round(displayTemperature(gpuCelsius, unit))}°` : "--°"}{" "}
          {dustBlastSecs > 0
            ? `Blast ${dustBlastSecs}s`
            : primaryFanRpm !== null
              ? `${primaryFanRpm}rpm`
              : "Fan --"}
        </span>
      </div>

      <div className="di-notch-bridge cursor-pointer">
        {cameraActive && <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />}
        {!cameraActive && micInUseBy && (
          <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
        )}
        <span className="di-notch-glow" />
      </div>

      {rightContent ? (
        <div onClick={(e) => e.stopPropagation()} className="shrink-0">
          {rightContent}
        </div>
      ) : (
        <div className="flex items-center justify-end gap-1 min-w-0 cursor-pointer py-0.5">
          <span className="font-mono tabular-nums text-[9px] tracking-tight font-medium text-[var(--sec)] truncate">
            {watts.toFixed(1)}W {usedGb !== null ? `${usedGb.toFixed(1)}G` : "--G"} ↓
            {rxBytesPerSec !== null ? formatRate(rxBytesPerSec) : "0B"}
          </span>
          {agentCount > 0 && (
            <span className="font-mono text-[8px] font-semibold px-1 py-0.5 rounded-full bg-[var(--acc)]/15 text-[var(--acc)] shrink-0">
              {agentCount}AI
            </span>
          )}
          {micMuted && (
            <span
              className="inline-flex items-center gap-0.5 font-mono text-[8px] font-semibold px-1 py-0.5 rounded-full bg-rose-500/20 text-rose-300 border border-rose-500/30 shrink-0"
              title="Microphone muted"
            >
              <MicOff className="w-2.5 h-2.5" />
            </span>
          )}
          {focusRemainingSecs !== null && (
            <span className="inline-flex items-center gap-0.5 font-mono text-[8px] font-semibold px-1 py-0.5 rounded-full bg-cyan-400/20 text-cyan-300 border border-cyan-400/30 shrink-0">
              <Timer className="w-2.5 h-2.5" />
              {formatFocusCountdown(focusRemainingSecs)}
            </span>
          )}
          {awakeBadge && (
            <span
              className="inline-flex items-center gap-0.5 font-mono text-[8px] font-semibold px-1 py-0.5 rounded-full bg-amber-400/20 text-amber-300 border border-amber-400/30 shrink-0"
              title={
                awakeStatus?.remaining_seconds !== null && awakeStatus?.remaining_seconds !== undefined
                  ? `Keep Awake · ${awakeBadge} left`
                  : awakeStatus?.auto_rule_reason ?? "Keep Awake · Always"
              }
            >
              <Coffee className="w-2.5 h-2.5" />
              {awakeBadge}
            </span>
          )}
        </div>
      )}
    </div>
  );
});
