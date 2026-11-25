pub use crate::application_preferences::contract::KeepAwakeRules;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SleepAssertion {
    pub pid: u32,
    pub process_name: String,
    pub assertion_type: String,
    pub reason: String,
}

/// One observation of the outside world for rule evaluation: only enabled
/// rules cause an OS probe, and the port count stays unenriched.
pub(crate) struct RuleConditions {
    pub(crate) on_ac: bool,
    pub(crate) dev_ports: usize,
    pub(crate) ai_active: bool,
}

impl RuleConditions {
    pub(crate) fn observe(rules: &KeepAwakeRules) -> Self {
        Self {
            on_ac: rules.while_on_ac && is_ac_power_connected(),
            dev_ports: if rules.while_ports_active {
                crate::ports::list_listening_ports_fast()
                    .map(|list| {
                        list.into_iter()
                            .filter(|p| (1024..=10000).contains(&p.port))
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            },
            ai_active: rules.while_ai_agents_active && crate::system_extras::ai_agent_active(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AwakeStatus {
    pub keep_awake_active: bool,
    #[ts(type = "number | null")]
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[ts(type = "number | null")]
    pub remaining_seconds: Option<u64>,
    pub assertions: Vec<SleepAssertion>,
    #[serde(default)]
    pub rules: KeepAwakeRules,
    #[serde(default)]
    pub auto_rule_reason: Option<String>,
}

impl AwakeStatus {
    pub fn is_indefinite(&self) -> bool {
        self.keep_awake_active
            && self.duration_seconds.is_none()
            && self.auto_rule_reason.is_none()
    }
}

const SLEEP_ASSERTION_TYPES: &[&str] = &[
    "PreventUserIdleSystemSleep",
    "PreventUserIdleDisplaySleep",
    "PreventSystemSleep",
    "NoIdleSleepAssertion",
];

pub fn read_sleep_assertions() -> Vec<SleepAssertion> {
    let Ok(output) = Command::new("/usr/bin/pmset")
        .args(["-g", "assertions"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_pmset_assertions(&stdout)
}

pub fn parse_pmset_assertions(raw: &str) -> Vec<SleepAssertion> {
    let mut in_listed_section = false;
    let mut seen: HashSet<(u32, String)> = HashSet::new();
    let mut assertions = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Listed by owning process:") {
            in_listed_section = true;
            continue;
        }
        if trimmed.starts_with("Kernel Assertions:") {
            break;
        }
        if !in_listed_section || !trimmed.starts_with("pid ") {
            continue;
        }

        let Some(after_pid) = trimmed.strip_prefix("pid ") else {
            continue;
        };
        let Some((pid_str, rest)) = after_pid.split_once('(') else {
            continue;
        };
        let Ok(pid) = pid_str.trim().parse::<u32>() else {
            continue;
        };
        let Some((process_name, after_paren)) = rest.split_once("):") else {
            continue;
        };
        let process_name = process_name.trim().to_string();

        let Some(assertion_type) = SLEEP_ASSERTION_TYPES
            .iter()
            .find(|kind| after_paren.contains(*kind))
            .map(|kind| (*kind).to_string())
        else {
            continue;
        };

        let reason = if let Some((_, after_named)) = after_paren.split_once("named: \"") {
            after_named
                .split('"')
                .next()
                .unwrap_or("Active assertion")
                .trim()
                .to_string()
        } else {
            assertion_type.clone()
        };

        if seen.insert((pid, reason.clone())) {
            assertions.push(SleepAssertion {
                pid,
                process_name,
                assertion_type,
                reason,
            });
        }
    }

    assertions
}

#[derive(Default)]
pub struct KeepAwakeManager {
    child: Option<Child>,
    duration_seconds: Option<u64>,
    expires_at: Option<Instant>,
    rules: KeepAwakeRules,
    manual_override: bool,
    auto_rule_reason: Option<String>,
}

fn ceil_duration_secs(d: Duration) -> u64 {
    d.as_secs() + u64::from(d.subsec_nanos() > 0)
}

pub fn is_ac_power_connected() -> bool {
    Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("'AC Power'"))
        .unwrap_or(false)
}

impl KeepAwakeManager {
    pub fn with_rules(rules: KeepAwakeRules) -> Self {
        let mut mgr = Self::default();
        mgr.rules = rules;
        mgr
    }

    fn refresh(&mut self) {
        if let Some(expires_at) = self.expires_at {
            if Instant::now() >= expires_at {
                self.stop();
                return;
            }
        }
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    self.child = None;
                    self.duration_seconds = None;
                    self.expires_at = None;
                    self.manual_override = false;
                    self.auto_rule_reason = None;
                }
                Ok(None) => {}
            }
        }
    }

    fn caffeinate_args(owner_pid: u32, duration_seconds: Option<u64>) -> Vec<String> {
        let mut args = vec!["-di".to_string(), "-w".to_string(), owner_pid.to_string()];
        if let Some(secs) = duration_seconds.filter(|&s| s > 0) {
            args.push("-t".to_string());
            args.push(secs.to_string());
        }
        args
    }

    fn spawn_caffeinate(&mut self, duration_seconds: Option<u64>) -> Result<(), String> {
        let mut cmd = Command::new("/usr/bin/caffeinate");
        cmd.args(Self::caffeinate_args(std::process::id(), duration_seconds));
        let (stored_duration, expires_at) = match duration_seconds {
            Some(secs) if secs > 0 => (Some(secs), Some(Instant::now() + Duration::from_secs(secs))),
            _ => (None, None),
        };

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start caffeinate: {e}"))?;
        self.child = Some(child);
        self.duration_seconds = stored_duration;
        self.expires_at = expires_at;
        Ok(())
    }

    pub fn set_keep_awake(
        &mut self,
        enabled: bool,
        duration_seconds: Option<u64>,
    ) -> Result<(), String> {
        self.stop();
        if !enabled {
            self.manual_override = false;
            return Ok(());
        }

        self.spawn_caffeinate(duration_seconds)?;
        self.manual_override = true;
        self.auto_rule_reason = None;
        Ok(())
    }

    /// Atomically applies an intent to the stored rules and returns the
    /// committed value, so probes always observe what actually won.
    pub(crate) fn change_rules(
        &mut self,
        change: impl FnOnce(&mut KeepAwakeRules),
    ) -> KeepAwakeRules {
        change(&mut self.rules);
        self.rules
    }

    pub(crate) fn desired_auto_reason(&self, conditions: &RuleConditions) -> Option<&'static str> {
        if self.rules.while_ai_agents_active && conditions.ai_active {
            Some("Auto: AI agent working")
        } else if self.rules.while_ports_active && conditions.dev_ports > 0 {
            Some("Auto: Dev ports active")
        } else if self.rules.while_on_ac && conditions.on_ac {
            Some("Auto: On AC power")
        } else {
            None
        }
    }

    pub(crate) fn evaluate_rules(&mut self, conditions: RuleConditions) -> Result<(), String> {
        self.refresh();
        if self.manual_override && self.child.is_some() {
            return Ok(());
        }

        if let Some(reason) = self.desired_auto_reason(&conditions) {
            self.auto_rule_reason = Some(reason.to_string());
            if self.child.is_none() {
                self.spawn_caffeinate(None)?;
            }
        } else if !self.manual_override && self.child.is_some() {
            self.stop();
        } else {
            self.auto_rule_reason = None;
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.duration_seconds = None;
        self.expires_at = None;
        self.manual_override = false;
        self.auto_rule_reason = None;
    }

    // Rules-only read for callers that would otherwise pay for a pmset
    // assertions query just to discard it.
    pub fn rules(&self) -> KeepAwakeRules {
        self.rules
    }

    /// Indefinite manual hold check for the tray toggle: refreshes local
    /// bookkeeping only, never a pmset query on the menu event thread.
    pub(crate) fn is_indefinite(&mut self) -> bool {
        self.refresh();
        self.child.is_some()
            && self.duration_seconds.is_none()
            && self.auto_rule_reason.is_none()
    }

    pub(crate) fn is_active_fast(&mut self) -> bool {
        self.refresh();
        self.child.is_some()
    }

    pub fn status(&mut self) -> AwakeStatus {
        self.refresh();
        let keep_awake_active = self.child.is_some();
        let remaining_seconds = if keep_awake_active {
            self.expires_at
                .map(|exp| ceil_duration_secs(exp.saturating_duration_since(Instant::now())))
        } else {
            None
        };
        AwakeStatus {
            keep_awake_active,
            duration_seconds: if keep_awake_active {
                self.duration_seconds
            } else {
                None
            },
            remaining_seconds,
            assertions: read_sleep_assertions(),
            rules: self.rules,
            auto_rule_reason: self.auto_rule_reason.clone(),
        }
    }
}

impl Drop for KeepAwakeManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Commits a rule intent to the manager before paying for OS probes, then
/// evaluates only if the committed rules still stand — a stale observation
/// can never overwrite a newer commit. Returns the authoritative status.
pub(crate) fn apply_rule_change(
    state: &Mutex<KeepAwakeManager>,
    change: impl FnOnce(&mut KeepAwakeRules),
) -> Result<AwakeStatus, String> {
    let rules = {
        let mut manager = state.lock().unwrap();
        manager.change_rules(change)
    };
    let observed = RuleConditions::observe(&rules);
    let mut manager = state.lock().unwrap();
    if manager.rules() == rules {
        manager.evaluate_rules(observed)?;
    }
    Ok(manager.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pmset_sleep_preventing_assertions() {
        let sample = r#"
2026-11-22 00:54:49 +0700 
Assertion status system-wide:
   PreventUserIdleSystemSleep     1
Listed by owning process:
   pid 563(powerd): [0x0001b304000183fe] 03:06:35 PreventUserIdleSystemSleep named: "Powerd - Prevent sleep while display is on"  
   pid 563(powerd): [0x0001d86e00098b51] 00:00:15 UserIsActive named: "com.apple.powermanagement.kernel.useractive"  
   pid 1042(sharingd): [0x0001de0700018c4b] 00:03:04 PreventUserIdleSystemSleep named: "Handoff"  
   pid 800(coreaudiod): [0x0001d3d600018a3a] 00:46:33 PreventUserIdleSystemSleep named: "audio-out"  
Kernel Assertions: 0x104=USB,MAGICWAKE
"#;
        let parsed = parse_pmset_assertions(sample);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].pid, 563);
        assert_eq!(parsed[0].process_name, "powerd");
        assert_eq!(parsed[0].assertion_type, "PreventUserIdleSystemSleep");
        assert_eq!(
            parsed[0].reason,
            "Powerd - Prevent sleep while display is on"
        );
        assert_eq!(parsed[1].process_name, "sharingd");
        assert_eq!(parsed[1].reason, "Handoff");
    }

    #[test]
    fn resolves_auto_keep_awake_rules_correctly() {
        let conditions = |on_ac: bool, dev_ports: usize, ai_active: bool| RuleConditions {
            on_ac,
            dev_ports,
            ai_active,
        };
        let mut mgr = KeepAwakeManager::default();
        assert_eq!(mgr.desired_auto_reason(&conditions(true, 2, true)), None);

        mgr.rules = KeepAwakeRules {
            while_on_ac: true,
            while_ports_active: false,
            while_ai_agents_active: false,
        };
        assert_eq!(
            mgr.desired_auto_reason(&conditions(true, 0, false)),
            Some("Auto: On AC power")
        );
        assert_eq!(mgr.desired_auto_reason(&conditions(false, 2, false)), None);

        mgr.rules.while_ports_active = true;
        assert_eq!(
            mgr.desired_auto_reason(&conditions(true, 1, false)),
            Some("Auto: Dev ports active")
        );

        mgr.rules.while_ai_agents_active = true;
        assert_eq!(
            mgr.desired_auto_reason(&conditions(false, 0, true)),
            Some("Auto: AI agent working")
        );
    }

    #[test]
    fn is_indefinite_excludes_timed_and_auto_holds() {
        let mut mgr = KeepAwakeManager::default();
        assert!(!mgr.is_indefinite());

        let child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
        mgr.child = Some(child);
        assert!(mgr.is_indefinite());

        mgr.duration_seconds = Some(3600);
        mgr.expires_at = Some(Instant::now() + Duration::from_secs(3600));
        assert!(!mgr.is_indefinite());

        mgr.duration_seconds = None;
        mgr.expires_at = None;
        mgr.auto_rule_reason = Some("Auto: On AC power".to_string());
        assert!(!mgr.is_indefinite());

        mgr.stop();
        assert!(!mgr.is_indefinite());
    }

    #[test]
    fn caffeinate_args_tie_child_to_owner_pid() {
        assert_eq!(
            KeepAwakeManager::caffeinate_args(4242, None),
            vec!["-di", "-w", "4242"]
        );
        assert_eq!(
            KeepAwakeManager::caffeinate_args(4242, Some(0)),
            vec!["-di", "-w", "4242"]
        );
        assert_eq!(
            KeepAwakeManager::caffeinate_args(4242, Some(1800)),
            vec!["-di", "-w", "4242", "-t", "1800"]
        );
    }
}
