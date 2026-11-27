use super::contract::{FanPlan, FanTarget, ThermalPolicyMode, ThermalPolicySettings, ThermalRule};
use super::evaluator::{self, ThermalPolicyEvaluator};
use super::settings;
use crate::hardware_telemetry::contract::{
    Availability, FanActuationStatus, FanMode, HardwareTelemetrySnapshot,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) trait SettingsStore {
    fn load(&self) -> Result<Option<ThermalPolicySettings>, String>;
    fn save(&mut self, settings: &ThermalPolicySettings) -> Result<(), String>;
}

pub(crate) trait FanActuation {
    fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String>;
    fn system_auto(&mut self, fan_id: usize) -> Result<(), String>;
    fn restore_all(&mut self) -> Result<(), String>;
    fn heartbeat(&mut self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectFanActuationRequest {
    Target { fan_id: usize, rpm: i32 },
    SystemAuto { fan_id: usize },
}

#[derive(Debug, Clone)]
pub enum ThermalPolicyChange {
    SelectMode(ThermalPolicyMode),
    UpsertRule(ThermalRule),
    DeleteRule(String),
}

pub(crate) struct ThermalPolicyState<S, F> {
    store: S,
    fan_actuation: F,
    current: ThermalPolicySettings,
    evaluator: ThermalPolicyEvaluator,
    applied_targets: BTreeMap<usize, i32>,
    direct_fans: BTreeSet<usize>,
    manual_plan_active: bool,
    system_auto_confirmed: bool,
    // Transient Dust Blast override: owns its targets and token but never
    // touches persisted settings, so the selected preset resumes on the next
    // fresh sample and survives a crash untouched.
    blast: Option<(u64, Vec<FanTarget>)>,
    last_blast_id: u64,
    // Ephemeral runtime trigger override (e.g., auto Performance on heavy load
    // or auto Quiet on battery). Never persisted and cleared on user changes.
    runtime_mode_override: Option<ThermalPolicyMode>,
}

impl<S: SettingsStore, F: FanActuation> ThermalPolicyState<S, F> {
    pub(crate) fn load(store: S, fan_actuation: F) -> Self {
        let current = store
            .load()
            .ok()
            .flatten()
            .filter(|settings| settings::validate(settings).is_ok())
            .unwrap_or_default();
        Self {
            store,
            fan_actuation,
            current,
            evaluator: ThermalPolicyEvaluator::default(),
            applied_targets: BTreeMap::new(),
            direct_fans: BTreeSet::new(),
            manual_plan_active: false,
            system_auto_confirmed: false,
            blast: None,
            last_blast_id: 0,
            runtime_mode_override: None,
        }
    }

    pub(crate) fn set_runtime_mode_override(&mut self, next: Option<ThermalPolicyMode>) -> bool {
        let desired = if self.current.mode == ThermalPolicyMode::Custom {
            None
        } else {
            next
        };
        if self.runtime_mode_override == desired {
            return false;
        }
        self.runtime_mode_override = desired;
        self.evaluator = ThermalPolicyEvaluator::default();
        true
    }

    pub(crate) fn apply_auto_thermal_triggers(
        &mut self,
        triggers: &crate::application_preferences::contract::AutoThermalTriggers,
        snapshot: &HardwareTelemetrySnapshot,
    ) {
        let cpu_c = match &snapshot.temperatures {
            Availability::Available { value } => value.cpu_celsius,
            _ => None,
        };
        let on_battery = match &snapshot.battery {
            Availability::Available { value } => value.is_charging == Some(false),
            _ => false,
        };
        let app_boost = !triggers.performance_apps.is_empty()
            && match &snapshot.top_processes {
                Availability::Available { value } => value.iter().any(|proc| {
                    !proc.is_paused
                        && proc.cpu_percent >= 5.0
                        && triggers
                            .performance_apps
                            .iter()
                            .any(|target| proc.name.eq_ignore_ascii_case(target.trim()))
                }),
                _ => false,
            };
        let desired = if app_boost
            || (triggers.performance_on_heavy_load
                && (cpu_c.is_some_and(|c| c >= 74.0)
                    || (self.runtime_mode_override == Some(ThermalPolicyMode::Performance)
                        && cpu_c.is_some_and(|c| c > 62.0))))
        {
            Some(ThermalPolicyMode::Performance)
        } else if triggers.quiet_on_battery && on_battery && cpu_c.is_none_or(|c| c < 70.0) {
            Some(ThermalPolicyMode::Quiet)
        } else {
            None
        };
        self.set_runtime_mode_override(desired);
    }

    pub(crate) fn current(&self) -> ThermalPolicySettings {
        self.current.clone()
    }

    // Manual Fan targets may only be established from fresh, usable telemetry;
    // a write issued against stale or unavailable data would own hardware on a
    // guess. The sleep guard lives here so tests exercise the time argument
    // instead of a process-global.
    pub(crate) fn direct_actuation(
        &mut self,
        request: DirectFanActuationRequest,
        snapshot: Option<&HardwareTelemetrySnapshot>,
    ) -> Result<(), String> {
        if matches!(request, DirectFanActuationRequest::Target { .. })
            && crate::power_management::is_sleeping()
        {
            return Err("manual Fan control is unavailable while the system is asleep".into());
        }
        self.direct_actuation_at(request, snapshot, crate::clock::now_unix_ms())
    }

    fn direct_actuation_at(
        &mut self,
        request: DirectFanActuationRequest,
        snapshot: Option<&HardwareTelemetrySnapshot>,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        // Blast interaction precedes the mode gate: an explicit SystemAuto
        // is a safety override even while an automatic preset is selected,
        // and an explicit Target is refused outright during a blast.
        if self.blast.is_some() {
            match request {
                DirectFanActuationRequest::Target { .. } => {
                    return Err("stop Dust Blast before setting manual Fan targets".into());
                }
                DirectFanActuationRequest::SystemAuto { .. } => {
                    self.blast = None;
                    return self.force_restore_system_auto();
                }
            }
        }
        if self.current.mode != ThermalPolicyMode::SystemAuto {
            return Err("direct Fan actuation is disabled while Thermal policy is active".into());
        }
        match request {
            DirectFanActuationRequest::Target { fan_id, rpm } => {
                match snapshot {
                    Some(snapshot) => self.apply_direct_target(fan_id, rpm, snapshot, now_unix_ms),
                    None => {
                        // A refusal while a direct fan is still owned is a
                        // fail-safe case: restore first, then report.
                        if !self.direct_fans.is_empty() {
                            let _ = self.force_restore_system_auto();
                        }
                        Err("manual Fan control requires fresh hardware telemetry".into())
                    }
                }
            }
            DirectFanActuationRequest::SystemAuto { fan_id } => {
                match self.fan_actuation.system_auto(fan_id) {
                    Ok(()) => {
                        self.direct_fans.remove(&fan_id);
                        Ok(())
                    }
                    Err(error) => {
                        self.recover_from_actuation_failure();
                        Err(error)
                    }
                }
            }
        }
    }

    // Shared single-target write: revalidates the snapshot and reported range
    // at `now` for every call, restores owned fans before reporting an invalid
    // request, and recovers on actuation failure.
    fn apply_direct_target(
        &mut self,
        fan_id: usize,
        rpm: i32,
        snapshot: &HardwareTelemetrySnapshot,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        let usable = direct_snapshot_usable(&BTreeSet::from([fan_id]), snapshot, now_unix_ms)
            && fan_target_within_reported_range(snapshot, fan_id, rpm);
        if !usable {
            if !self.direct_fans.is_empty() {
                let _ = self.force_restore_system_auto();
            }
            return Err("manual Fan control requires fresh hardware telemetry".into());
        }
        match self.fan_actuation.set_target(fan_id, rpm) {
            Ok(()) => {
                self.direct_fans.insert(fan_id);
                self.system_auto_confirmed = false;
                Ok(())
            }
            Err(error) => {
                self.recover_from_actuation_failure();
                Err(error)
            }
        }
    }

    // Dust Blast: a transient, unpersisted override that holds every reported
    // fan at its maximum for the countdown's lifetime.
    pub(crate) fn start_dust_blast(
        &mut self,
        snapshot: &HardwareTelemetrySnapshot,
    ) -> Result<u64, String> {
        if crate::power_management::is_sleeping() {
            return Err("Dust Blast is unavailable while the system is asleep".into());
        }
        self.start_dust_blast_at(snapshot, crate::clock::now_unix_ms)
    }

    fn start_dust_blast_at(
        &mut self,
        snapshot: &HardwareTelemetrySnapshot,
        now: impl Fn() -> u64,
    ) -> Result<u64, String> {
        if self.blast.is_some() {
            return Err("Dust Blast is already running".into());
        }
        let targets: Vec<FanTarget> = match &snapshot.fans {
            Availability::Available { value } => value
                .iter()
                .map(|fan| FanTarget {
                    fan_id: fan.id,
                    rpm: fan.max_speed_rpm.unwrap_or(0),
                })
                .collect(),
            _ => Vec::new(),
        };
        if targets.is_empty() || targets.iter().any(|target| target.rpm <= 0) {
            return Err("No controllable fans detected for Dust Blast.".into());
        }
        // Preflight every target against the same snapshot before releasing or
        // touching owned fans; the write loop re-checks the clock per write.
        let ids: BTreeSet<usize> = targets.iter().map(|target| target.fan_id).collect();
        if !direct_snapshot_usable(&ids, snapshot, now())
            || targets.iter().any(|target| {
                !fan_target_within_reported_range(snapshot, target.fan_id, target.rpm)
            })
        {
            return Err("Dust Blast requires fresh hardware telemetry".into());
        }
        if self.manual_plan_active || !self.direct_fans.is_empty() {
            self.force_restore_system_auto()?;
        }
        for target in &targets {
            self.apply_direct_target(target.fan_id, target.rpm, snapshot, now())?;
        }
        self.last_blast_id = self.last_blast_id.wrapping_add(1).max(1);
        let id = self.last_blast_id;
        self.blast = Some((id, targets));
        Ok(id)
    }

    pub(crate) fn dust_blast_is_active(&self, id: u64) -> bool {
        self.blast.as_ref().is_some_and(|(active, _)| *active == id)
    }

    pub(crate) fn has_active_blast(&self) -> bool {
        self.blast.is_some()
    }

    pub(crate) fn cancel_dust_blast(&mut self) -> Result<(), String> {
        if self.blast.is_none() {
            return Ok(());
        }
        self.force_restore_system_auto()
    }

    // The countdown's own finish: a stale token must not restore over a newer
    // session.
    pub(crate) fn finish_dust_blast(&mut self, id: u64) -> Result<(), String> {
        if !self.dust_blast_is_active(id) {
            return Ok(());
        }
        self.blast = None;
        self.force_restore_system_auto()
    }

    pub(crate) fn process_snapshot(
        &mut self,
        snapshot: &HardwareTelemetrySnapshot,
    ) -> Result<FanPlan, String> {
        // A decision that acquires the lock after sleep began must never
        // reapply targets; restore anything owned and refuse the plan.
        if crate::power_management::is_sleeping() {
            self.restore_system_auto()?;
            return Ok(FanPlan::SystemAuto);
        }
        self.process_snapshot_with_clock(snapshot, crate::clock::now_unix_ms)
    }

    #[cfg(test)]
    fn process_snapshot_at(
        &mut self,
        snapshot: &HardwareTelemetrySnapshot,
        timestamp: u64,
    ) -> Result<FanPlan, String> {
        self.process_snapshot_with_clock(snapshot, || timestamp)
    }

    // The clock is a function, not a fixed timestamp: a preliminary restore or
    // a slow SMC write can outlive the snapshot's freshness window mid-plan,
    // and nothing stale may be written or heartbeated afterwards.
    fn process_snapshot_with_clock(
        &mut self,
        snapshot: &HardwareTelemetrySnapshot,
        now: impl Fn() -> u64,
    ) -> Result<FanPlan, String> {
        if let Some((_, blast_targets)) = self.blast.clone() {
            let direct_ids: BTreeSet<usize> =
                blast_targets.iter().map(|target| target.fan_id).collect();
            if direct_snapshot_usable(&direct_ids, snapshot, now()) {
                if let Err(error) = self.fan_actuation.heartbeat() {
                    self.recover_from_actuation_failure();
                    return Err(error);
                }
                return Ok(FanPlan::Targets {
                    targets: blast_targets,
                });
            }
            // Stale or unusable telemetry clears the override via the
            // confirmed restore path; nothing may reissue a blast write.
            self.blast = None;
            self.restore_system_auto()?;
            return Ok(FanPlan::SystemAuto);
        }
        let effective_mode =
            if self.current.mode == ThermalPolicyMode::Custom || !self.direct_fans.is_empty() {
                self.current.mode.clone()
            } else {
                self.runtime_mode_override
                    .clone()
                    .unwrap_or_else(|| self.current.mode.clone())
            };
        let effective_settings = ThermalPolicySettings {
            mode: effective_mode.clone(),
            rules: self.current.rules.clone(),
        };

        if effective_mode != ThermalPolicyMode::SystemAuto && !self.direct_fans.is_empty() {
            // An automatic policy takes over from direct manual ownership:
            // release those fans before any of its targets or heartbeats apply.
            self.force_restore_system_auto()?;
        }
        // Time may have advanced during a preliminary restore; stale data can
        // never evaluate into automatic targets. (SystemAuto mode needs no
        // target and already restores below without redundant writes.)
        if effective_mode != ThermalPolicyMode::SystemAuto
            && !evaluator::snapshot_is_fresh(snapshot, now())
        {
            self.evaluator = ThermalPolicyEvaluator::default();
            self.force_restore_system_auto()?;
            return Ok(FanPlan::SystemAuto);
        }
        let plan = self
            .evaluator
            .evaluate(&effective_settings, snapshot, now());
        if effective_mode == ThermalPolicyMode::SystemAuto {
            if !self.direct_fans.is_empty()
                && direct_snapshot_usable(&self.direct_fans, snapshot, now())
            {
                if let Err(error) = self.fan_actuation.heartbeat() {
                    self.recover_from_actuation_failure();
                    return Err(error);
                }
            } else {
                self.restore_system_auto()?;
            }
            return Ok(plan);
        }
        if self.apply_plan(&plan, snapshot, &now)? {
            Ok(plan)
        } else {
            // Staleness was detected mid-plan: the confirmed restore already
            // ran inside apply_plan.
            Ok(FanPlan::SystemAuto)
        }
    }

    // Returns false when staleness was detected mid-plan: the confirmed
    // restore already ran and the caller reports SystemAuto.
    fn apply_plan(
        &mut self,
        plan: &FanPlan,
        snapshot: &HardwareTelemetrySnapshot,
        now: &impl Fn() -> u64,
    ) -> Result<bool, String> {
        match plan {
            FanPlan::SystemAuto => {
                self.restore_system_auto()?;
                Ok(true)
            }
            FanPlan::Targets { targets } => {
                for target in targets {
                    // The daemon may have expired our lease and restored
                    // system auto without telling us; a fan reading back in
                    // auto mode must be rewritten even when the cached RPM
                    // still matches.
                    let changed = self.applied_targets.get(&target.fan_id) != Some(&target.rpm)
                        || fan_reported_system_auto(snapshot, target.fan_id);
                    if !changed {
                        continue;
                    }
                    if !evaluator::snapshot_is_fresh(snapshot, now()) {
                        self.evaluator = ThermalPolicyEvaluator::default();
                        self.force_restore_system_auto()?;
                        return Ok(false);
                    }
                    if let Err(error) = self.fan_actuation.set_target(target.fan_id, target.rpm) {
                        self.recover_from_actuation_failure();
                        return Err(error);
                    }
                }

                self.applied_targets = targets
                    .iter()
                    .map(|target| (target.fan_id, target.rpm))
                    .collect();
                self.manual_plan_active = true;
                self.system_auto_confirmed = false;
                if !evaluator::snapshot_is_fresh(snapshot, now()) {
                    self.evaluator = ThermalPolicyEvaluator::default();
                    self.force_restore_system_auto()?;
                    return Ok(false);
                }
                if let Err(error) = self.fan_actuation.heartbeat() {
                    self.recover_from_actuation_failure();
                    return Err(error);
                }
                Ok(true)
            }
        }
    }

    fn restore_system_auto(&mut self) -> Result<(), String> {
        if self.manual_plan_active || !self.system_auto_confirmed {
            return self.force_restore_system_auto();
        }
        Ok(())
    }

    pub(crate) fn force_restore_system_auto(&mut self) -> Result<(), String> {
        // A failed restore may still have returned some fans to system auto, so
        // cached targets are invalidated either way; confirmation is recorded
        // only on a fully successful restore.
        let result = self.fan_actuation.restore_all();
        self.applied_targets.clear();
        self.direct_fans.clear();
        self.blast = None;
        self.system_auto_confirmed = result.is_ok();
        if result.is_ok() {
            self.manual_plan_active = false;
        }
        result
    }

    fn recover_from_actuation_failure(&mut self) {
        let _ = self.fan_actuation.restore_all();
        self.applied_targets.clear();
        self.direct_fans.clear();
        self.blast = None;
        self.manual_plan_active = false;
        self.system_auto_confirmed = false;
    }

    pub(crate) fn update(
        &mut self,
        change: ThermalPolicyChange,
    ) -> Result<ThermalPolicySettings, String> {
        let restore_system_auto = matches!(
            &change,
            ThermalPolicyChange::SelectMode(ThermalPolicyMode::SystemAuto)
        );
        let mut updated = self.current.clone();
        match change {
            ThermalPolicyChange::SelectMode(mode) => updated.mode = mode,
            ThermalPolicyChange::UpsertRule(rule) => {
                if let Some(existing) = updated
                    .rules
                    .iter_mut()
                    .find(|existing| existing.id == rule.id)
                {
                    *existing = rule;
                } else {
                    updated.rules.push(rule);
                }
                updated.mode = ThermalPolicyMode::Custom;
            }
            ThermalPolicyChange::DeleteRule(rule_id) => {
                updated.rules.retain(|rule| rule.id != rule_id);
            }
        }

        settings::validate(&updated)?;
        self.store.save(&updated)?;
        self.current = updated.clone();
        self.runtime_mode_override = None;
        self.evaluator = ThermalPolicyEvaluator::default();

        // A committed selection supersedes any transient override too; a
        // failed restore must surface rather than be hidden.
        if restore_system_auto || self.blast.is_some() {
            self.force_restore_system_auto()?;
        }

        Ok(updated)
    }

    // Publish settings committed by the profile import. Actuation bookkeeping
    // is untouched: clearing a confirmed restore would make the next tick issue
    // a redundant restore that undoes freshly set direct manual targets.
    pub(crate) fn accept_import(&mut self, next: ThermalPolicySettings) {
        self.current = next;
        self.evaluator = ThermalPolicyEvaluator::default();
    }
}

// Direct manual fans may only keep their leases while fresh usable telemetry
// confirms temperatures and every directly controlled fan. Anything else is a
// fail-safe case, not a heartbeat.
fn direct_snapshot_usable(
    direct_fans: &BTreeSet<usize>,
    snapshot: &HardwareTelemetrySnapshot,
    now_unix_ms: u64,
) -> bool {
    if snapshot.fan_actuation_status != FanActuationStatus::Ready
        || !evaluator::snapshot_is_fresh(snapshot, now_unix_ms)
    {
        return false;
    }
    let temperatures_valid = match &snapshot.temperatures {
        Availability::Available { value } => [value.cpu_celsius, value.gpu_celsius]
            .into_iter()
            .flatten()
            .chain(value.sensors.iter().map(|sensor| sensor.celsius))
            .any(|celsius| celsius.is_finite() && celsius > 0.0),
        _ => false,
    };
    if !temperatures_valid {
        return false;
    }
    match &snapshot.fans {
        Availability::Available { value } => direct_fans
            .iter()
            .all(|fan_id| value.iter().any(|fan| fan.id == *fan_id)),
        _ => false,
    }
}

// A manual target stays inside the fan's own reported envelope when the
// snapshot carried one; unknown bounds cannot veto.
fn fan_target_within_reported_range(
    snapshot: &HardwareTelemetrySnapshot,
    fan_id: usize,
    rpm: i32,
) -> bool {
    match &snapshot.fans {
        Availability::Available { value } => {
            value
                .iter()
                .find(|fan| fan.id == fan_id)
                .is_some_and(|fan| {
                    fan.min_speed_rpm.is_none_or(|min| rpm >= min)
                        && fan.max_speed_rpm.is_none_or(|max| rpm <= max)
                })
        }
        _ => false,
    }
}

fn fan_reported_system_auto(snapshot: &HardwareTelemetrySnapshot, fan_id: usize) -> bool {
    matches!(
        &snapshot.fans,
        Availability::Available { value }
            if value
                .iter()
                .any(|fan| fan.id == fan_id && fan.mode == Some(FanMode::SystemAuto))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_telemetry::contract::{
        Availability, FanMode, FanReading, TemperatureReadings,
    };
    use crate::thermal_policy::contract::ThermalTarget;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct MemoryStore {
        value: Rc<RefCell<Option<ThermalPolicySettings>>>,
        events: Rc<RefCell<Vec<&'static str>>>,
        fail_save: bool,
    }

    impl SettingsStore for MemoryStore {
        fn load(&self) -> Result<Option<ThermalPolicySettings>, String> {
            Ok(self.value.borrow().clone())
        }

        fn save(&mut self, settings: &ThermalPolicySettings) -> Result<(), String> {
            if self.fail_save {
                return Err("save failed".into());
            }
            self.events.borrow_mut().push("persist");
            *self.value.borrow_mut() = Some(settings.clone());
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingFanActuation {
        restore_count: Rc<RefCell<usize>>,
        direct_requests: Rc<RefCell<Vec<DirectFanActuationRequest>>>,
        events: Rc<RefCell<Vec<&'static str>>>,
        fail_direct: bool,
        fail_target_once: Rc<RefCell<bool>>,
        fail_heartbeat_once: Rc<RefCell<bool>>,
        fail_restore: bool,
    }

    impl FanActuation for RecordingFanActuation {
        fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String> {
            self.direct_requests
                .borrow_mut()
                .push(DirectFanActuationRequest::Target { fan_id, rpm });
            if self.fail_direct {
                return Err("direct actuation failed".into());
            }
            if *self.fail_target_once.borrow() {
                *self.fail_target_once.borrow_mut() = false;
                return Err("target failed".into());
            }
            Ok(())
        }

        fn system_auto(&mut self, fan_id: usize) -> Result<(), String> {
            self.direct_requests
                .borrow_mut()
                .push(DirectFanActuationRequest::SystemAuto { fan_id });
            if self.fail_direct {
                return Err("direct actuation failed".into());
            }
            Ok(())
        }

        fn restore_all(&mut self) -> Result<(), String> {
            self.events.borrow_mut().push("restore_all");
            *self.restore_count.borrow_mut() += 1;
            if self.fail_restore {
                return Err("restore failed".into());
            }
            Ok(())
        }

        fn heartbeat(&mut self) -> Result<(), String> {
            self.events.borrow_mut().push("heartbeat");
            if *self.fail_heartbeat_once.borrow() {
                *self.fail_heartbeat_once.borrow_mut() = false;
                return Err("heartbeat failed".into());
            }
            Ok(())
        }
    }

    fn snapshot(cpu_celsius: Option<f64>, captured_at_unix_ms: u64) -> HardwareTelemetrySnapshot {
        snapshot_with_mode(cpu_celsius, captured_at_unix_ms, FanMode::SystemAuto)
    }

    fn snapshot_with_mode(
        cpu_celsius: Option<f64>,
        captured_at_unix_ms: u64,
        fan_mode: FanMode,
    ) -> HardwareTelemetrySnapshot {
        HardwareTelemetrySnapshot {
            temperatures: Availability::Available {
                value: TemperatureReadings {
                    cpu_celsius,
                    gpu_celsius: None,
                    sensors: vec![],
                },
            },
            fans: Availability::Available {
                value: vec![FanReading {
                    id: 0,
                    label: "Fan 1".into(),
                    speed_rpm: 2000,
                    min_speed_rpm: Some(1000),
                    max_speed_rpm: Some(5000),
                    target_speed_rpm: None,
                    mode: Some(fan_mode),
                }],
            },
            battery: Availability::NotPresent,
            top_processes: Availability::NotPresent,
            fan_actuation_status: FanActuationStatus::Ready,
            captured_at_unix_ms,
        }
    }

    fn snapshot_two_fans(
        cpu_celsius: Option<f64>,
        captured_at_unix_ms: u64,
    ) -> HardwareTelemetrySnapshot {
        let mut snap = snapshot(cpu_celsius, captured_at_unix_ms);
        if let Availability::Available { value } = &mut snap.fans {
            value.push(FanReading {
                id: 1,
                label: "Fan 2".into(),
                speed_rpm: 2000,
                min_speed_rpm: Some(1000),
                max_speed_rpm: Some(5000),
                target_speed_rpm: None,
                mode: Some(FanMode::SystemAuto),
            });
        }
        snap
    }

    fn rule(id: &str) -> ThermalRule {
        ThermalRule {
            id: id.into(),
            name: "CPU".into(),
            target: ThermalTarget::Cpu,
            low_celsius: 40.0,
            high_celsius: 80.0,
            min_fan_percent: 20,
            max_fan_percent: 100,
            active: true,
        }
    }

    fn loaded(
        settings: ThermalPolicySettings,
    ) -> (
        ThermalPolicyState<MemoryStore, RecordingFanActuation>,
        MemoryStore,
        RecordingFanActuation,
    ) {
        let store = MemoryStore::default();
        *store.value.borrow_mut() = Some(settings);
        let shared_store = store.clone();
        let fan_actuation = RecordingFanActuation {
            events: store.events.clone(),
            ..Default::default()
        };
        let shared_fan_actuation = fan_actuation.clone();
        (
            ThermalPolicyState::load(store, fan_actuation),
            shared_store,
            shared_fan_actuation,
        )
    }

    #[test]
    fn stale_snapshot_at_decision_time_forces_system_auto() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, fan_actuation) = loaded(settings);
        let stale = snapshot(
            Some(70.0),
            crate::clock::now_unix_ms().saturating_sub(60_000),
        );

        let plan = state.process_snapshot(&stale).unwrap();

        assert_eq!(plan, FanPlan::SystemAuto);
        assert!(fan_actuation.direct_requests.borrow().is_empty());
    }

    #[test]
    fn imported_settings_keep_confirmed_restore_without_redundant_writes() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        state.force_restore_system_auto().unwrap();
        state.accept_import(ThermalPolicySettings::default());
        let restores = *fan_actuation.restore_count.borrow();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();

        state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
            .unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), restores);
        assert_eq!(
            fan_actuation.direct_requests.borrow().as_slice(),
            &[DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 3000
            }]
        );
    }

    #[test]
    fn startup_loads_persisted_settings_as_authoritative() {
        let persisted = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (state, _, _) = loaded(persisted.clone());
        assert_eq!(state.current(), persisted);
    }

    #[test]
    fn startup_uses_safe_defaults_for_invalid_persisted_settings() {
        let store = MemoryStore::default();
        let mut invalid = rule("cpu");
        invalid.low_celsius = 90.0;
        invalid.high_celsius = 80.0;
        *store.value.borrow_mut() = Some(ThermalPolicySettings {
            mode: ThermalPolicyMode::Custom,
            rules: vec![invalid],
        });

        let state = ThermalPolicyState::load(store, RecordingFanActuation::default());
        assert_eq!(state.current(), ThermalPolicySettings::default());
    }

    #[test]
    fn system_auto_snapshot_heartbeats_direct_target_without_redundant_restore() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();
        state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
            .unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), 1);
        assert_eq!(
            fan_actuation
                .events
                .borrow()
                .iter()
                .filter(|event| **event == "heartbeat")
                .count(),
            1
        );
    }

    #[test]
    fn system_auto_heartbeat_failure_restores_and_returns_error() {
        let store = MemoryStore::default();
        let fan_actuation = RecordingFanActuation {
            events: store.events.clone(),
            fail_heartbeat_once: Rc::new(RefCell::new(true)),
            ..Default::default()
        };
        let shared = fan_actuation.clone();
        let mut state = ThermalPolicyState::load(store, fan_actuation);

        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();

        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
                .unwrap_err(),
            "heartbeat failed"
        );
        assert_eq!(*shared.restore_count.borrow(), 2);
    }

    #[test]
    fn system_auto_snapshots_do_not_heartbeat_without_direct_fans() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
            .unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), 1);
        assert!(fan_actuation
            .events
            .borrow()
            .iter()
            .all(|event| *event != "heartbeat"));
    }

    #[test]
    fn stale_snapshot_after_direct_target_restores_at_five_seconds() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();

        let plan = state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 6_001)
            .unwrap();

        assert_eq!(plan, FanPlan::SystemAuto);
        assert_eq!(*fan_actuation.restore_count.borrow(), 2);
        assert!(fan_actuation
            .events
            .borrow()
            .iter()
            .all(|event| *event != "heartbeat"));
    }

    #[test]
    fn unusable_snapshot_does_not_renew_direct_leases() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();
        let events_before = fan_actuation.events.borrow().len();

        let mut unavailable = snapshot(Some(70.0), 2_000);
        unavailable.temperatures = Availability::Unavailable {
            reason: "unavailable".into(),
        };
        state.process_snapshot_at(&unavailable, 2_000).unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), 2);
        assert!(fan_actuation.events.borrow()[events_before..]
            .iter()
            .all(|event| *event == "restore_all"));

        // A snapshot that no longer reports the controlled fan stays
        // fail-closed without a redundant restore either.
        let mut missing_fan = snapshot(Some(70.0), 3_000);
        missing_fan.fans = Availability::Available { value: vec![] };
        state.process_snapshot_at(&missing_fan, 3_000).unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), 2);
        assert!(fan_actuation
            .events
            .borrow()
            .iter()
            .all(|event| *event != "heartbeat"));
    }

    #[test]
    fn unchanged_target_is_reissued_when_the_fan_reports_system_auto() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, fan_actuation) = loaded(settings);
        state
            .process_snapshot_at(
                &snapshot_with_mode(Some(70.0), 1_000, FanMode::Manual),
                1_000,
            )
            .unwrap();

        // The daemon may have expired the lease and restored auto on its own;
        // the same cached RPM must be rewritten when the fan reads back auto.
        state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
            .unwrap();
        assert_eq!(fan_actuation.direct_requests.borrow().len(), 2,);

        // Reporting manual again suppresses the unchanged write.
        state
            .process_snapshot_at(
                &snapshot_with_mode(Some(70.0), 3_000, FanMode::Manual),
                3_000,
            )
            .unwrap();
        assert_eq!(fan_actuation.direct_requests.borrow().len(), 2,);
    }

    #[test]
    fn snapshot_processing_applies_changed_targets_and_heartbeats_without_persisting() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, store, fan_actuation) = loaded(settings.clone());

        let first = state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();
        let second = state
            .process_snapshot_at(
                &snapshot_with_mode(Some(70.0), 2_000, FanMode::Manual),
                2_000,
            )
            .unwrap();

        assert_eq!(
            first,
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 4657
                }]
            }
        );
        assert_eq!(second, first);
        assert_eq!(
            *fan_actuation.direct_requests.borrow(),
            vec![DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 4657
            }]
        );
        assert_eq!(*store.events.borrow(), vec!["heartbeat", "heartbeat"]);
        assert_eq!(state.current(), settings);
    }

    #[test]
    fn fail_safe_snapshots_restore_system_auto_without_redundant_writes() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, fan_actuation) = loaded(settings);
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();

        let stale = state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 6_001)
            .unwrap();
        let mut unavailable = snapshot(Some(70.0), 7_000);
        unavailable.temperatures = Availability::Unavailable {
            reason: "unavailable".into(),
        };
        let unavailable_plan = state.process_snapshot_at(&unavailable, 7_000).unwrap();
        let mut missing = snapshot(Some(70.0), 8_000);
        missing.fans = Availability::NotPresent;
        let missing_plan = state.process_snapshot_at(&missing, 8_000).unwrap();

        assert_eq!(stale, FanPlan::SystemAuto);
        assert_eq!(unavailable_plan, FanPlan::SystemAuto);
        assert_eq!(missing_plan, FanPlan::SystemAuto);
        assert_eq!(*fan_actuation.restore_count.borrow(), 1);
        assert_eq!(
            fan_actuation
                .events
                .borrow()
                .iter()
                .filter(|event| **event == "heartbeat")
                .count(),
            1
        );
    }

    #[test]
    fn target_and_heartbeat_failures_restore_and_recover_on_later_snapshots() {
        for fail_heartbeat in [false, true] {
            let settings = ThermalPolicySettings {
                mode: ThermalPolicyMode::Performance,
                rules: vec![],
            };
            let store = MemoryStore::default();
            *store.value.borrow_mut() = Some(settings);
            let fan_actuation = RecordingFanActuation {
                events: store.events.clone(),
                fail_target_once: Rc::new(RefCell::new(!fail_heartbeat)),
                fail_heartbeat_once: Rc::new(RefCell::new(fail_heartbeat)),
                ..Default::default()
            };
            let shared = fan_actuation.clone();
            let mut state = ThermalPolicyState::load(store, fan_actuation);

            let error = state
                .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
                .unwrap_err();
            assert_eq!(
                error,
                if fail_heartbeat {
                    "heartbeat failed"
                } else {
                    "target failed"
                }
            );
            assert_eq!(*shared.restore_count.borrow(), 1);

            let recovered = state
                .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
                .unwrap();
            assert!(matches!(recovered, FanPlan::Targets { .. }));
            assert_eq!(
                shared
                    .direct_requests
                    .borrow()
                    .iter()
                    .filter(|request| matches!(request, DirectFanActuationRequest::Target { .. }))
                    .count(),
                2
            );
            assert_eq!(
                shared
                    .events
                    .borrow()
                    .iter()
                    .filter(|event| **event == "heartbeat")
                    .count(),
                if fail_heartbeat { 2 } else { 1 }
            );
        }
    }

    #[test]
    fn edited_custom_rule_updates_the_next_fan_plan() {
        let initial_rule = ThermalRule {
            min_fan_percent: 20,
            ..rule("cpu")
        };
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Custom,
            rules: vec![initial_rule.clone()],
        };
        let (mut state, store, fan_actuation) = loaded(settings);

        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(40.0), 1_000), 1_000)
                .unwrap(),
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 1800,
                }]
            }
        );

        state
            .update(ThermalPolicyChange::UpsertRule(ThermalRule {
                min_fan_percent: 10,
                ..initial_rule
            }))
            .unwrap();

        assert_eq!(state.current().rules[0].min_fan_percent, 10);
        assert_eq!(
            store.value.borrow().as_ref().unwrap().rules[0].min_fan_percent,
            10
        );
        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(40.0), 2_000), 2_000)
                .unwrap(),
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 1400,
                }]
            }
        );
        assert_eq!(
            *fan_actuation.direct_requests.borrow(),
            vec![
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 1800,
                },
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 1400,
                },
            ]
        );
    }

    #[test]
    fn raising_custom_rule_updates_the_next_fan_plan() {
        let initial_rule = ThermalRule {
            min_fan_percent: 10,
            ..rule("cpu")
        };
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Custom,
            rules: vec![initial_rule.clone()],
        };
        let (mut state, _, _) = loaded(settings);

        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(40.0), 1_000), 1_000)
                .unwrap(),
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 1400,
                }]
            }
        );
        state
            .update(ThermalPolicyChange::UpsertRule(ThermalRule {
                min_fan_percent: 20,
                ..initial_rule
            }))
            .unwrap();
        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(40.0), 2_000), 2_000)
                .unwrap(),
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 1800,
                }]
            }
        );
    }

    #[test]
    fn failed_updates_preserve_evaluator_history() {
        for persistence_failure in [false, true] {
            let settings = ThermalPolicySettings {
                mode: ThermalPolicyMode::Performance,
                rules: vec![],
            };
            let store = MemoryStore {
                fail_save: persistence_failure,
                ..Default::default()
            };
            *store.value.borrow_mut() = Some(settings.clone());
            let fan_actuation = RecordingFanActuation {
                events: store.events.clone(),
                ..Default::default()
            };
            let mut state = ThermalPolicyState::load(store, fan_actuation);
            assert_eq!(
                state
                    .process_snapshot_at(&snapshot(Some(75.0), 1_000), 1_000)
                    .unwrap(),
                FanPlan::Targets {
                    targets: vec![super::super::contract::FanTarget {
                        fan_id: 0,
                        rpm: 5000,
                    }]
                }
            );

            let result = if persistence_failure {
                state.update(ThermalPolicyChange::SelectMode(ThermalPolicyMode::Quiet))
            } else {
                let mut invalid = rule("cpu");
                invalid.low_celsius = 90.0;
                invalid.high_celsius = 80.0;
                state.update(ThermalPolicyChange::UpsertRule(invalid))
            };
            assert!(result.is_err());
            assert_eq!(state.current(), settings);
            assert_eq!(
                state
                    .process_snapshot_at(&snapshot(Some(40.0), 2_000), 2_000)
                    .unwrap(),
                FanPlan::Targets {
                    targets: vec![super::super::contract::FanTarget {
                        fan_id: 0,
                        rpm: 4600,
                    }]
                }
            );
        }
    }

    #[test]
    fn direct_target_and_system_auto_succeed_in_system_auto_mode() {
        let settings = ThermalPolicySettings::default();
        let (mut state, store, fan_actuation) = loaded(settings.clone());

        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 1,
                    rpm: 3200,
                },
                Some(&snapshot_two_fans(Some(70.0), 1_000)),
                1_000,
            )
            .unwrap();
        state
            .direct_actuation_at(
                DirectFanActuationRequest::SystemAuto { fan_id: 1 },
                None,
                1_000,
            )
            .unwrap();

        assert_eq!(state.current(), settings);
        assert!(store.events.borrow().is_empty());
        assert_eq!(
            *fan_actuation.direct_requests.borrow(),
            vec![
                DirectFanActuationRequest::Target {
                    fan_id: 1,
                    rpm: 3200,
                },
                DirectFanActuationRequest::SystemAuto { fan_id: 1 },
            ]
        );
    }

    #[test]
    fn active_policy_modes_reject_direct_requests_without_actuation() {
        for mode in [
            ThermalPolicyMode::Quiet,
            ThermalPolicyMode::Performance,
            ThermalPolicyMode::Custom,
        ] {
            let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings {
                mode,
                rules: vec![],
            });

            assert!(state
                .direct_actuation_at(
                    DirectFanActuationRequest::Target {
                        fan_id: 0,
                        rpm: 3000,
                    },
                    Some(&snapshot(Some(70.0), 1_000)),
                    1_000
                )
                .is_err());
            assert!(state
                .direct_actuation_at(
                    DirectFanActuationRequest::SystemAuto { fan_id: 0 },
                    None,
                    1_000
                )
                .is_err());
            assert!(fan_actuation.direct_requests.borrow().is_empty());
        }
    }

    #[test]
    fn direct_actuation_failures_are_returned_without_mutating_settings() {
        for request in [
            DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 3000,
            },
            DirectFanActuationRequest::SystemAuto { fan_id: 0 },
        ] {
            let settings = ThermalPolicySettings::default();
            let store = MemoryStore::default();
            let shared_store = store.clone();
            let fan_actuation = RecordingFanActuation {
                fail_direct: true,
                ..Default::default()
            };
            let mut state = ThermalPolicyState::load(store, fan_actuation);

            assert_eq!(
                state
                    .direct_actuation_at(request, Some(&snapshot(Some(70.0), 1_000)), 1_000)
                    .unwrap_err(),
                "direct actuation failed"
            );
            assert_eq!(state.current(), settings);
            assert!(shared_store.events.borrow().is_empty());
            assert!(shared_store.value.borrow().is_none());
        }
    }

    #[test]
    fn direct_target_requires_fresh_usable_telemetry() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        // At the freshness boundary a manual target is still accepted.
        state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 1_000)),
                6_000,
            )
            .unwrap();
        assert_eq!(
            fan_actuation.direct_requests.borrow().as_slice(),
            &[DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 3000
            }]
        );

        // One millisecond later the same request cannot establish ownership;
        // the restore runs first, and the refusal does not reapply afterwards.
        let stale = snapshot(Some(70.0), 1_000);
        assert!(state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3500,
                },
                Some(&stale),
                6_001,
            )
            .is_err());
        // No telemetry at all can never authorize a manual target either.
        assert!(state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3500,
                },
                None,
                6_001,
            )
            .is_err());
        assert_eq!(
            fan_actuation.direct_requests.borrow().as_slice(),
            &[DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 3000
            }]
        );
        assert!(state.direct_fans.is_empty());
    }

    #[test]
    fn dust_blast_holds_targets_without_touching_persisted_settings() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, store, fan_actuation) = loaded(settings.clone());

        let id = state
            .start_dust_blast_at(&snapshot(Some(70.0), 1_000), || 1_000)
            .unwrap();
        assert!(state.dust_blast_is_active(id));
        // The persisted preset is untouched: no save, settings unchanged.
        assert_eq!(state.current(), settings);
        assert!(store.events.borrow().is_empty());
        assert_eq!(
            fan_actuation.direct_requests.borrow().as_slice(),
            &[DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 5000
            }]
        );

        // A fresh usable snapshot heartbeats the blast targets once per tick.
        assert_eq!(
            state
                .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
                .unwrap(),
            FanPlan::Targets {
                targets: vec![FanTarget {
                    fan_id: 0,
                    rpm: 5000
                }]
            }
        );

        // A stale snapshot clears the override via the restore path; the
        // selected Performance preset then resumes normally on the next fresh
        // sample — but the blast's max-RPM target is never reissued.
        let plan = state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 8_000)
            .unwrap();
        assert_eq!(plan, FanPlan::SystemAuto);
        assert!(!state.has_active_blast());
        state
            .process_snapshot_at(&snapshot(Some(70.0), 9_000), 9_000)
            .unwrap();
        assert_eq!(
            fan_actuation
                .direct_requests
                .borrow()
                .iter()
                .filter(|request| {
                    matches!(request, DirectFanActuationRequest::Target { rpm, .. } if *rpm == 5000)
                })
                .count(),
            1
        );
    }

    #[test]
    fn dust_blast_tokens_do_not_restore_over_newer_sessions() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        let first = state
            .start_dust_blast_at(&snapshot(Some(70.0), 1_000), || 1_000)
            .unwrap();
        state.cancel_dust_blast().unwrap();
        let second = state
            .start_dust_blast_at(&snapshot(Some(70.0), 2_000), || 2_000)
            .unwrap();
        assert_ne!(first, second);
        assert!(!state.dust_blast_is_active(first));
        assert!(state.dust_blast_is_active(second));

        // The first countdown's expiry cannot restore over the newer blast.
        let restores = *fan_actuation.restore_count.borrow();
        state.finish_dust_blast(first).unwrap();
        assert!(state.dust_blast_is_active(second));
        assert_eq!(*fan_actuation.restore_count.borrow(), restores);

        state.finish_dust_blast(second).unwrap();
        assert!(!state.has_active_blast());
    }

    #[test]
    fn explicit_mode_selection_cancels_dust_blast_without_overwriting_mode() {
        let (mut state, _, _) = loaded(ThermalPolicySettings::default());
        let id = state
            .start_dust_blast_at(&snapshot(Some(70.0), 1_000), || 1_000)
            .unwrap();

        let updated = state
            .update(ThermalPolicyChange::SelectMode(ThermalPolicyMode::Quiet))
            .unwrap();
        assert_eq!(updated.mode, ThermalPolicyMode::Quiet);
        assert!(!state.has_active_blast());

        // Neither the stale token nor a finish call can resurrect the blast
        // or the previous mode.
        state.finish_dust_blast(id).unwrap();
        state.cancel_dust_blast().unwrap();
        assert_eq!(state.current().mode, ThermalPolicyMode::Quiet);
        assert!(!state.has_active_blast());
        // Manual targets are refused while a blast is active and a per-fan
        // SystemAuto is honored as a cancel.
        let id = state
            .start_dust_blast_at(&snapshot(Some(70.0), 3_000), || 3_000)
            .unwrap();
        assert!(state
            .direct_actuation_at(
                DirectFanActuationRequest::Target {
                    fan_id: 0,
                    rpm: 3000,
                },
                Some(&snapshot(Some(70.0), 3_000)),
                3_000,
            )
            .is_err());
        state
            .direct_actuation_at(
                DirectFanActuationRequest::SystemAuto { fan_id: 0 },
                None,
                3_000,
            )
            .unwrap();
        assert!(!state.dust_blast_is_active(id));
    }

    #[test]
    fn failed_dust_blast_start_keeps_the_selected_preset() {
        // No reported max RPM: nothing is written, nothing is persisted.
        let (mut state, store, fan_actuation) = loaded(ThermalPolicySettings::default());
        let mut no_max = snapshot(Some(70.0), 1_000);
        if let Availability::Available { value } = &mut no_max.fans {
            for fan in value.iter_mut() {
                fan.max_speed_rpm = None;
            }
        }
        assert!(state.start_dust_blast_at(&no_max, || 1_000).is_err());
        assert!(!state.has_active_blast());
        assert!(fan_actuation.direct_requests.borrow().is_empty());
        assert!(store.events.borrow().is_empty());

        // An actuation failure part-way through start recovers and keeps the
        // preset.
        let store = MemoryStore::default();
        let fan_actuation = RecordingFanActuation {
            events: store.events.clone(),
            fail_direct: true,
            ..Default::default()
        };
        let shared = fan_actuation.clone();
        let mut state = ThermalPolicyState::load(store, fan_actuation);
        assert!(state
            .start_dust_blast_at(&snapshot(Some(70.0), 1_000), || 1_000)
            .is_err());
        assert!(!state.has_active_blast());
        assert_eq!(state.current().mode, ThermalPolicyMode::SystemAuto);
        assert_eq!(*shared.restore_count.borrow(), 1);
    }

    #[test]
    fn dust_blast_start_rechecks_the_clock_per_write() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        // Preflight and the first fan write observe a fresh clock; the second
        // write lands at 5001ms and must refuse instead of writing stale.
        let calls = Cell::new(0_u64);
        let now = || {
            let call = calls.get();
            calls.set(call + 1);
            if call < 2 {
                1_000
            } else {
                6_001
            }
        };
        assert!(state
            .start_dust_blast_at(&snapshot_two_fans(Some(70.0), 1_000), now)
            .is_err());
        assert!(!state.has_active_blast());
        assert_eq!(
            fan_actuation.direct_requests.borrow().as_slice(),
            &[DirectFanActuationRequest::Target {
                fan_id: 0,
                rpm: 5000
            }]
        );
        assert!(*fan_actuation.restore_count.borrow() >= 1);
    }

    #[test]
    fn cancel_and_finish_edge_cases_do_not_restore_unrelated_fans() {
        let (mut state, _, fan_actuation) = loaded(ThermalPolicySettings::default());
        // No active blast: cancel restores nothing.
        state.cancel_dust_blast().unwrap();
        assert_eq!(*fan_actuation.restore_count.borrow(), 0);
        // A stale finish token restores nothing either.
        state.finish_dust_blast(999).unwrap();
        assert_eq!(*fan_actuation.restore_count.borrow(), 0);

        // Restore failures on cancel/finish propagate instead of hiding.
        let store = MemoryStore::default();
        let fan_actuation = RecordingFanActuation {
            events: store.events.clone(),
            fail_restore: true,
            ..Default::default()
        };
        let shared = fan_actuation.clone();
        let mut state = ThermalPolicyState::load(store, fan_actuation);
        let id = state
            .start_dust_blast_at(&snapshot(Some(70.0), 1_000), || 1_000)
            .unwrap();
        assert!(state.finish_dust_blast(id).is_err());
        assert!(!state.has_active_blast());
        let _ = shared;
    }

    #[test]
    fn automatic_plan_rechecks_freshness_between_fan_writes() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, fan_actuation) = loaded(settings);
        // Calls: stale recheck, evaluate, fan-0 write check (fresh), fan-1
        // write check (5001ms stale -> confirmed restore, no second write,
        // no heartbeat).
        let calls = Cell::new(0_u64);
        let now = || {
            let call = calls.get();
            calls.set(call + 1);
            if call < 3 {
                2_000
            } else {
                7_001
            }
        };
        let plan = state
            .process_snapshot_with_clock(&snapshot_two_fans(Some(70.0), 2_000), now)
            .unwrap();
        assert_eq!(plan, FanPlan::SystemAuto);
        let writes = fan_actuation
            .direct_requests
            .borrow()
            .iter()
            .filter(|request| matches!(request, DirectFanActuationRequest::Target { .. }))
            .count();
        assert_eq!(writes, 1);
        assert!(fan_actuation
            .events
            .borrow()
            .iter()
            .all(|event| *event != "heartbeat"));
        assert!(*fan_actuation.restore_count.borrow() >= 1);
    }

    #[test]
    fn mode_selection_persists_and_commits_authoritative_settings() {
        let (mut state, store, _) = loaded(ThermalPolicySettings::default());
        let updated = state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::Performance,
            ))
            .unwrap();
        assert_eq!(updated.mode, ThermalPolicyMode::Performance);
        assert_eq!(state.current(), updated);
        assert_eq!(*store.value.borrow(), Some(updated));
    }

    #[test]
    fn rule_upsert_selects_custom_and_rule_deletion_persists() {
        let (mut state, store, _) = loaded(ThermalPolicySettings::default());
        let updated = state
            .update(ThermalPolicyChange::UpsertRule(rule("cpu")))
            .unwrap();
        assert_eq!(updated.mode, ThermalPolicyMode::Custom);
        assert_eq!(updated.rules, vec![rule("cpu")]);

        let deleted = state
            .update(ThermalPolicyChange::DeleteRule("cpu".into()))
            .unwrap();
        assert!(deleted.rules.is_empty());
        assert_eq!(*store.value.borrow(), Some(deleted));
    }

    #[test]
    fn validation_failure_leaves_state_unpersisted_and_performs_no_effect() {
        let (mut state, store, fan_actuation) = loaded(ThermalPolicySettings::default());
        let mut invalid = rule("cpu");
        invalid.low_celsius = 90.0;
        invalid.high_celsius = 80.0;

        assert!(state
            .update(ThermalPolicyChange::UpsertRule(invalid))
            .is_err());
        assert_eq!(state.current(), ThermalPolicySettings::default());
        assert_eq!(
            *store.value.borrow(),
            Some(ThermalPolicySettings::default())
        );
        assert_eq!(*fan_actuation.restore_count.borrow(), 0);
    }

    #[test]
    fn persistence_failure_leaves_state_unchanged_and_performs_no_effect() {
        let store = MemoryStore {
            fail_save: true,
            ..Default::default()
        };
        let fan_actuation = RecordingFanActuation::default();
        let shared_fan_actuation = fan_actuation.clone();
        let mut state = ThermalPolicyState::load(store, fan_actuation);

        assert!(state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::SystemAuto
            ))
            .is_err());
        assert_eq!(state.current(), ThermalPolicySettings::default());
        assert_eq!(*shared_fan_actuation.restore_count.borrow(), 0);
    }

    #[test]
    fn system_auto_mode_cycle_clears_evaluator_history() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, _) = loaded(settings);
        state
            .process_snapshot_at(&snapshot(Some(75.0), 1_000), 1_000)
            .unwrap();
        state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::SystemAuto,
            ))
            .unwrap();
        state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::Performance,
            ))
            .unwrap();

        let plan = state
            .process_snapshot_at(&snapshot(Some(40.0), 2_000), 2_000)
            .unwrap();
        assert_eq!(
            plan,
            FanPlan::Targets {
                targets: vec![super::super::contract::FanTarget {
                    fan_id: 0,
                    rpm: 2600,
                }]
            }
        );
    }

    #[test]
    fn explicit_system_auto_selection_clears_runtime_state() {
        let settings = ThermalPolicySettings {
            mode: ThermalPolicyMode::Performance,
            rules: vec![],
        };
        let (mut state, _, fan_actuation) = loaded(settings);
        state
            .process_snapshot_at(&snapshot(Some(70.0), 1_000), 1_000)
            .unwrap();

        state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::SystemAuto,
            ))
            .unwrap();
        state
            .process_snapshot_at(&snapshot(Some(70.0), 2_000), 2_000)
            .unwrap();

        assert_eq!(*fan_actuation.restore_count.borrow(), 1);
    }

    #[test]
    fn rule_deletion_while_system_auto_performs_no_actuation() {
        let (mut state, store, fan_actuation) = loaded(ThermalPolicySettings {
            mode: ThermalPolicyMode::SystemAuto,
            rules: vec![rule("cpu")],
        });

        let updated = state
            .update(ThermalPolicyChange::DeleteRule("cpu".into()))
            .unwrap();

        assert!(updated.rules.is_empty());
        assert_eq!(*store.value.borrow(), Some(updated));
        assert_eq!(*fan_actuation.restore_count.borrow(), 0);
    }

    #[test]
    fn persisted_system_auto_transition_commits_before_restoring_all_fans() {
        let initial = ThermalPolicySettings {
            mode: ThermalPolicyMode::Quiet,
            rules: vec![],
        };
        let (mut state, store, fan_actuation) = loaded(initial);
        let updated = state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::SystemAuto,
            ))
            .unwrap();

        assert_eq!(*store.value.borrow(), Some(updated.clone()));
        assert_eq!(state.current(), updated);
        assert_eq!(*fan_actuation.restore_count.borrow(), 1);
        assert_eq!(*store.events.borrow(), vec!["persist", "restore_all"]);
    }

    #[test]
    fn actuation_failure_keeps_the_persisted_authoritative_transition() {
        let store = MemoryStore::default();
        *store.value.borrow_mut() = Some(ThermalPolicySettings {
            mode: ThermalPolicyMode::Quiet,
            rules: vec![],
        });
        let shared_store = store.clone();
        let fan_actuation = RecordingFanActuation {
            events: store.events.clone(),
            fail_restore: true,
            ..Default::default()
        };
        let mut state = ThermalPolicyState::load(store, fan_actuation);

        assert!(state
            .update(ThermalPolicyChange::SelectMode(
                ThermalPolicyMode::SystemAuto,
            ))
            .is_err());
        assert_eq!(state.current().mode, ThermalPolicyMode::SystemAuto);
        assert_eq!(
            shared_store.value.borrow().as_ref().unwrap().mode,
            ThermalPolicyMode::SystemAuto
        );
    }

    #[test]
    fn runtime_mode_override_boosts_without_mutating_persisted_settings() {
        let (mut state, store, fan_actuation) = loaded(ThermalPolicySettings {
            mode: ThermalPolicyMode::SystemAuto,
            rules: vec![],
        });
        assert!(state.set_runtime_mode_override(Some(ThermalPolicyMode::Performance)));
        assert_eq!(state.current().mode, ThermalPolicyMode::SystemAuto);
        assert_eq!(
            store.value.borrow().as_ref().unwrap().mode,
            ThermalPolicyMode::SystemAuto
        );

        let snap = snapshot(Some(78.0), 10_000);
        let plan = state.process_snapshot_at(&snap, 10_000).unwrap();
        assert!(matches!(
            plan,
            super::super::contract::FanPlan::Targets { .. }
        ));
        assert!(!fan_actuation.direct_requests.borrow().is_empty());

        assert!(state.set_runtime_mode_override(None));
        let plan_after = state.process_snapshot_at(&snap, 11_000).unwrap();
        assert!(matches!(
            plan_after,
            super::super::contract::FanPlan::SystemAuto
        ));
    }

    #[test]
    fn performance_apps_trigger_boosts_without_persisting_and_clears_when_idle() {
        let (mut state, store, _fan_actuation) = loaded(ThermalPolicySettings {
            mode: ThermalPolicyMode::SystemAuto,
            rules: vec![],
        });
        let triggers = crate::application_preferences::contract::AutoThermalTriggers {
            performance_on_heavy_load: false,
            quiet_on_battery: false,
            performance_apps: vec!["Xcode".into()],
        };
        let mut snap = snapshot(Some(52.0), 500_000);
        snap.top_processes = Availability::Available {
            value: vec![crate::hardware_telemetry::contract::ProcessReading {
                name: "Xcode".into(),
                cpu_percent: 25.0,
                memory_mb: 1200.0,
                process_count: 2,
                primary_pid: 999,
                is_paused: false,
            }],
        };
        state.apply_auto_thermal_triggers(&triggers, &snap);
        let plan = state.process_snapshot_at(&snap, 500_000).unwrap();
        assert!(matches!(
            plan,
            super::super::contract::FanPlan::Targets { .. }
        ));
        assert_eq!(state.current().mode, ThermalPolicyMode::SystemAuto);
        assert_eq!(
            store.value.borrow().as_ref().unwrap().mode,
            ThermalPolicyMode::SystemAuto
        );

        snap.top_processes = Availability::Available { value: vec![] };
        state.apply_auto_thermal_triggers(&triggers, &snap);
        let cleared = state.process_snapshot_at(&snap, 501_000).unwrap();
        assert!(matches!(
            cleared,
            super::super::contract::FanPlan::SystemAuto
        ));
    }
}
