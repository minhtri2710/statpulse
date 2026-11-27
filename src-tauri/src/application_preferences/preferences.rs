use super::contract::{
    ApplicationPreferenceChange, ApplicationPreferences, CustomShortcutBinding,
    GlobalShortcutPreset, SUPPORTED_TELEMETRY_INTERVALS_MS,
};

pub trait PreferencesStore {
    fn load(&self) -> Result<Option<ApplicationPreferences>, String>;
    // Run `operation` while the store is known writable; file-backed stores
    // hold the profile lock and refuse while an import is unrecovered.
    fn with_writable<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        operation()
    }
    fn save(&mut self, preferences: &ApplicationPreferences) -> Result<(), String>;
}

pub trait AutostartAdapter {
    fn is_enabled(&self) -> Result<bool, String>;
    fn set_enabled(&mut self, enabled: bool) -> Result<(), String>;
}

pub struct ApplicationPreferencesModule<S, A> {
    store: S,
    autostart: A,
    current: ApplicationPreferences,
}

impl<S: PreferencesStore, A: AutostartAdapter> ApplicationPreferencesModule<S, A> {
    pub fn load(store: S, autostart: A) -> Result<Self, String> {
        let current = store.load()?.unwrap_or_default();
        validate(&current)?;
        Ok(Self {
            store,
            autostart,
            current,
        })
    }

    pub fn current(&self) -> ApplicationPreferences {
        self.current.clone()
    }

    // Re-read the OS login-item state and publish it; a denied or malformed
    // query fails rather than pretending the stored value is confirmed.
    pub fn confirmed_current(&mut self) -> Result<ApplicationPreferences, String> {
        self.current.launch_at_login = self.autostart.is_enabled()?;
        Ok(self.current.clone())
    }

    pub fn update(
        &mut self,
        change: ApplicationPreferenceChange,
    ) -> Result<ApplicationPreferences, String> {
        let mut updated = self.current.clone();
        match change {
            ApplicationPreferenceChange::SetTemperatureUnit { value } => {
                updated.temperature_unit = value;
            }
            ApplicationPreferenceChange::SetTelemetryIntervalMs { value } => {
                updated.telemetry_interval_ms = value;
            }
            ApplicationPreferenceChange::SetLaunchAtLogin { value } => {
                let outcome =
                    self.store
                        .with_writable(|| match self.autostart.set_enabled(value) {
                            Ok(()) => self.autostart.is_enabled().map(|actual| (actual, None)),
                            Err(error) => {
                                let actual = self.autostart.is_enabled().map_err(|read| {
                                    format!(
                                        "{error}; actual launch-at-login state is unknown: {read}"
                                    )
                                })?;
                                Ok((
                                    actual,
                                    Some(format!("{error}; launch at login remains {actual}")),
                                ))
                            }
                        });
                let (actual, error) = outcome?;
                updated.launch_at_login = actual;
                self.current = updated.clone();
                if let Some(error) = error {
                    return Err(error);
                }
                return Ok(updated);
            }
            ApplicationPreferenceChange::SetMenuBarStyle { value } => {
                updated.menu_bar_style = value;
            }
            ApplicationPreferenceChange::SetTrafficLightMenuBar { value } => {
                updated.traffic_light_menu_bar = value;
            }
            ApplicationPreferenceChange::SetPopoverLook { value } => {
                updated.popover_look = value;
            }
            ApplicationPreferenceChange::SetLayoutDensity { value } => {
                updated.layout_density = value;
            }
            ApplicationPreferenceChange::SetDynamicIslandDisplay { value } => {
                updated.dynamic_island_display = value;
            }
            ApplicationPreferenceChange::SetChartPalette { value } => {
                updated.chart_palette = value;
            }
            ApplicationPreferenceChange::SetKeepAwakeRules { value } => {
                updated.keep_awake_rules = value;
            }
            ApplicationPreferenceChange::SetGlobalShortcut { value } => {
                updated.global_shortcut = value;
            }
            ApplicationPreferenceChange::SetGlobalShortcutCustom { value } => {
                if value.is_some() {
                    updated.global_shortcut = GlobalShortcutPreset::Custom;
                } else if updated.global_shortcut == GlobalShortcutPreset::Custom {
                    updated.global_shortcut = GlobalShortcutPreset::CmdShiftS;
                }
                updated.global_shortcut_custom = value;
            }
            ApplicationPreferenceChange::SetAlerts { value } => {
                updated.alerts = value;
            }
            ApplicationPreferenceChange::SetAutoThermal { value } => {
                updated.auto_thermal = value;
            }
            ApplicationPreferenceChange::SetMicMuteShortcut { value } => {
                updated.mic_mute_shortcut = value;
            }
            ApplicationPreferenceChange::SetMicMuteShortcutCustom { value } => {
                if value.is_some() {
                    updated.mic_mute_shortcut = true;
                }
                updated.mic_mute_shortcut_custom = value;
            }
            ApplicationPreferenceChange::SetWorkspaceRoots { value } => {
                updated.workspace_roots = value;
            }
        }
        validate(&updated)?;
        self.store.save(&updated)?;
        self.current = updated.clone();
        Ok(updated)
    }

    pub fn accept_import(&mut self, next: ApplicationPreferences) {
        self.current = next;
    }
}

fn is_function_key_code(key_code: u16) -> bool {
    matches!(
        key_code,
        0x7A | 0x78
            | 0x63
            | 0x76
            | 0x60
            | 0x61
            | 0x62
            | 0x64
            | 0x65
            | 0x6D
            | 0x67
            | 0x6F
            | 0x69
            | 0x6B
            | 0x71
            | 0x6A
            | 0x40
            | 0x4F
            | 0x50
            | 0x5A
    )
}

fn validate_shortcut(binding: &CustomShortcutBinding, field: &str) -> Result<(), String> {
    if binding.key_code > 0x7F {
        return Err(format!("{field} key code is out of range"));
    }
    if binding.modifiers == 0 || (binding.modifiers & !0x0F) != 0 {
        return Err(format!("{field} must include a modifier key"));
    }
    if !is_function_key_code(binding.key_code) && (binding.modifiers & (1 | 4 | 8)) == 0 {
        return Err(format!(
            "{field} must include ⌘, ⌥, or ⌃ for non-function keys"
        ));
    }
    let trimmed = binding.label.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 24 {
        return Err(format!("{field} label must be 1..=24 characters"));
    }
    Ok(())
}

pub fn validate(preferences: &ApplicationPreferences) -> Result<(), String> {
    if !SUPPORTED_TELEMETRY_INTERVALS_MS.contains(&preferences.telemetry_interval_ms) {
        return Err("telemetry interval must be 1000, 1500, or 2500 milliseconds".into());
    }
    if let Some(binding) = &preferences.global_shortcut_custom {
        validate_shortcut(binding, "global shortcut")?;
    }
    if preferences.global_shortcut == GlobalShortcutPreset::Custom
        && preferences.global_shortcut_custom.is_none()
    {
        return Err("custom global shortcut requires a key binding".into());
    }
    if let Some(binding) = &preferences.mic_mute_shortcut_custom {
        validate_shortcut(binding, "mic mute shortcut")?;
    }
    if let (Some(global), Some(mic)) = (
        preferences.effective_global_binding(),
        preferences.effective_mic_binding(),
    ) {
        if global == mic {
            return Err(
                "global hotkey and mic mute hotkey cannot use the same key combination".into(),
            );
        }
    }
    if !(60..=105).contains(&preferences.alerts.high_cpu_celsius) {
        return Err("high CPU alert threshold must be between 60°C and 105°C".into());
    }
    if !(50..=100).contains(&preferences.alerts.charge_limit_percent) {
        return Err("battery charge alert target must be between 50% and 100%".into());
    }
    if !(32..=50).contains(&preferences.alerts.battery_heat_celsius) {
        return Err("battery heat alert threshold must be between 32°C and 50°C".into());
    }
    if !(2..=500).contains(&preferences.alerts.low_disk_gb) {
        return Err("low disk alert threshold must be between 2 GB and 500 GB".into());
    }
    if preferences.alerts.ai_burn_rate_usd_per_hour > 500 {
        return Err("AI burn rate alert must be 500 USD/hr or less".into());
    }
    if preferences.workspace_roots.len() > 8 {
        return Err("at most 8 workspace scan folders are supported".into());
    }
    for root in &preferences.workspace_roots {
        if !crate::application_preferences::contract::is_valid_workspace_root_name(root) {
            return Err(format!(
                "workspace root '{root}' must be a single home-relative folder name"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application_preferences::contract::TemperatureUnit;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct MemoryStore {
        value: Rc<RefCell<Option<ApplicationPreferences>>>,
        fail_save: bool,
    }

    impl PreferencesStore for MemoryStore {
        fn load(&self) -> Result<Option<ApplicationPreferences>, String> {
            Ok(self.value.borrow().clone())
        }

        fn save(&mut self, preferences: &ApplicationPreferences) -> Result<(), String> {
            if self.fail_save {
                return Err("save failed".into());
            }
            *self.value.borrow_mut() = Some(preferences.clone());
            Ok(())
        }
    }

    struct MemoryAutostart {
        enabled: bool,
        fail_set: bool,
    }

    impl AutostartAdapter for MemoryAutostart {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(self.enabled)
        }

        fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
            if self.fail_set {
                return Err("autostart failed".into());
            }
            self.enabled = enabled;
            Ok(())
        }
    }

    fn module(store: MemoryStore) -> ApplicationPreferencesModule<MemoryStore, MemoryAutostart> {
        ApplicationPreferencesModule::load(
            store,
            MemoryAutostart {
                enabled: false,
                fail_set: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn first_launch_uses_defaults_until_autostart_is_confirmed() {
        let mut loaded = ApplicationPreferencesModule::load(
            MemoryStore::default(),
            MemoryAutostart {
                enabled: true,
                fail_set: false,
            },
        )
        .unwrap();
        assert_eq!(loaded.current().telemetry_interval_ms, 1_500);
        // The stored snapshot is not a confirmed OS state; confirming re-reads
        // the adapter and publishes the actual value.
        assert!(!loaded.current().launch_at_login);
        assert!(loaded.confirmed_current().unwrap().launch_at_login);
        assert!(loaded.current().launch_at_login);
    }

    #[test]
    fn persisted_preferences_survive_reload() {
        let store = MemoryStore::default();
        let shared = store.clone();
        let mut preferences = module(store);
        preferences
            .update(ApplicationPreferenceChange::SetTemperatureUnit {
                value: TemperatureUnit::Fahrenheit,
            })
            .unwrap();
        assert_eq!(
            module(shared).current().temperature_unit,
            TemperatureUnit::Fahrenheit
        );
    }

    #[test]
    fn invalid_cadence_is_rejected_without_mutation() {
        let mut preferences = module(MemoryStore::default());
        assert!(preferences
            .update(ApplicationPreferenceChange::SetTelemetryIntervalMs { value: 999 })
            .is_err());
        assert_eq!(preferences.current().telemetry_interval_ms, 1_500);
    }

    #[test]
    fn persistence_failure_leaves_current_state_unchanged() {
        let mut preferences = module(MemoryStore {
            fail_save: true,
            ..Default::default()
        });
        assert!(preferences
            .update(ApplicationPreferenceChange::SetTemperatureUnit {
                value: TemperatureUnit::Fahrenheit,
            })
            .is_err());
        assert_eq!(
            preferences.current().temperature_unit,
            TemperatureUnit::Celsius
        );
    }

    #[test]
    fn autostart_failure_reports_actual_state() {
        let mut preferences = ApplicationPreferencesModule::load(
            MemoryStore::default(),
            MemoryAutostart {
                enabled: false,
                fail_set: true,
            },
        )
        .unwrap();
        let error = preferences
            .update(ApplicationPreferenceChange::SetLaunchAtLogin { value: true })
            .unwrap_err();
        assert!(error.contains("remains false"));
        assert!(!preferences.current().launch_at_login);
    }

    #[test]
    fn design_and_menu_bar_preferences_persist_and_reload() {
        use crate::application_preferences::contract::{
            ChartPalette, DynamicIslandDisplayMode, LayoutDensity, MenuBarStyle, PopoverLook,
        };

        let store = MemoryStore::default();
        let shared = store.clone();
        let mut preferences = module(store);

        preferences
            .update(ApplicationPreferenceChange::SetMenuBarStyle {
                value: MenuBarStyle::StackedReadout,
            })
            .unwrap();
        preferences
            .update(ApplicationPreferenceChange::SetTrafficLightMenuBar { value: true })
            .unwrap();
        preferences
            .update(ApplicationPreferenceChange::SetPopoverLook {
                value: PopoverLook::Terminal,
            })
            .unwrap();
        preferences
            .update(ApplicationPreferenceChange::SetLayoutDensity {
                value: LayoutDensity::Compact,
            })
            .unwrap();
        preferences
            .update(ApplicationPreferenceChange::SetDynamicIslandDisplay {
                value: DynamicIslandDisplayMode::AllDisplays,
            })
            .unwrap();
        preferences
            .update(ApplicationPreferenceChange::SetChartPalette {
                value: ChartPalette::OkabeIto,
            })
            .unwrap();

        let reloaded = module(shared).current();
        assert_eq!(reloaded.menu_bar_style, MenuBarStyle::StackedReadout);
        assert!(reloaded.traffic_light_menu_bar);
        assert_eq!(reloaded.popover_look, PopoverLook::Terminal);
        assert_eq!(reloaded.layout_density, LayoutDensity::Compact);
        assert_eq!(
            reloaded.dynamic_island_display,
            DynamicIslandDisplayMode::AllDisplays
        );
        assert_eq!(reloaded.chart_palette, ChartPalette::OkabeIto);
    }

    #[test]
    fn legacy_preferences_json_without_design_fields_deserializes_with_defaults() {
        use crate::application_preferences::contract::{
            ChartPalette, DynamicIslandDisplayMode, LayoutDensity, MenuBarStyle, PopoverLook,
        };

        let legacy_json = r#"{
            "temperature_unit": "fahrenheit",
            "telemetry_interval_ms": 1000,
            "launch_at_login": true
        }"#;
        let parsed: ApplicationPreferences = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.temperature_unit, TemperatureUnit::Fahrenheit);
        assert_eq!(parsed.telemetry_interval_ms, 1000);
        assert!(parsed.launch_at_login);
        assert_eq!(parsed.menu_bar_style, MenuBarStyle::CompactText);
        assert!(!parsed.traffic_light_menu_bar);
        assert_eq!(parsed.popover_look, PopoverLook::LiquidGlass);
        assert_eq!(parsed.layout_density, LayoutDensity::Comfortable);
        assert_eq!(
            parsed.dynamic_island_display,
            DynamicIslandDisplayMode::Primary
        );
        assert_eq!(parsed.chart_palette, ChartPalette::Default);
    }

    #[test]
    fn custom_shortcuts_validate_persist_and_reject_conflicts() {
        let store = MemoryStore::default();
        let mut preferences = module(store);

        let custom_global = CustomShortcutBinding {
            key_code: 0x11, // T
            modifiers: 1 | 4,
            label: "⌘⌥T".into(),
        };
        let updated = preferences
            .update(ApplicationPreferenceChange::SetGlobalShortcutCustom {
                value: Some(custom_global.clone()),
            })
            .unwrap();
        assert_eq!(updated.global_shortcut, GlobalShortcutPreset::Custom);
        assert_eq!(updated.global_shortcut_custom, Some(custom_global.clone()));

        // Reject Shift-only non-function key
        assert!(preferences
            .update(ApplicationPreferenceChange::SetMicMuteShortcutCustom {
                value: Some(CustomShortcutBinding {
                    key_code: 0x00,
                    modifiers: 2,
                    label: "⇧A".into(),
                }),
            })
            .is_err());

        // Reject out-of-range key code
        assert!(preferences
            .update(ApplicationPreferenceChange::SetMicMuteShortcutCustom {
                value: Some(CustomShortcutBinding {
                    key_code: 0x99,
                    modifiers: 1,
                    label: "⌘?".into(),
                }),
            })
            .is_err());

        // Reject duplicate binding with active global hotkey
        assert!(preferences
            .update(ApplicationPreferenceChange::SetMicMuteShortcutCustom {
                value: Some(custom_global),
            })
            .is_err());

        // Distinct custom mic binding succeeds and enables mic_mute_shortcut
        let custom_mic = CustomShortcutBinding {
            key_code: 0x7A, // F1
            modifiers: 2,   // Shift+F1 (function key allows Shift-only)
            label: "⇧F1".into(),
        };
        let updated = preferences
            .update(ApplicationPreferenceChange::SetMicMuteShortcutCustom {
                value: Some(custom_mic.clone()),
            })
            .unwrap();
        assert!(updated.mic_mute_shortcut);
        assert_eq!(updated.mic_mute_shortcut_custom, Some(custom_mic));
    }
}
