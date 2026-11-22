use super::contract::{
    ApplicationPreferenceChange, ApplicationPreferences, SUPPORTED_TELEMETRY_INTERVALS_MS,
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
                self.store.save(&updated)?;
            }
            ApplicationPreferenceChange::SetTelemetryIntervalMs { value } => {
                updated.telemetry_interval_ms = value;
                validate(&updated)?;
                self.store.save(&updated)?;
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
                if let Some(error) = error {
                    self.current = updated;
                    return Err(error);
                }
            }
            ApplicationPreferenceChange::SetMenuBarStyle { value } => {
                updated.menu_bar_style = value;
                self.store.save(&updated)?;
            }
            ApplicationPreferenceChange::SetTrafficLightMenuBar { value } => {
                updated.traffic_light_menu_bar = value;
                self.store.save(&updated)?;
            }
            ApplicationPreferenceChange::SetPopoverLook { value } => {
                updated.popover_look = value;
                self.store.save(&updated)?;
            }
            ApplicationPreferenceChange::SetLayoutDensity { value } => {
                updated.layout_density = value;
                self.store.save(&updated)?;
            }
            ApplicationPreferenceChange::SetChartPalette { value } => {
                updated.chart_palette = value;
                self.store.save(&updated)?;
            }
        }
        self.current = updated.clone();
        Ok(updated)
    }

    pub fn accept_import(&mut self, next: ApplicationPreferences) {
        self.current = next;
    }
}

pub fn validate(preferences: &ApplicationPreferences) -> Result<(), String> {
    if !SUPPORTED_TELEMETRY_INTERVALS_MS.contains(&preferences.telemetry_interval_ms) {
        return Err("telemetry interval must be 1000, 1500, or 2500 milliseconds".into());
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
            ChartPalette, LayoutDensity, MenuBarStyle, PopoverLook,
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
            .update(ApplicationPreferenceChange::SetChartPalette {
                value: ChartPalette::OkabeIto,
            })
            .unwrap();

        let reloaded = module(shared).current();
        assert_eq!(reloaded.menu_bar_style, MenuBarStyle::StackedReadout);
        assert!(reloaded.traffic_light_menu_bar);
        assert_eq!(reloaded.popover_look, PopoverLook::Terminal);
        assert_eq!(reloaded.layout_density, LayoutDensity::Compact);
        assert_eq!(reloaded.chart_palette, ChartPalette::OkabeIto);
    }

    #[test]
    fn legacy_preferences_json_without_design_fields_deserializes_with_defaults() {
        use crate::application_preferences::contract::{
            ChartPalette, LayoutDensity, MenuBarStyle, PopoverLook,
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
        assert_eq!(parsed.chart_palette, ChartPalette::Default);
    }
}
