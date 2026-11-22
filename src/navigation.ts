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
