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
    last_burn_rate_ms: Option<u64>,
    last_runaway_agent_ms: Option<u64>,
    high_cpu_agent_started_ms: Option<(String, u64)>,
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
                    title: format!(
                        "High Swap Usage · {:.1} GB",
                        ex.memory.swap_used_mb / 1024.0
                    ),
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

            if prefs.ai_burn_rate_usd_per_hour > 0
                && ex.ai_token_usage.estimated_cost_usd_1h
                    >= f64::from(prefs.ai_burn_rate_usd_per_hour)
                && cooldown_ready(self.last_burn_rate_ms, now_ms, 1_800_000)
            {
                self.last_burn_rate_ms = Some(now_ms);
                fired.push(FiredAlert {
                    title: format!(
                        "AI Burn Rate · ${:.2}/hr",
                        ex.ai_token_usage.estimated_cost_usd_1h
                    ),
                    detail: Some(format!(
                        "Above ${}/hr budget",
                        prefs.ai_burn_rate_usd_per_hour
                    )),
                    tone: "warn",
                    tab: Some("agents"),
                });
            }

            if prefs.notify_agent_runaway {
                if let Some(hot_agent) = ex.agents.iter().find(|a| a.cpu_percent >= 85.0) {
                    let agent_key = if hot_agent.session_id.is_empty() {
                        &hot_agent.agent_name
                    } else {
                        &hot_agent.session_id
                    };
                    let started_at = match &self.high_cpu_agent_started_ms {
                        Some((key, ts)) if key == agent_key => *ts,
                        _ => {
                            self.high_cpu_agent_started_ms = Some((agent_key.clone(), now_ms));
                            now_ms
                        }
                    };
                    if now_ms.saturating_sub(started_at) >= 600_000
                        && cooldown_ready(self.last_runaway_agent_ms, now_ms, 900_000)
                    {
                        self.last_runaway_agent_ms = Some(now_ms);
                        let branch_suffix = hot_agent
                            .git_branch
                            .as_deref()
                            .map(|b| format!(" ({b})"))
                            .unwrap_or_default();
                        fired.push(FiredAlert {
                            title: format!("Runaway Agent · {}", hot_agent.agent_name),
                            detail: Some(format!(
                                "{:.0}% CPU for 10m+{branch_suffix}",
                                hot_agent.cpu_percent
                            )),
                            tone: "danger",
                            tab: Some("agents"),
                        });
                    }
                } else {
                    self.high_cpu_agent_started_ms = None;
                }
            } else {
                self.high_cpu_agent_started_ms = None;
            }
        }

        fired
    }
}

pub fn send_macos_notification(title: &str, subtitle: Option<&str>) {
    crate::macos_bridge::post_user_notification(title, subtitle.unwrap_or(""));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_telemetry::contract::{
        BatteryReading, FanActuationStatus, TemperatureReadings,
    };

    fn snap_with(
        cpu_c: f64,
        charge: i32,
        charging: bool,
        bat_temp: f64,
    ) -> HardwareTelemetrySnapshot {
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

    #[test]
    fn evaluates_ai_burn_rate_and_runaway_agent_alerts() {
        let mut eval = AlertEvaluator::default();
        let prefs = AlertPreferences {
            ai_burn_rate_usd_per_hour: 10,
            notify_agent_runaway: true,
            ..Default::default()
        };
        let cool_snap = snap_with(55.0, 50, false, 30.0);
        let mut extras = crate::system_extras::collect_pill_extras();
        extras.ai_token_usage.estimated_cost_usd_1h = 12.50;
        extras.agents = vec![crate::system_extras::AgentSessionItem {
            agent_name: "Claude Code".into(),
            primary_pid: 4242,
            process_count: 2,
            cpu_percent: 96.0,
            memory_mb: 512.0,
            uptime: "12:00".into(),
            ports: Vec::new(),
            children: Vec::new(),
            session_id: "Claude Code:4242".into(),
            ..Default::default()
        }];

        let first = eval.evaluate(1_000_000, &prefs, &cool_snap, Some(&extras));
        assert!(first.iter().any(|a| a.title.contains("AI Burn Rate")));
        assert!(!first.iter().any(|a| a.title.contains("Runaway Agent")));

        // Switching to a different worktree session of the same agent resets the 10m timer.
        extras.agents[0].session_id = "Claude Code:5252".into();
        let switched = eval.evaluate(1_000_000 + 590_000, &prefs, &cool_snap, Some(&extras));
        assert!(!switched.iter().any(|a| a.title.contains("Runaway Agent")));

        let after_10m = eval.evaluate(
            1_000_000 + 590_000 + 600_000,
            &prefs,
            &cool_snap,
            Some(&extras),
        );
        assert!(after_10m.iter().any(|a| a.title.contains("Runaway Agent")));
    }
}
