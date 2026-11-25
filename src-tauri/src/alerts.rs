use crate::application_preferences::contract::AlertPreferences;
use crate::hardware_telemetry::contract::{Availability, HardwareTelemetrySnapshot};
use crate::system_extras::SystemExtrasSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiredAlert {
    pub title: String,
    pub detail: Option<String>,
    pub tone: &'static str,
    pub tab: Option<&'static str>,
}

#[derive(Debug, Default)]
pub struct AlertEvaluator {
    last_cpu_ms: Option<u64>,
    last_swap_ms: Option<u64>,
    last_disk_ms: Option<u64>,
    last_charge_limit_ms: Option<u64>,
    last_bat_heat_ms: Option<u64>,
    last_agent_idle_ms: Option<u64>,
    active_agent_started_ms: Option<u64>,
}

fn cooldown_ready(last: Option<u64>, now_ms: u64, cooldown_ms: u64) -> bool {
    last.is_none_or(|prev| now_ms.saturating_sub(prev) >= cooldown_ms)
}

impl AlertEvaluator {
    pub fn evaluate(
        &mut self,
        now_ms: u64,
        prefs: &AlertPreferences,
        snap: &HardwareTelemetrySnapshot,
        extras: Option<&SystemExtrasSnapshot>,
    ) -> Vec<FiredAlert> {
        if !prefs.enabled {
            return Vec::new();
        }
        let mut fired = Vec::new();

        if let Availability::Available { value } = &snap.temperatures {
            if let Some(cpu_c) = value.cpu_celsius {
                if cpu_c >= f64::from(prefs.high_cpu_celsius)
                    && cooldown_ready(self.last_cpu_ms, now_ms, 60_000)
                {
                    self.last_cpu_ms = Some(now_ms);
                    fired.push(FiredAlert {
                        title: format!("High CPU Temp · {cpu_c:.0}°C"),
                        detail: Some(format!("Above {}°C threshold", prefs.high_cpu_celsius)),
                        tone: "danger",
                        tab: Some("fans"),
                    });
                }
            }
        }

        if let Availability::Available { value: bat } = &snap.battery {
            if bat.is_charging != Some(true)
                || bat
                    .charge_percent
                    .is_some_and(|c| c < i32::from(prefs.charge_limit_percent).saturating_sub(3))
            {
                self.last_charge_limit_ms = None;
            } else if prefs.notify_charge_limit
                && bat
                    .charge_percent
                    .is_some_and(|c| c >= i32::from(prefs.charge_limit_percent))
                && cooldown_ready(self.last_charge_limit_ms, now_ms, 900_000)
            {
                self.last_charge_limit_ms = Some(now_ms);
                fired.push(FiredAlert {
                    title: format!("Battery Reached {}% Target", prefs.charge_limit_percent),
                    detail: Some("Unplug AC to reduce cell wear".into()),
                    tone: "good",
                    tab: Some("devices"),
                });
            }

            if bat.is_charging == Some(true) {
                if let Some(bat_temp) = bat.temperature_celsius {
                    if bat_temp >= f64::from(prefs.battery_heat_celsius)
                        && cooldown_ready(self.last_bat_heat_ms, now_ms, 300_000)
                    {
                        self.last_bat_heat_ms = Some(now_ms);
                        fired.push(FiredAlert {
                            title: format!("Battery Heat Guard · {bat_temp:.1}°C"),
                            detail: Some("High cell temp while charging".into()),
                            tone: "warn",
                            tab: Some("devices"),
                        });
                    }
                }
            }
        }

        if let Some(ex) = extras {
            if ex.memory.swap_used_mb >= f64::from(prefs.high_swap_mb)
                && cooldown_ready(self.last_swap_ms, now_ms, 300_000)
            {
                self.last_swap_ms = Some(now_ms);
                fired.push(FiredAlert {
                    title: format!("High Swap Usage · {:.1} GB", ex.memory.swap_used_mb / 1024.0),
                    detail: Some("Memory pressure elevated".into()),
                    tone: "warn",
                    tab: Some("system"),
                });
            }

            if ex.disk.total_gb > 0.0
                && ex.disk.free_gb < f64::from(prefs.low_disk_gb)
                && cooldown_ready(self.last_disk_ms, now_ms, 900_000)
            {
                self.last_disk_ms = Some(now_ms);
                fired.push(FiredAlert {
                    title: format!("Low Disk Space · {:.1} GB free", ex.disk.free_gb),
                    detail: Some("Clean rebuildable caches".into()),
                    tone: "danger",
                    tab: Some("system"),
                });
            }

            let any_busy = ex.agents.iter().any(|a| a.cpu_percent >= 3.0);
            if any_busy {
                self.active_agent_started_ms.get_or_insert(now_ms);
            } else if let Some(started) = self.active_agent_started_ms {
                let quiet = ex.agents.is_empty() || ex.agents.iter().all(|a| a.cpu_percent < 0.5);
                if quiet {
                    self.active_agent_started_ms = None;
                    if now_ms.saturating_sub(started) >= 30_000
                        && cooldown_ready(self.last_agent_idle_ms, now_ms, 60_000)
                    {
                        self.last_agent_idle_ms = Some(now_ms);
                        fired.push(FiredAlert {
                            title: "AI Agent Idle".into(),
                            detail: Some("Background coding run settled".into()),
                            tone: "info",
                            tab: Some("agents"),
                        });
                    }
                }
            }
        }

        fired
    }
}

fn escape_applescript_str(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn send_macos_notification(title: &str, subtitle: Option<&str>) {
    let t = escape_applescript_str(title);
    let script = match subtitle {
        Some(sub) if !sub.trim().is_empty() => format!(
            "display notification \"{}\" with title \"StatPulse\" subtitle \"{}\"",
            escape_applescript_str(sub),
            t
        ),
        _ => format!("display notification \"{t}\" with title \"StatPulse\""),
    };
    std::thread::spawn(move || {
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .status();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_telemetry::contract::{
        BatteryReading, FanActuationStatus, TemperatureReadings,
    };

    fn snap_with(cpu_c: f64, charge: i32, charging: bool, bat_temp: f64) -> HardwareTelemetrySnapshot {
        HardwareTelemetrySnapshot {
            temperatures: Availability::Available {
                value: TemperatureReadings {
                    cpu_celsius: Some(cpu_c),
                    gpu_celsius: Some(50.0),
                    sensors: Vec::new(),
                },
            },
            fans: Availability::Available { value: Vec::new() },
            battery: Availability::Available {
                value: BatteryReading {
                    charge_percent: Some(charge),
                    temperature_celsius: Some(bat_temp),
                    is_charging: Some(charging),
                    cycle_count: Some(100),
                    power_watts: Some(15.0),
                },
            },
            top_processes: Availability::NotPresent,
            fan_actuation_status: FanActuationStatus::Ready,
            captured_at_unix_ms: 100_000,
        }
    }

    #[test]
    fn evaluates_thresholds_cooldowns_and_charge_target() {
        let mut eval = AlertEvaluator::default();
        let prefs = AlertPreferences {
            notify_charge_limit: true,
            ..Default::default()
        };

        let hot = snap_with(88.0, 82, true, 39.0);
        let first = eval.evaluate(100_000, &prefs, &hot, None);
        assert_eq!(first.len(), 3);
        assert!(first.iter().any(|a| a.title.contains("High CPU Temp")));
        assert!(first.iter().any(|a| a.title.contains("80% Target")));
        assert!(first.iter().any(|a| a.title.contains("Battery Heat Guard")));

        // Within cooldown: deduplicated
        let second = eval.evaluate(120_000, &prefs, &hot, None);
        assert!(second.is_empty());

        // Unplugging resets the charge-target alert for the next plug-in
        let unplugged = snap_with(55.0, 82, false, 31.0);
        assert!(eval.evaluate(125_000, &prefs, &unplugged, None).is_empty());
        let replugged = snap_with(55.0, 82, true, 31.0);
        let third = eval.evaluate(130_000, &prefs, &replugged, None);
        assert_eq!(third.len(), 1);
        assert!(third[0].title.contains("80% Target"));
    }
}
