use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SUPPORTED_TELEMETRY_INTERVALS_MS: [u32; 3] = [1_000, 1_500, 2_500];

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct KeepAwakeRules {
    #[serde(default)]
    pub while_on_ac: bool,
    #[serde(default)]
    pub while_ports_active: bool,
    #[serde(default)]
    pub while_ai_agents_active: bool,
}

impl KeepAwakeRules {
    pub fn any_enabled(&self) -> bool {
        self.while_on_ac || self.while_ports_active || self.while_ai_agents_active
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MenuBarStyle {
    #[default]
    CompactText,
    CombinedTempFan,
    CombinedTempPower,
    StackedReadout,
    TachometerGauge,
    SparklineMini,
    PerCoreBars,
    BarChart,
    Pie,
    Ring,
    Meter,
    UpDownGraph,
    BatteryIcon,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChartPalette {
    #[default]
    Default,
    OkabeIto,
    Monochrome,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PopoverLook {
    #[default]
    LiquidGlass,
    Classic,
    TrafficLight,
    ThermalAmber,
    Terminal,
    Blueprint,
    Pill,
    Neon,
    Sunset,
    Badge,
    Mint,
    Bold,
    Catppuccin,
    TokyoNight,
    Nord,
    Gruvbox,
    RosePine,
    Dracula,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LayoutDensity {
    #[default]
    Comfortable,
    Compact,
    DynamicIsland,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DynamicIslandDisplayMode {
    #[default]
    Primary,
    FollowCursor,
    AllDisplays,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CustomShortcutBinding {
    /// Carbon virtual key code (kVK_ANSI_*, kVK_F1..F20, kVK_Space, etc., 0..=127)
    pub key_code: u16,
    /// Modifier bitmask: 1 = Cmd (⌘), 2 = Shift (⇧), 4 = Option (⌥), 8 = Control (⌃)
    pub modifiers: u8,
    /// Formatted display label, e.g. "⌘⇧S" or "⌘⌥Space"
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GlobalShortcutPreset {
    Disabled,
    #[default]
    CmdShiftS,
    OptSpace,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AlertPreferences {
    pub enabled: bool,
    #[serde(default)]
    pub native_notifications: bool,
    pub high_cpu_celsius: u8,
    pub high_swap_mb: u32,
    pub low_disk_gb: u16,
    pub notify_charge_limit: bool,
    pub charge_limit_percent: u8,
    pub battery_heat_celsius: u8,
    #[serde(default)]
    pub ai_burn_rate_usd_per_hour: u16,
    #[serde(default = "default_true")]
    pub notify_agent_runaway: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AlertPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            native_notifications: false,
            high_cpu_celsius: 85,
            high_swap_mb: 4096,
            low_disk_gb: 15,
            notify_charge_limit: false,
            charge_limit_percent: 80,
            battery_heat_celsius: 38,
            ai_burn_rate_usd_per_hour: 0,
            notify_agent_runaway: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AutoThermalTriggers {
    pub performance_on_heavy_load: bool,
    pub quiet_on_battery: bool,
    #[serde(default)]
    pub performance_apps: Vec<String>,
}

pub fn default_workspace_roots() -> Vec<String> {
    vec!["Work".to_string()]
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ApplicationPreferences {
    pub temperature_unit: TemperatureUnit,
    pub telemetry_interval_ms: u32,
    pub launch_at_login: bool,
    #[serde(default)]
    pub menu_bar_style: MenuBarStyle,
    #[serde(default)]
    pub traffic_light_menu_bar: bool,
    #[serde(default)]
    pub popover_look: PopoverLook,
    #[serde(default)]
    pub layout_density: LayoutDensity,
    #[serde(default)]
    pub dynamic_island_display: DynamicIslandDisplayMode,
    #[serde(default)]
    pub chart_palette: ChartPalette,
    #[serde(default)]
    pub keep_awake_rules: KeepAwakeRules,
    #[serde(default)]
    pub global_shortcut: GlobalShortcutPreset,
    #[serde(default)]
    pub global_shortcut_custom: Option<CustomShortcutBinding>,
    #[serde(default)]
    pub alerts: AlertPreferences,
    #[serde(default)]
    pub auto_thermal: AutoThermalTriggers,
    #[serde(default)]
    pub mic_mute_shortcut: bool,
    #[serde(default)]
    pub mic_mute_shortcut_custom: Option<CustomShortcutBinding>,
    #[serde(default = "default_workspace_roots")]
    pub workspace_roots: Vec<String>,
}

impl Default for ApplicationPreferences {
    fn default() -> Self {
        Self {
            temperature_unit: TemperatureUnit::Celsius,
            telemetry_interval_ms: 1_500,
            launch_at_login: false,
            menu_bar_style: MenuBarStyle::default(),
            traffic_light_menu_bar: false,
            popover_look: PopoverLook::default(),
            layout_density: LayoutDensity::default(),
            dynamic_island_display: DynamicIslandDisplayMode::default(),
            chart_palette: ChartPalette::default(),
            keep_awake_rules: KeepAwakeRules::default(),
            global_shortcut: GlobalShortcutPreset::default(),
            global_shortcut_custom: None,
            alerts: AlertPreferences::default(),
            auto_thermal: AutoThermalTriggers::default(),
            mic_mute_shortcut: false,
            mic_mute_shortcut_custom: None,
            workspace_roots: default_workspace_roots(),
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ApplicationPreferenceChange {
    SetTemperatureUnit { value: TemperatureUnit },
    SetTelemetryIntervalMs { value: u32 },
    SetLaunchAtLogin { value: bool },
    SetMenuBarStyle { value: MenuBarStyle },
    SetTrafficLightMenuBar { value: bool },
    SetPopoverLook { value: PopoverLook },
    SetLayoutDensity { value: LayoutDensity },
    SetDynamicIslandDisplay { value: DynamicIslandDisplayMode },
    SetChartPalette { value: ChartPalette },
    SetKeepAwakeRules { value: KeepAwakeRules },
    SetGlobalShortcut { value: GlobalShortcutPreset },
    SetGlobalShortcutCustom { value: Option<CustomShortcutBinding> },
    SetAlerts { value: AlertPreferences },
    SetAutoThermal { value: AutoThermalTriggers },
    SetMicMuteShortcut { value: bool },
    SetMicMuteShortcutCustom { value: Option<CustomShortcutBinding> },
    SetWorkspaceRoots { value: Vec<String> },
}
