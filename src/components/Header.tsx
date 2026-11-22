import React from "react";
import {
  Home,
  Fan,
  Cpu,
  List,
  Wifi,
  Usb,
  Volume2,
  ArrowLeftRight,
  Sparkles,
  Coffee,
  Palette,
  Settings,
  X,
  ShieldAlert,
  Activity,
  Command,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ThermalPolicyMode } from "../types";

export type AppTab =
  | "home"
  | "fans"
  | "system"
  | "apps"
  | "network"
  | "devices"
  | "volume"
  | "ports"
  | "agents"
  | "awake"
  | "design"
  | "settings";

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
  const navItems: {
    id: AppTab;
    label: string;
    icon: React.ReactNode;
    badge?: number;
    activeDot?: boolean;
  }[] = [
    { id: "home", label: "Home", icon: <Home className="w-3.5 h-3.5" /> },
    {
      id: "fans",
      label: "Fans",
      icon: <Fan className="w-3.5 h-3.5" />,
      badge: fanCount > 0 ? fanCount : undefined,
    },
    { id: "system", label: "System", icon: <Cpu className="w-3.5 h-3.5" /> },
    {
      id: "apps",
      label: "Apps",
      icon: <List className="w-3.5 h-3.5" />,
      badge: appCount > 0 ? appCount : undefined,
    },
    { id: "network", label: "Network", icon: <Wifi className="w-3.5 h-3.5" /> },
    { id: "devices", label: "Devices", icon: <Usb className="w-3.5 h-3.5" /> },
    { id: "volume", label: "Volume", icon: <Volume2 className="w-3.5 h-3.5" /> },
    {
      id: "ports",
      label: "Ports",
      icon: <ArrowLeftRight className="w-3.5 h-3.5" />,
      badge: portCount > 0 ? portCount : undefined,
    },
    {
      id: "agents",
      label: "Agents",
      icon: <Sparkles className="w-3.5 h-3.5" />,
      badge: agentCount > 0 ? agentCount : undefined,
    },
    {
      id: "awake",
      label: "Awake",
      icon: <Coffee className="w-3.5 h-3.5" />,
      activeDot: keepAwakeActive,
    },
    { id: "design", label: "Design", icon: <Palette className="w-3.5 h-3.5" /> },
  ];

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
                className="absolute top-1 right-2 w-1.5 h-1.5 rounded-full bg-amber-400"
                title="Keep Awake Active"
              />
            )}
          </button>
        ))}
      </div>

      <button
        type="button"
        className="pp-tab mt-auto"
        aria-pressed={activeTab === "settings"}
        onClick={() => setActiveTab("settings")}
        title="Settings"
      >
        <Settings className="w-3.5 h-3.5" />
        <span>Settings</span>
      </button>
    </nav>
  );
};

interface HeaderProps {
  activeTab: AppTab;
  hasAccess: boolean;
  policyMode: ThermalPolicyMode;
  onCyclePolicy: () => void;
  onOpenCommandPalette: () => void;
  onHideWindow: () => void;
}

const TAB_TITLES: Record<AppTab, string> = {
  home: "Home",
  fans: "Fans & Curve",
  system: "System & Storage",
  apps: "Apps & Thermal Load",
  network: "Network & Speed",
  devices: "Devices & Power",
  volume: "Volume & Audio",
  ports: "Open Ports",
  agents: "AI Agents",
  awake: "Keep Awake",
  design: "Design Studio",
  settings: "Settings & Backup",
};

const POLICY_BADGES: Record<ThermalPolicyMode, string> = {
  system_auto: "★ Auto",
  quiet: "★ Quiet",
  performance: "★ Perf",
  custom: "★ Custom",
};

export const Header: React.FC<HeaderProps> = ({
  activeTab,
  hasAccess,
  policyMode,
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
