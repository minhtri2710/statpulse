use crate::thermal_policy::contract::ThermalPolicySettings;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SUPPORTED_TELEMETRY_INTERVALS_MS: [u32; 3] = [1_000, 1_500, 2_500];

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
    pub chart_palette: ChartPalette,
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
            chart_palette: ChartPalette::default(),
        }
    }
}

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
    SetChartPalette { value: ChartPalette },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ConfigurationBundle {
    pub version: u32,
    pub preferences: ApplicationPreferences,
    pub thermal_policy: ThermalPolicySettings,
}
