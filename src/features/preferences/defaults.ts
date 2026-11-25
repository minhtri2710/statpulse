import type { ApplicationPreferences } from "../../types";

export const DEFAULT_PREFERENCES: ApplicationPreferences = {
  temperature_unit: "celsius",
  telemetry_interval_ms: 1500,
  popover_look: "liquid_glass",
  layout_density: "comfortable",
  dynamic_island_display: "primary",
  menu_bar_style: "compact_text",
  chart_palette: "default",
  traffic_light_menu_bar: false,
  launch_at_login: false,
  keep_awake_rules: {
    while_on_ac: false,
    while_ports_active: false,
    while_ai_agents_active: false,
  },
  global_shortcut: "cmd_shift_s",
  alerts: {
    enabled: true,
    native_notifications: true,
    high_cpu_celsius: 85,
    high_swap_mb: 4096,
    low_disk_gb: 15,
    notify_charge_limit: false,
    charge_limit_percent: 80,
    battery_heat_celsius: 38,
  },
  auto_thermal: {
    performance_on_heavy_load: false,
    quiet_on_battery: false,
  },
};
