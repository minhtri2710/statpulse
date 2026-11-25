import React from "react";
import { X, ShieldAlert, Activity, Command, Coffee, Timer } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThermalPolicyMode } from "../../types";
import { AppTab, getTabBadge, TABS } from "../../navigation";
import { POLICY_MODES } from "../thermals/policyModes";
import { formatDuration } from "../../services/format";

interface NavigationRailProps {
  activeTab: AppTab;
  setActiveTab: (tab: AppTab) => void;
  fanCount: number;
  appCount: number;
  portCount: number;
  agentCount: number;
  keepAwakeActive: boolean;
}

export const NavigationRail: React.FC<NavigationRailProps> = ({
  activeTab,
  setActiveTab,
  fanCount,
  appCount,
  portCount,
  agentCount,
  keepAwakeActive,
}) => {
  const counts = {
    fans: fanCount,
    apps: appCount,
    ports: portCount,
    agents: agentCount,
  };
  const navItems = TABS.filter((tab) => tab.id !== "settings").map((tab) => ({
    id: tab.id,
    label: tab.label,
    icon: <tab.icon className="w-3.5 h-3.5" />,
    badge: getTabBadge(tab.id, counts),
    activeDot: tab.id === "awake" ? keepAwakeActive : undefined,
  }));
  const settingsTab = TABS.find((tab) => tab.id === "settings")!;
  const SettingsIcon = settingsTab.icon;

  return (
    <nav className="pp-rail" aria-label="Popover views">
      <div className="flex items-center justify-center py-0.5 mb-0.5">
        <div
          className="w-6 h-6 rounded-md bg-gradient-to-tr from-cyan-500 via-sky-500 to-indigo-500 flex items-center justify-center shadow-md shadow-cyan-500/25 border border-white/15"
          title="StatPulse"
        >
          <Activity className="w-3.5 h-3.5 text-white" />
        </div>
      </div>

      <div className="flex flex-col gap-0.5 flex-1">
        {navItems.map((item, idx) => (
          <button
            key={item.id}
            type="button"
            className="pp-tab"
            aria-pressed={activeTab === item.id}
            onClick={() => setActiveTab(item.id)}
            title={idx < 9 ? `${item.label} (⌘${idx + 1})` : item.label}
          >
            {item.icon}
            <span>{item.label}</span>
            {item.badge !== undefined && (
              <i className="pp-tab__badge">{item.badge}</i>
            )}
            {item.activeDot && (
              <span
                className="absolute top-1 right-2 w-1.5 h-1.5 rounded-full bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.8)] animate-pulse"
                title="Keep Awake Active"
              />
            )}
          </button>
        ))}
      </div>

      <button
        type="button"
        className="pp-tab mt-auto"
        aria-pressed={activeTab === settingsTab.id}
        onClick={() => setActiveTab(settingsTab.id)}
        title={settingsTab.label}
      >
        <SettingsIcon className="w-3.5 h-3.5" />
        <span>{settingsTab.label}</span>
      </button>
    </nav>
  );
};

interface HeaderProps {
  activeTab: AppTab;
  hasAccess: boolean;
  policyMode: ThermalPolicyMode;
  keepAwakeActive: boolean;
  keepAwakeRemainingSeconds: number | null;
  focusRemainingSecs: number | null;
  onOpenAwake: () => void;
  onCyclePolicy: () => void;
  onOpenCommandPalette: () => void;
  onHideWindow: () => void;
}

const TAB_TITLES: Record<AppTab, string> = Object.fromEntries(
  TABS.map((tab) => [tab.id, tab.title]),
) as Record<AppTab, string>;

const POLICY_BADGES: Record<ThermalPolicyMode, string> = Object.fromEntries(
  POLICY_MODES.map((mode) => [mode.id, `★ ${mode.short}`]),
) as Record<ThermalPolicyMode, string>;

export const Header: React.FC<HeaderProps> = ({
  activeTab,
  hasAccess,
  policyMode,
  keepAwakeActive,
  keepAwakeRemainingSeconds,
  focusRemainingSecs,
  onOpenAwake,
  onCyclePolicy,
  onOpenCommandPalette,
  onHideWindow,
}) => {
  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button === 0 && !(e.target as HTMLElement).closest("button")) {
      getCurrentWindow().startDragging().catch(() => {});
    }
  };

  return (
    <div
      data-tauri-drag-region
      onMouseDown={handleMouseDown}
      className="pp-head select-none cursor-grab active:cursor-grabbing"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
        <span data-tauri-drag-region className="pp-title truncate">
          {TAB_TITLES[activeTab]}
        </span>
        {!hasAccess && (
          <span
            title="SMC Access Limited"
            className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-300 text-[9px] border border-rose-500/30"
          >
            <ShieldAlert className="w-2.5 h-2.5" />
            Limited
          </span>
        )}
      </div>

      <div className="flex items-center gap-1.5 shrink-0">
        {focusRemainingSecs !== null && (
          <button
            type="button"
            onClick={onOpenAwake}
            className="pp-pin !border-cyan-400/40 !bg-cyan-400/15 !text-cyan-300 hover:!border-cyan-300/60 transition-colors cursor-pointer inline-flex items-center gap-1"
            title="Focus Timer Active — click to manage"
          >
            <Timer className="w-2.5 h-2.5" />
            <span>{formatDuration(focusRemainingSecs)}</span>
          </button>
        )}
        {keepAwakeActive && (
          <button
            type="button"
            onClick={onOpenAwake}
            className="pp-pin !border-amber-400/40 !bg-amber-400/15 !text-amber-300 hover:!border-amber-300/60 transition-colors cursor-pointer inline-flex items-center gap-1"
            title="Keep Awake Active — click to manage"
          >
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
            <Coffee className="w-2.5 h-2.5" />
            <span>
              {keepAwakeRemainingSeconds !== null
                ? formatDuration(keepAwakeRemainingSeconds)
                : "Awake"}
            </span>
          </button>
        )}

        <button
          type="button"
          onClick={onOpenCommandPalette}
          className="pp-pin hover:border-white/30 transition-colors cursor-pointer inline-flex items-center gap-1"
          title="Quick Command Palette (⌘K)"
        >
          <Command className="w-2.5 h-2.5" />K
        </button>

        <button
          type="button"
          onClick={onCyclePolicy}
          className="pp-pin hover:border-white/30 transition-colors cursor-pointer"
          title="Click to cycle Thermal Policy"
        >
          {POLICY_BADGES[policyMode]}
        </button>

        <button
          type="button"
          onClick={onHideWindow}
          className="p-1 rounded text-[var(--sec)] hover:text-white hover:bg-white/10 transition-colors"
          title="Hide Window (Esc)"
          aria-label="Hide Window"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
};
