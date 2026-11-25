import { displayTemperature, fanSpeedPercent } from "./display";
import { evalCurvePercent, resolveTargetTemp } from "./curve";
import { POLICY_MODES } from "./policyModes";
import React, { useMemo, useState } from "react";
import presets from "../../../shared/thermal-presets.json";
import { Sliders, Shield, Flame, Zap, Plus, Check, Trash2, Cpu, Thermometer, Pencil, Activity } from "lucide-react";
import {
  FanReading,
  TemperatureReading,
  ThermalPolicyMode,
  ThermalRule,
  ThermalTarget,
} from "../../types";

interface FanRuleManagerProps {
  activePreset: ThermalPolicyMode;
  customRules: ThermalRule[];
  sensors: TemperatureReading[];
  fans?: FanReading[];
  currentCpuCelsius?: number | null;
  currentGpuCelsius?: number | null;
  unit?: "C" | "F";
  defaultShowCurve?: boolean;
  onSelectPreset: (preset: ThermalPolicyMode) => void;
  onSaveRule: (rule: ThermalRule) => void;
  onDeleteRule: (id: string) => void;
}

const QUIET_CURVE = presets.quiet as ThermalRule;
const PERFORMANCE_CURVE = presets.performance as ThermalRule;
const cyan = "bg-cyan-500/20 border-cyan-500/50 text-white shadow-md shadow-cyan-500/10";
const presetCards = {
  system_auto: { Icon: Shield, tint: "text-emerald-400", check: "text-cyan-400", active: cyan, help: "Default SMC" },
  quiet: { Icon: Zap, tint: "text-cyan-400", check: "text-cyan-400", active: cyan, help: `${QUIET_CURVE.low_celsius}–${QUIET_CURVE.high_celsius}°C` },
  performance: { Icon: Flame, tint: "text-amber-400", check: "text-amber-400", active: "bg-amber-500/20 border-amber-500/50 text-white shadow-md shadow-amber-500/10", help: `${PERFORMANCE_CURVE.low_celsius}–${PERFORMANCE_CURVE.high_celsius}°C` },
  custom: { Icon: Sliders, tint: "text-purple-400", check: "text-purple-400", active: "bg-purple-500/20 border-purple-500/50 text-white shadow-md shadow-purple-500/10", help: "User Rules" },
} satisfies Record<ThermalPolicyMode, { Icon: typeof Shield; tint: string; check: string; active: string; help: string }>;

function formatTempTick(celsius: number, unit: "C" | "F"): string {
  return `${Math.round(displayTemperature(celsius, unit))}°`;
}

export const FanRuleManager: React.FC<FanRuleManagerProps> = ({
  activePreset,
  customRules,
  sensors,
  fans = [],
  currentCpuCelsius = null,
  currentGpuCelsius = null,
  unit = "C",
  defaultShowCurve = false,
  onSelectPreset,
  onSaveRule,
  onDeleteRule,
}) => {
  const [editing, setEditing] = useState(false);
  const [showCurve, setShowCurve] = useState(defaultShowCurve);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [targetSensor, setTargetSensor] = useState<string>("hottest");
  const [lowTemp, setLowTemp] = useState<number>(45);
  const [highTemp, setHighTemp] = useState<number>(80);
  const [minFan, setMinFan] = useState<number>(20);
  const [maxFan, setMaxFan] = useState<number>(100);
  const [ruleName, setRuleName] = useState<string>("Custom Thermal Profile");

  const resetForm = () => {
    setEditingRuleId(null);
    setRuleName("Custom Thermal Profile");
    setTargetSensor("hottest");
    setLowTemp(45);
    setHighTemp(80);
    setMinFan(20);
    setMaxFan(100);
  };

  const handleStartNewRule = () => {
    resetForm();
    setEditing(true);
  };

  const handleStartEditRule = (rule: ThermalRule) => {
    setEditingRuleId(rule.id);
    setRuleName(rule.name);
    setTargetSensor(rule.target.type === "sensor_key" ? rule.target.key : rule.target.type);
    setLowTemp(rule.low_celsius);
    setHighTemp(rule.high_celsius);
    setMinFan(rule.min_fan_percent);
    setMaxFan(rule.max_fan_percent);
    setEditing(true);
  };

  const handleSaveRuleSubmit = () => {
    const target: ThermalTarget =
      targetSensor === "hottest" || targetSensor === "cpu" || targetSensor === "gpu"
        ? { type: targetSensor }
        : { type: "sensor_key", key: targetSensor };
    const ruleToSave: ThermalRule = {
      id: editingRuleId || Date.now().toString(),
      name: ruleName,
      target,
      low_celsius: lowTemp,
      high_celsius: highTemp,
      min_fan_percent: minFan,
      max_fan_percent: maxFan,
      active: true,
    };
    onSaveRule(ruleToSave);
    onSelectPreset("custom");
    setEditing(false);
    resetForm();
  };

  const previewTarget: ThermalTarget = useMemo(() => {
    if (targetSensor === "hottest" || targetSensor === "cpu" || targetSensor === "gpu") {
      return { type: targetSensor };
    }
    return { type: "sensor_key", key: targetSensor };
  }, [targetSensor]);

  const displayedCurves = useMemo<ThermalRule[]>(() => {
    if (editing) {
      return [
        {
          id: editingRuleId || "preview",
          name: ruleName || "Preview",
          target: previewTarget,
          low_celsius: Math.min(lowTemp, highTemp - 1),
          high_celsius: Math.max(highTemp, lowTemp + 1),
          min_fan_percent: minFan,
          max_fan_percent: Math.max(maxFan, minFan),
          active: true,
        },
      ];
    }
    if (activePreset === "quiet") return [QUIET_CURVE];
    if (activePreset === "performance") return [PERFORMANCE_CURVE];
    if (activePreset === "custom") {
      return customRules.filter((r) => r.active);
    }
    return [];
  }, [
    editing,
    editingRuleId,
    ruleName,
    previewTarget,
    lowTemp,
    highTemp,
    minFan,
    maxFan,
    activePreset,
    customRules,
  ]);

  const primaryCurve = displayedCurves[0] ?? null;

  const liveOperatingPoint = useMemo(() => {
    const temp = primaryCurve
      ? resolveTargetTemp(primaryCurve.target, currentCpuCelsius, currentGpuCelsius, sensors)
      : resolveTargetTemp({ type: "hottest" }, currentCpuCelsius, currentGpuCelsius, sensors);
    if (temp === null) return null;

    let fanPct = fanSpeedPercent(fans[0]);
    if (fanPct === null && primaryCurve) {
      fanPct = evalCurvePercent(primaryCurve, temp);
    }
    return { temp, fanPct: fanPct ?? 0 };
  }, [primaryCurve, currentCpuCelsius, currentGpuCelsius, sensors, fans]);

  const svgW = 320;
  const svgH = 88;
  const left = 24;
  const right = 308;
  const top = 10;
  const bottom = 70;
  const toX = (c: number) => left + ((Math.min(Math.max(c, 30), 100) - 30) / 70) * (right - left);
  const toY = (pct: number) => bottom - (Math.min(Math.max(pct, 0), 100) / 100) * (bottom - top);

  const curveAccent =
    editing || activePreset === "custom"
      ? "#c084fc"
      : activePreset === "performance"
        ? "var(--warn)"
        : "var(--acc)";

  return (
    <div className="glass-card p-2.5 rounded-xl flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold text-slate-300">
          <Sliders className="w-3.5 h-3.5 text-cyan-400" />
          Smart Thermal Control
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            type="button"
            onClick={() => setShowCurve((v) => !v)}
            aria-expanded={showCurve || editing}
            className={`px-2 py-0.5 rounded-lg border text-[10px] font-semibold flex items-center gap-1 transition-all ${
              showCurve || editing
                ? "bg-purple-500/20 text-purple-200 border-purple-400/40"
                : "bg-slate-900/60 text-slate-300 border-white/10 hover:text-white hover:border-white/20"
            }`}
          >
            <Activity className="w-3 h-3" />
            Curve
          </button>
          <button
            type="button"
            onClick={handleStartNewRule}
            className="px-2 py-0.5 rounded-lg bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 text-[10px] font-bold flex items-center gap-1 hover:bg-cyan-500/30 transition-all shadow-sm"
          >
            <Plus className="w-3 h-3" />
            New Rule
          </button>
        </div>
      </div>
      <div className="grid grid-cols-4 gap-1.5 text-[11px]">
        {POLICY_MODES.map(({ id, label }) => {
          const card = presetCards[id];
          return (
            <button
              key={id}
              type="button"
              onClick={() => onSelectPreset(id)}
              className={`p-2 rounded-xl border text-left transition-all ${
                activePreset === id
                  ? card.active
                  : "bg-slate-900/40 border-white/5 text-slate-400 hover:text-white"
              }`}
            >
              <div className="flex items-center justify-between mb-0.5">
                <card.Icon className={`w-3.5 h-3.5 ${card.tint}`} />
                {activePreset === id && <Check className={`w-3 h-3 ${card.check}`} />}
              </div>
              <div className="font-bold text-[10.5px] truncate">{label}</div>
              <div className="text-[9px] text-slate-400 truncate">{card.help}</div>
            </button>
          );
        })}
      </div>
      {(showCurve || editing) && (
      <div className="rounded-xl bg-slate-950/65 border border-white/5 p-2 flex flex-col gap-1">
        <div className="flex items-center justify-between text-[9px] text-slate-400 font-mono px-0.5">
          <span>
            {editing
              ? "Draft Response Curve"
              : activePreset === "system_auto"
                ? "macOS SMC Curve (Reference)"
                : "Active Thermal Response Curve"}
          </span>
          <span className="text-slate-400">2°C hysteresis · 400 RPM/s</span>
        </div>

        <svg viewBox={`0 0 ${svgW} ${svgH}`} className="w-full h-20 overflow-visible">
          {[0, 50, 100].map((pct) => (
            <g key={pct}>
              <line
                x1={left}
                y1={toY(pct)}
                x2={right}
                y2={toY(pct)}
                stroke="rgba(255,255,255,0.06)"
                strokeDasharray={pct === 0 ? undefined : "2 3"}
              />
              <text
                x={left - 4}
                y={toY(pct) + 3}
                textAnchor="end"
                className="fill-slate-500 font-mono"
                fontSize="8"
              >
                {pct}%
              </text>
            </g>
          ))}
          {[30, 50, 70, 90, 100].map((c) => (
            <g key={c}>
              <line
                x1={toX(c)}
                y1={top}
                x2={toX(c)}
                y2={bottom}
                stroke="rgba(255,255,255,0.04)"
              />
              <text
                x={toX(c)}
                y={bottom + 12}
                textAnchor="middle"
                className="fill-slate-500 font-mono"
                fontSize="8"
              >
                {formatTempTick(c, unit)}
              </text>
            </g>
          ))}
          {displayedCurves.length === 0 && (
            <polyline
              fill="none"
              stroke="rgba(52, 211, 153, 0.55)"
              strokeWidth="1.8"
              strokeDasharray="4 3"
              points={`${toX(30)},${toY(0)} ${toX(48)},${toY(0)} ${toX(88)},${toY(82)} ${toX(100)},${toY(100)}`}
            />
          )}
          {displayedCurves.map((curve, idx) => {
            const isPrimary = idx === 0;
            const pts = `${toX(30)},${toY(curve.min_fan_percent)} ${toX(curve.low_celsius)},${toY(curve.min_fan_percent)} ${toX(curve.high_celsius)},${toY(curve.max_fan_percent)} ${toX(100)},${toY(curve.max_fan_percent)}`;
            const hysteresisPoly = `${toX(curve.low_celsius - 2)},${toY(curve.min_fan_percent)} ${toX(curve.high_celsius - 2)},${toY(curve.max_fan_percent)} ${toX(curve.high_celsius)},${toY(curve.max_fan_percent)} ${toX(curve.low_celsius)},${toY(curve.min_fan_percent)}`;
            const fillPoly = `${toX(30)},${bottom} ${pts} ${toX(100)},${bottom}`;

            return (
              <g key={curve.id}>
                {isPrimary && (
                  <>
                    <polygon points={fillPoly} fill={curveAccent} fillOpacity="0.09" />
                    <polygon points={hysteresisPoly} fill={curveAccent} fillOpacity="0.22" />
                  </>
                )}
                <polyline
                  fill="none"
                  stroke={curveAccent}
                  strokeOpacity={isPrimary ? 1 : 0.45}
                  strokeWidth={isPrimary ? 2.2 : 1.3}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  points={pts}
                />
                {isPrimary && (
                  <>
                    <circle
                      cx={toX(curve.low_celsius)}
                      cy={toY(curve.min_fan_percent)}
                      r="2.8"
                      fill={curveAccent}
                    />
                    <circle
                      cx={toX(curve.high_celsius)}
                      cy={toY(curve.max_fan_percent)}
                      r="2.8"
                      fill={curveAccent}
                    />
                  </>
                )}
              </g>
            );
          })}
          {liveOperatingPoint && (
            <g>
              <line
                x1={toX(liveOperatingPoint.temp)}
                y1={top}
                x2={toX(liveOperatingPoint.temp)}
                y2={bottom}
                stroke="rgba(255,255,255,0.16)"
                strokeDasharray="2 2"
              />
              <circle
                cx={toX(liveOperatingPoint.temp)}
                cy={toY(liveOperatingPoint.fanPct)}
                r="5.5"
                fill="var(--good)"
                fillOpacity="0.25"
              />
              <circle
                cx={toX(liveOperatingPoint.temp)}
                cy={toY(liveOperatingPoint.fanPct)}
                r="3"
                fill="var(--good)"
                stroke="#0f172a"
                strokeWidth="1"
              />
            </g>
          )}
        </svg>
      </div>
      )}
      {editing && (
        <div className="p-3.5 rounded-xl bg-slate-950/90 border border-cyan-500/30 flex flex-col gap-3 shadow-xl backdrop-blur-md">
          <div className="flex items-center justify-between border-b border-white/10 pb-2">
            <div className="text-xs font-bold text-cyan-400 flex items-center gap-1.5">
              <Cpu className="w-3.5 h-3.5 text-cyan-400" />
              {editingRuleId ? "Edit Thermal Rule" : "Configure Thermal Rule"}
            </div>
            <span className="text-[9px] font-mono text-slate-400">
              {editingRuleId ? "Editing Rule" : "New Profile"}
            </span>
          </div>

          <div className="space-y-1">
            <label className="text-[10px] font-medium text-slate-300">Profile Name</label>
            <input
              type="text"
              value={ruleName}
              onChange={(e) => setRuleName(e.target.value)}
              placeholder="Rule Name"
              className="w-full bg-slate-900/90 border border-white/10 rounded-lg px-2.5 py-1.5 text-xs text-white outline-none focus:border-cyan-400 transition-colors"
            />
          </div>

          <div className="space-y-1">
            <label className="text-[10px] font-medium text-slate-300">Target Sensor Component</label>
            <select
              value={targetSensor}
              onChange={(e) => setTargetSensor(e.target.value)}
              className="w-full bg-slate-900/90 border border-white/10 rounded-lg px-2.5 py-1.5 text-xs text-white outline-none focus:border-cyan-400 cursor-pointer font-mono transition-colors"
            >
              <option value="hottest">🔥 Hottest Component (Auto Target)</option>
              <option value="cpu">💻 CPU Aggregate</option>
              <option value="gpu">🎮 GPU Cluster</option>
              {sensors.map((s) => (
                <option key={s.key} value={s.key}>
                  {s.label} ({s.key})
                </option>
              ))}
            </select>
          </div>

          <div className="grid grid-cols-2 gap-2.5">
            <div className="space-y-1">
              <div className="flex justify-between items-center text-[10px] text-slate-300 font-medium">
                <span className="flex items-center gap-1">
                  <Thermometer className="w-3 h-3 text-cyan-400" /> Low Trigger
                </span>
                <span className="font-mono text-cyan-400 font-bold">
                  {lowTemp}°C{unit === "F" ? ` (${Math.round(displayTemperature(lowTemp, "F"))}°F)` : ""}
                </span>
              </div>
              <input
                type="number"
                min={30}
                max={70}
                value={lowTemp}
                onChange={(e) => setLowTemp(Number(e.target.value))}
                className="w-full bg-slate-900/90 border border-white/10 rounded-lg px-2.5 py-1 text-xs text-white font-mono outline-none focus:border-cyan-400 transition-colors"
              />
            </div>

            <div className="space-y-1">
              <div className="flex justify-between items-center text-[10px] text-slate-300 font-medium">
                <span className="flex items-center gap-1">
                  <Thermometer className="w-3 h-3 text-amber-400" /> High Threshold
                </span>
                <span className="font-mono text-amber-400 font-bold">
                  {highTemp}°C{unit === "F" ? ` (${Math.round(displayTemperature(highTemp, "F"))}°F)` : ""}
                </span>
              </div>
              <input
                type="number"
                min={60}
                max={100}
                value={highTemp}
                onChange={(e) => setHighTemp(Number(e.target.value))}
                className="w-full bg-slate-900/90 border border-white/10 rounded-lg px-2.5 py-1 text-xs text-white font-mono outline-none focus:border-cyan-400 transition-colors"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2.5">
            <div className="space-y-1">
              <div className="flex justify-between text-[10px] text-slate-300">
                <span>Min Speed</span>
                <span className="font-mono text-cyan-400 font-semibold">{minFan}%</span>
              </div>
              <input
                type="range"
                min={10}
                max={50}
                value={minFan}
                onChange={(e) => setMinFan(Number(e.target.value))}
                className="w-full h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-cyan-400"
              />
            </div>

            <div className="space-y-1">
              <div className="flex justify-between text-[10px] text-slate-300">
                <span>Max Speed</span>
                <span className="font-mono text-amber-400 font-semibold">{maxFan}%</span>
              </div>
              <input
                type="range"
                min={60}
                max={100}
                value={maxFan}
                onChange={(e) => setMaxFan(Number(e.target.value))}
                className="w-full h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-amber-400"
              />
            </div>
          </div>

          <div className="flex gap-2 justify-end pt-1">
            <button
              type="button"
              onClick={() => {
                setEditing(false);
                resetForm();
              }}
              className="px-3 py-1.5 rounded-lg text-xs text-slate-400 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleSaveRuleSubmit}
              className="px-3.5 py-1.5 rounded-lg bg-gradient-to-r from-cyan-500 to-blue-500 text-slate-950 font-bold text-xs hover:from-cyan-400 hover:to-blue-400 shadow-md shadow-cyan-500/20 transition-all"
            >
              {editingRuleId ? "Update Rule" : "Save Rule"}
            </button>
          </div>
        </div>
      )}
      {customRules.length > 0 && (
        <div className="flex flex-col gap-1.5 mt-1">
          <div className="flex items-center justify-between text-[10px] font-bold text-slate-400 uppercase tracking-wider">
            <span>Custom Profiles</span>
            {activePreset === "custom" && (
              <span className="text-purple-400 flex items-center gap-1 font-mono normal-case text-[9px] bg-purple-500/10 border border-purple-500/30 px-1.5 py-0.5 rounded">
                <span className="w-1.5 h-1.5 rounded-full bg-purple-400 animate-pulse"></span>
                ACTIVE POLICY
              </span>
            )}
          </div>
          {customRules.map((r) => (
            <div
              key={r.id}
              onClick={() => onSelectPreset("custom")}
              className={`p-2 rounded-xl border flex items-center justify-between text-xs cursor-pointer transition-all ${
                activePreset === "custom"
                  ? "bg-purple-950/30 border-purple-500/40 hover:border-purple-500/60"
                  : "bg-slate-900/60 border-white/5 hover:border-white/10"
              }`}
            >
              <div className="flex-1 pr-2">
                <div className="flex items-center gap-2">
                  <span className="font-semibold text-white">{r.name}</span>
                  {activePreset === "custom" && (
                    <span className="text-[8px] bg-purple-500/20 text-purple-300 font-mono px-1.5 py-0.5 rounded border border-purple-500/30">
                      ENFORCED
                    </span>
                  )}
                </div>
                <div className="text-[9px] text-slate-400 font-mono mt-0.5">
                  Target: {r.target.type === "sensor_key" ? r.target.key : r.target.type} | {r.low_celsius}°C - {r.high_celsius}°C ({r.min_fan_percent}%-{r.max_fan_percent}%)
                </div>
              </div>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  title="Edit Rule"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleStartEditRule(r);
                  }}
                  className="p-1.5 rounded-lg text-slate-400 hover:text-cyan-300 hover:bg-cyan-500/10 transition-all"
                >
                  <Pencil className="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  title="Delete Rule"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteRule(r.id);
                  }}
                  className="p-1.5 rounded-lg text-slate-500 hover:text-rose-400 hover:bg-white/5 transition-all"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
