import { displayTemperature } from "../features/thermals/display";
import React from "react";
import { LOOKS } from "../features/looks";
import {
  Check,
  Palette,
  LayoutGrid,
  TrafficCone,
  Monitor,
  Eye,
} from "lucide-react";
import {
  ApplicationPreferenceChange,
  ApplicationPreferences,
  ChartPalette,
  HardwareTelemetrySnapshot,
  LayoutDensity,
  MenuBarStyle,
} from "../types";

interface DesignStudioModalProps {
  preferences: ApplicationPreferences;
  telemetry: HardwareTelemetrySnapshot | null;
  history: { time: number; cpu: number; gpu: number | null }[];
  onUpdatePreferences: (change: ApplicationPreferenceChange) => void;
}

const MENU_BAR_STYLES: {
  id: MenuBarStyle;
  name: string;
  description: string;
}[] = [
  { id: "compact_text", name: "Compact Temp", description: "Thermometer glyph + temp" },
  { id: "combined_temp_fan", name: "Temp + Fan RPM", description: "Fan glyph + temp & RPM" },
  { id: "combined_temp_power", name: "Temp + Power", description: "Power bolt + temp & watts" },
  { id: "stacked_readout", name: "Stacked 2-Line", description: "Dense native badge readout" },
  { id: "tachometer_gauge", name: "Tachometer Arc", description: "270° RPM / thermal dial" },
  { id: "sparkline_mini", name: "Live Sparkline", description: "Mini trend frame + temp" },
  { id: "per_core_bars", name: "Per-Core Bars", description: "Core thermal bars + temp" },
  { id: "bar_chart", name: "Bar Chart", description: "CPU / Fan / Power 3-bar view" },
  { id: "pie", name: "Pie Gauge", description: "Filled radial thermal wedge" },
  { id: "ring", name: "Ring Progress", description: "Minimal circular ring arc" },
  { id: "meter", name: "Segmented Meter", description: "6-step pill thermal meter" },
  { id: "up_down_graph", name: "Up / Down Graph", description: "Mirrored CPU & fan waveform" },
  { id: "battery_icon", name: "Battery & Temp", description: "Battery level glyph + temp" },
];

const CHART_PALETTES: {
  id: ChartPalette;
  name: string;
  description: string;
  dots: [string, string, string];
}[] = [
  {
    id: "default",
    name: "Look Default",
    description: "Follows active popover theme",
    dots: ["#22d3ee", "#9ad48a", "#f0b35a"],
  },
  {
    id: "okabe_ito",
    name: "Okabe–Ito Safe",
    description: "Colour-blind accessible palette",
    dots: ["#56b4e9", "#009e73", "#e69f00"],
  },
  {
    id: "monochrome",
    name: "Monochrome",
    description: "High-contrast slate grayscale",
    dots: ["#e2e8f0", "#94a3b8", "#64748b"],
  },
];

const POPOVER_LOOKS = LOOKS;

function formatTemp(celsius: number | null, unit: "celsius" | "fahrenheit"): string {
  const short = unit === "fahrenheit" ? "F" : "C";
  if (celsius === null) return `--°${short}`;
  return `${Math.round(displayTemperature(celsius, short))}°${short}`;
}

export const DesignStudioModal: React.FC<DesignStudioModalProps> = ({
  preferences,
  telemetry,
  history,
  onUpdatePreferences,
}) => {
  const cpuCelsius =
    telemetry?.temperatures.status === "available"
      ? telemetry.temperatures.value.cpu_celsius
      : 52;
  const fans = telemetry?.fans.status === "available" ? telemetry.fans.value : [];
  const maxFanRpm = fans.length > 0 ? Math.max(...fans.map((f) => f.speed_rpm)) : 2400;
  const powerWatts =
    telemetry?.battery.status === "available" ? telemetry.battery.value.power_watts : 14.2;
  const batteryPct =
    telemetry?.battery.status === "available" ? telemetry.battery.value.charge_percent ?? 82 : 82;

  const tempStr = formatTemp(cpuCelsius ?? 52, preferences.temperature_unit);
  const fanStr = maxFanRpm === 0 ? "Silent" : `${maxFanRpm} RPM`;
  const wattStr = powerWatts !== null ? `${powerWatts.toFixed(1)}W` : "14.2W";

  const trafficLightActive =
    preferences.traffic_light_menu_bar || preferences.popover_look === "traffic_light";

  const previewAccentColor = (() => {
    const t = cpuCelsius ?? 52;
    if (trafficLightActive) {
      if (t >= 80) return "#f43f5e";
      if (t >= 60) return "#fbbf24";
      return "#e2e8f0";
    }
    switch (preferences.popover_look) {
      case "classic":
        return "#e2e8f0";
      case "terminal":
        return "#4ade80";
      case "thermal_amber":
        return "#f0a36b";
      case "blueprint":
        return "#38bdf8";
      case "pill":
        return "#60a5fa";
      case "neon":
        return t >= 65 ? "#e879f9" : "#22d3ee";
      case "sunset":
        return "#fb7185";
      case "badge":
        return "#818cf8";
      case "mint":
        return "#34d399";
      case "bold":
        return "#facc15";
      case "catppuccin":
        return "#89b4fa";
      case "tokyo_night":
        return "#7aa2f7";
      case "nord":
        return "#88c0d0";
      case "gruvbox":
        return "#fabd2f";
      case "rose_pine":
        return "#9ccfd8";
      case "dracula":
        return "#bd93f9";
      default:
        return t >= 85 ? "#f43f5e" : t >= 70 ? "#fbbf24" : "#22d3ee";
    }
  })();

  const sparkSamples = history.length >= 2 ? history.slice(-12).map((p) => p.cpu) : [46, 48, 49, 51, 50, 52, 54, 52];
  const minS = Math.min(...sparkSamples) - 2;
  const maxS = Math.max(...sparkSamples) + 2;
  const sparkSpan = maxS - minS || 1;
  const sparkPoints = sparkSamples
    .map((v, i) => {
      const x = (i / Math.max(sparkSamples.length - 1, 1)) * 28;
      const y = 12 - ((v - minS) / sparkSpan) * 10;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  const renderPreviewGlyph = (style: MenuBarStyle, accent: string) => {
    switch (style) {
      case "compact_text":
        return (
          <div className="flex items-center gap-1 font-mono text-[11px]" style={{ color: accent }}>
            <svg width="12" height="14" viewBox="0 0 12 14" fill="none">
              <rect x="4" y="1" width="4" height="8" rx="2" stroke={accent} strokeWidth="1.3" />
              <circle cx="6" cy="10.5" r="2.5" fill={accent} />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "combined_temp_fan":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6.5" stroke={accent} strokeOpacity="0.4" strokeWidth="1.2" />
              <circle cx="8" cy="4.2" r="2" fill={accent} />
              <circle cx="11.8" cy="8" r="2" fill={accent} />
              <circle cx="8" cy="11.8" r="2" fill={accent} />
              <circle cx="4.2" cy="8" r="2" fill={accent} />
            </svg>
            <span className="font-semibold text-white">{tempStr} · {fanStr}</span>
          </div>
        );
      case "combined_temp_power":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <span
              className="px-1 py-0.5 rounded text-[9px] font-bold border"
              style={{ color: accent, borderColor: `${accent}66`, backgroundColor: `${accent}22` }}
            >
              ⚡
            </span>
            <span className="font-semibold text-white">{tempStr} · {wattStr}</span>
          </div>
        );
      case "stacked_readout":
        return (
          <div
            className="px-1.5 py-0.5 rounded border flex flex-col leading-none items-center justify-center font-mono"
            style={{ borderColor: `${accent}88`, backgroundColor: `${accent}1e` }}
          >
            <span className="text-[8px] font-bold tracking-tight" style={{ color: accent }}>
              CPU {tempStr}
            </span>
            <span className="text-[7px] text-slate-200 mt-0.5">
              {maxFanRpm === 0 ? "SILENT" : `${maxFanRpm} RPM`}
            </span>
          </div>
        );
      case "tachometer_gauge":
        return (
          <div className="flex items-center gap-1 font-mono text-[11px]">
            <svg width="15" height="14" viewBox="0 0 18 16" fill="none">
              <path
                d="M 3 13 A 7 7 0 1 1 15 13"
                stroke={accent}
                strokeOpacity="0.35"
                strokeWidth="2"
                strokeLinecap="round"
              />
              <path
                d="M 3 13 A 7 7 0 0 1 12 4"
                stroke={accent}
                strokeWidth="2.2"
                strokeLinecap="round"
              />
              <line x1="9" y1="9" x2="12" y2="5" stroke={accent} strokeWidth="1.6" strokeLinecap="round" />
              <circle cx="9" cy="9" r="1.5" fill={accent} />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "sparkline_mini":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <svg
              width="30"
              height="14"
              viewBox="0 0 28 14"
              className="rounded border"
              style={{ borderColor: `${accent}66`, backgroundColor: `${accent}1a` }}
            >
              <polyline fill="none" stroke={accent} strokeWidth="1.6" points={sparkPoints} />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "per_core_bars":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <div
              className="h-3.5 px-1 rounded border flex items-end gap-0.5 py-0.5"
              style={{ borderColor: `${accent}66`, backgroundColor: `${accent}1a` }}
            >
              {[50, 72, 85, 62, 48, 70].map((pct, i) => (
                <span
                  key={i}
                  className="w-0.5 rounded-sm"
                  style={{ height: `${pct}%`, backgroundColor: accent }}
                />
              ))}
            </div>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "bar_chart":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <div
              className="h-3.5 px-1 rounded border flex items-end gap-1 py-0.5"
              style={{ borderColor: `${accent}66`, backgroundColor: `${accent}1a` }}
            >
              <span className="w-1 rounded-sm" style={{ height: "75%", backgroundColor: accent }} />
              <span className="w-1 rounded-sm bg-sky-400" style={{ height: "45%" }} />
              <span className="w-1 rounded-sm bg-amber-400" style={{ height: "60%" }} />
            </div>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "pie":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6" stroke={accent} strokeOpacity="0.4" strokeWidth="1.2" fill={`${accent}22`} />
              <path d="M8 8 L8 2 A6 6 0 1 1 2.8 11 Z" fill={accent} />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "ring":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="5.5" stroke={accent} strokeOpacity="0.28" strokeWidth="2.4" />
              <path d="M8 2.5 A5.5 5.5 0 1 1 3 10.5" stroke={accent} strokeWidth="2.4" strokeLinecap="round" />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "meter":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <div
              className="h-3.5 px-1 rounded border flex items-center gap-0.5"
              style={{ borderColor: `${accent}66`, backgroundColor: `${accent}1a` }}
            >
              {[1, 2, 3, 4, 5].map((n) => (
                <span
                  key={n}
                  className="w-1 h-2 rounded-xs"
                  style={{ backgroundColor: n <= 4 ? accent : `${accent}33` }}
                />
              ))}
            </div>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "up_down_graph":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <svg
              width="24"
              height="14"
              viewBox="0 0 24 14"
              className="rounded border"
              style={{ borderColor: `${accent}66`, backgroundColor: `${accent}1a` }}
            >
              <line x1="2" y1="7" x2="22" y2="7" stroke={accent} strokeOpacity="0.35" strokeWidth="0.7" />
              <polyline fill="none" stroke={accent} strokeWidth="1.2" points="2,6 7,4 13,5 18,3 22,4" />
              <polyline fill="none" stroke="#38bdf8" strokeWidth="1.2" points="2,8 7,10 13,9 18,11 22,10" />
            </svg>
            <span className="font-semibold text-white">{tempStr}</span>
          </div>
        );
      case "battery_icon":
        return (
          <div className="flex items-center gap-1.5 font-mono text-[11px]">
            <div className="flex items-center">
              <div
                className="w-5 h-3 rounded-xs border p-0.5 flex items-center"
                style={{ borderColor: accent }}
              >
                <div
                  className="h-full rounded-xs"
                  style={{ width: `${Math.max(batteryPct, 15)}%`, backgroundColor: accent }}
                />
              </div>
              <div className="w-0.5 h-1.5 rounded-r-xs ml-px" style={{ backgroundColor: accent }} />
            </div>
            <span className="font-semibold text-white">
              {batteryPct}% · {tempStr}
            </span>
          </div>
        );
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between text-[10px] text-slate-400">
          <span className="flex items-center gap-1.5 font-bold uppercase tracking-wider">
            <Monitor className="w-3.5 h-3.5 text-cyan-400" />
            Live Menu Bar Preview
          </span>
          <span className="font-mono text-[9px] text-slate-400">
            {trafficLightActive ? "Traffic Light Active" : "Look Accent"}
          </span>
        </div>

        <div className="w-full h-8 rounded-lg bg-slate-950/90 border border-white/10 px-2.5 flex items-center justify-between shadow-inner">
          <span className="text-[10px] font-semibold text-slate-400 tracking-tight">
             Finder · File · Edit
          </span>
          <div className="flex items-center gap-2.5">
            <div className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10">
              {renderPreviewGlyph(preferences.menu_bar_style, previewAccentColor)}
            </div>
            <span className="text-[10px] text-slate-400 font-mono">Sat 23:15</span>
          </div>
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-xs font-bold text-white">Menu Bar Style</span>
          <span className="text-[10px] text-slate-400">13 render modes</span>
        </div>

        <div className="grid grid-cols-1 gap-1.5">
          {MENU_BAR_STYLES.map((style) => {
            const active = preferences.menu_bar_style === style.id;
            return (
              <button
                key={style.id}
                type="button"
                onClick={() =>
                  onUpdatePreferences({ type: "set_menu_bar_style", value: style.id })
                }
                className={`px-2.5 py-2 rounded-lg border text-left flex items-center justify-between gap-2 transition-all ${
                  active
                    ? "bg-cyan-500/15 border-cyan-400/50 text-white"
                    : "bg-slate-900/55 border-white/5 text-slate-300 hover:border-white/15"
                }`}
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 text-[11px] font-semibold">
                    <span>{style.name}</span>
                    {active && <Check className="w-3 h-3 text-cyan-400 shrink-0" />}
                  </div>
                  <div className="text-[9px] text-slate-400 truncate">{style.description}</div>
                </div>
                <div className="shrink-0 px-1.5 py-0.5 rounded bg-slate-950/75 border border-white/10">
                  {renderPreviewGlyph(style.id, previewAccentColor)}
                </div>
              </button>
            );
          })}
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 min-w-0">
          <div className="p-1.5 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400 shrink-0">
            <TrafficCone className="w-4 h-4" />
          </div>
          <div className="min-w-0">
            <div className="text-xs font-semibold text-white">Quiet Until It Matters</div>
            <div className="text-[10px] text-slate-400">
              Monochrome &lt;60°C · Amber 60–79°C · Red ≥80°C
            </div>
          </div>
        </div>

        <button
          type="button"
          onClick={() =>
            onUpdatePreferences({
              type: "set_traffic_light_menu_bar",
              value: !preferences.traffic_light_menu_bar,
            })
          }
          className={`px-2.5 py-1 rounded-lg text-[10px] font-bold border transition-all shrink-0 ${
            preferences.traffic_light_menu_bar
              ? "bg-amber-500/20 border-amber-400/50 text-amber-300"
              : "bg-slate-900/60 border-white/10 text-slate-400 hover:text-white"
          }`}
        >
          {preferences.traffic_light_menu_bar ? "On" : "Off"}
        </button>
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1.5 text-xs font-bold text-white">
            <Palette className="w-3.5 h-3.5 text-cyan-400" />
            Popover Looks
          </span>
          <span className="text-[10px] text-slate-400">18 themes</span>
        </div>

        <div className="grid grid-cols-2 gap-1.5">
          {POPOVER_LOOKS.map((look) => {
            const active = preferences.popover_look === look.id;
            return (
              <button
                key={look.id}
                type="button"
                onClick={() =>
                  onUpdatePreferences({ type: "set_popover_look", value: look.id })
                }
                className={`p-2 rounded-lg border text-left flex items-center justify-between gap-2 transition-all ${
                  active
                    ? "bg-cyan-500/15 border-cyan-400/50 text-white"
                    : "bg-slate-900/55 border-white/5 text-slate-300 hover:border-white/15"
                }`}
              >
                <div className="min-w-0">
                  <div className="text-[11px] font-semibold truncate">{look.name}</div>
                  <div className="text-[9px] text-slate-400 truncate">{look.tagline}</div>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <span
                    className="w-3 h-3 rounded-full border border-white/20"
                    style={{ backgroundColor: look.swatches[0] }}
                  />
                  <span
                    className="w-3 h-3 rounded-full border border-white/20"
                    style={{ backgroundColor: look.swatches[1] }}
                  />
                </div>
              </button>
            );
          })}
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="flex items-center gap-1.5 text-xs font-bold text-white">
            <Eye className="w-3.5 h-3.5 text-sky-400" />
            Chart Colour Palette
          </span>
          <span className="text-[10px] text-slate-400">Colour-blind safe option</span>
        </div>

        <div className="grid grid-cols-3 gap-1.5">
          {CHART_PALETTES.map((pal) => {
            const active = preferences.chart_palette === pal.id;
            return (
              <button
                key={pal.id}
                type="button"
                onClick={() =>
                  onUpdatePreferences({ type: "set_chart_palette", value: pal.id })
                }
                className={`p-2 rounded-lg border text-left flex flex-col gap-1 transition-all ${
                  active
                    ? "bg-cyan-500/15 border-cyan-400/50 text-white"
                    : "bg-slate-900/55 border-white/5 text-slate-300 hover:border-white/15"
                }`}
              >
                <div className="flex items-center gap-1">
                  {pal.dots.map((d, idx) => (
                    <span
                      key={idx}
                      className="w-2.5 h-2.5 rounded-full border border-white/15"
                      style={{ backgroundColor: d }}
                    />
                  ))}
                </div>
                <div className="text-[10px] font-semibold truncate">{pal.name}</div>
                <div className="text-[8.5px] text-slate-400 leading-tight">{pal.description}</div>
              </button>
            );
          })}
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <LayoutGrid className="w-4 h-4 text-cyan-400 shrink-0" />
          <div className="min-w-0">
            <div className="text-xs font-semibold text-white">Layout Density</div>
            <div className="text-[10px] text-slate-400 truncate">
              Dynamic Island floats a live capsule below the menu bar
            </div>
          </div>
        </div>

        <div className="flex items-center p-0.5 bg-slate-900/60 rounded-lg border border-white/5 text-[10px] shrink-0">
          {(
            [
              { id: "comfortable", label: "Comfortable" },
              { id: "compact", label: "Compact" },
              { id: "dynamic_island", label: "Dynamic Island" },
            ] as { id: LayoutDensity; label: string }[]
          ).map((opt) => (
            <button
              key={opt.id}
              type="button"
              onClick={() =>
                onUpdatePreferences({ type: "set_layout_density", value: opt.id })
              }
              className={`px-2 py-1 rounded-md font-medium whitespace-nowrap transition-all ${
                preferences.layout_density === opt.id
                  ? "bg-cyan-500/20 text-cyan-300 border border-cyan-500/30"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
