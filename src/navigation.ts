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
  type LucideIcon,
} from "lucide-react";

export const TABS = [
  { id: "home", label: "Home", title: "Home", icon: Home },
  { id: "fans", label: "Fans", title: "Fans & Curve", icon: Fan },
  { id: "system", label: "System", title: "System & Storage", icon: Cpu },
  { id: "apps", label: "Apps", title: "Apps & Thermal Load", icon: List },
  { id: "network", label: "Network", title: "Network & Speed", icon: Wifi },
  { id: "devices", label: "Devices", title: "Devices & Power", icon: Usb },
  { id: "volume", label: "Volume", title: "Volume & Audio", icon: Volume2 },
  { id: "ports", label: "Ports", title: "Open Ports", icon: ArrowLeftRight },
  { id: "agents", label: "Agents", title: "AI Agents", icon: Sparkles },
  { id: "awake", label: "Awake", title: "Keep Awake", icon: Coffee },
  { id: "design", label: "Design", title: "Design Studio", icon: Palette },
  { id: "settings", label: "Settings", title: "Settings & Backup", icon: Settings },
] as const satisfies readonly { id: string; label: string; title: string; icon: LucideIcon }[];

export type AppTab = (typeof TABS)[number]["id"];

export interface TabCounts {
  fans: number;
  apps: number;
  ports: number;
  agents: number;
}

export function getTabBadge(tabId: AppTab, counts: TabCounts): number | undefined {
  switch (tabId) {
    case "fans":
      return counts.fans > 0 ? counts.fans : undefined;
    case "apps":
      return counts.apps > 0 ? counts.apps : undefined;
    case "ports":
      return counts.ports > 0 ? counts.ports : undefined;
    case "agents":
      return counts.agents > 0 ? counts.agents : undefined;
    default:
      return undefined;
  }
}
