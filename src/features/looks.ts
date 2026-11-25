import type { PopoverLook } from "../types";

export const LOOKS = [
  { id: "liquid_glass", name: "Liquid Glass", tagline: "Deep slate + cyan", swatches: ["#0d121e", "#22d3ee"] },
  { id: "classic", name: "Classic", tagline: "Neutral macOS HUD", swatches: ["#1f2124", "#f0a36b"] },
  { id: "traffic_light", name: "Traffic Light", tagline: "Quiet until hot", swatches: ["#1c1e22", "#fbbf24"] },
  { id: "thermal_amber", name: "Thermal Amber", tagline: "Warm instrument", swatches: ["#1b1614", "#f0a36b"] },
  { id: "terminal", name: "Terminal", tagline: "Monospace shell", swatches: ["#0b100d", "#4ade80"] },
  { id: "blueprint", name: "Blueprint", tagline: "Schematic grid", swatches: ["#0c182c", "#38bdf8"] },
  { id: "pill", name: "Pill", tagline: "Soft rounded chips", swatches: ["#161b26", "#60a5fa"] },
  { id: "neon", name: "Neon", tagline: "Cyan → magenta", swatches: ["#0f101a", "#e879f9"] },
  { id: "sunset", name: "Sunset", tagline: "Peach → rose warm", swatches: ["#1e1419", "#fb7185"] },
  { id: "badge", name: "Badge", tagline: "Indigo status HUD", swatches: ["#1b1d24", "#818cf8"] },
  { id: "mint", name: "Mint", tagline: "Clean calm mint", swatches: ["#111c19", "#34d399"] },
  { id: "bold", name: "Bold", tagline: "High-contrast gold", swatches: ["#121215", "#facc15"] },
  { id: "catppuccin", name: "Catppuccin Mocha", tagline: "Pastel dark slate", swatches: ["#1e1e2e", "#89b4fa"] },
  { id: "tokyo_night", name: "Tokyo Night", tagline: "Neon night indigo", swatches: ["#1a1b26", "#7aa2f7"] },
  { id: "nord", name: "Nord Frost", tagline: "Arctic frost blue", swatches: ["#2e3440", "#88c0d0"] },
  { id: "gruvbox", name: "Gruvbox Dark", tagline: "Warm retro amber", swatches: ["#1d2021", "#fabd2f"] },
  { id: "rose_pine", name: "Rosé Pine", tagline: "Foam & dusk rose", swatches: ["#191724", "#9ccfd8"] },
  { id: "dracula", name: "Dracula", tagline: "Purple & neon cyan", swatches: ["#282a36", "#bd93f9"] },
] as const satisfies readonly {
  id: PopoverLook;
  name: string;
  tagline: string;
  swatches: readonly [string, string];
}[];

// Command-palette theme order; labels come from the same catalog.
export const PALETTE_LOOK_IDS: readonly PopoverLook[] = [
  "catppuccin",
  "tokyo_night",
  "nord",
  "gruvbox",
  "rose_pine",
  "dracula",
  "liquid_glass",
];

export const lookName = (id: PopoverLook) => LOOKS.find((look) => look.id === id)?.name ?? id;

export const previewAccentColor = (id: PopoverLook): string =>
  LOOKS.find((look) => look.id === id)?.swatches[1] ?? "#22d3ee";
