use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ThermalPolicyMode {
    #[default]
    SystemAuto,
    Quiet,
    Performance,
    Custom,
}

impl ThermalPolicyMode {
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::SystemAuto => "Auto",
            Self::Quiet => "Quiet",
            Self::Performance => "Performance",
            Self::Custom => "Custom",
        }
    }

    pub fn parse_preset_arg(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" | "system_auto" => Ok(Self::SystemAuto),
            "quiet" => Ok(Self::Quiet),
            "performance" | "perf" => Ok(Self::Performance),
            other => Err(format!(
                "Unsupported thermal mode '{other}' (use auto, quiet, or performance)"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ThermalTarget {
    Hottest,
    Cpu,
    Gpu,
    SensorKey { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ThermalRule {
    pub id: String,
    pub name: String,
    pub target: ThermalTarget,
    pub low_celsius: f64,
    pub high_celsius: f64,
    pub min_fan_percent: u8,
    pub max_fan_percent: u8,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export)]
pub struct ThermalPolicySettings {
    pub mode: ThermalPolicyMode,
    pub rules: Vec<ThermalRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FanTarget {
    pub fan_id: usize,
    pub rpm: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FanPlan {
    SystemAuto,
    Targets { targets: Vec<FanTarget> },
}

#[derive(Deserialize)]
pub(crate) struct Presets {
    pub quiet: ThermalRule,
    pub performance: ThermalRule,
}

// One authoritative copy of the built-in rules, shared with the frontend.
pub(crate) fn presets() -> &'static Presets {
    static PRESETS: std::sync::OnceLock<Presets> = std::sync::OnceLock::new();
    PRESETS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../shared/thermal-presets.json"))
            .expect("built-in thermal presets must match the ThermalRule contract")
    })
}

/// Borrows the active rule list; built-in modes never allocate per tick.
pub(crate) fn rules_for(settings: &ThermalPolicySettings) -> &[ThermalRule] {
    match settings.mode {
        ThermalPolicyMode::SystemAuto => &[],
        ThermalPolicyMode::Quiet => std::slice::from_ref(&presets().quiet),
        ThermalPolicyMode::Performance => std::slice::from_ref(&presets().performance),
        ThermalPolicyMode::Custom => &settings.rules,
    }
}
