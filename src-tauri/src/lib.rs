mod alerts;
mod app_update;
mod application_preferences;
mod awake;
mod clock;
pub mod fan_actuation;
mod hardware_telemetry;
mod macos_bridge;
pub mod mcp;
mod menu_bar;
mod native_window;
mod ports;
mod power_management;
mod proc_info;
mod profile_files;
mod smc;
mod system_extras;
mod thermal_policy;
mod tray_menu;

use application_preferences::adapters::MacLoginItem;
use application_preferences::contract::{
    ApplicationPreferenceChange, ApplicationPreferences, LayoutDensity,
};
use application_preferences::preferences::ApplicationPreferencesModule;
use fan_actuation::client;
use hardware_telemetry::contract::{Availability, FanActuationStatus, HardwareTelemetrySnapshot};
use profile_files::{ConfigurationBundle, ProfileFiles};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use thermal_policy::adapters::ProductionFanActuation;
use thermal_policy::contract::{ThermalPolicyMode, ThermalPolicySettings, ThermalRule};
use thermal_policy::state::{DirectFanActuationRequest, ThermalPolicyChange, ThermalPolicyState};

const MAX_TRAY_HISTORY_POINTS: usize = 24;

type PreferencesModule = ApplicationPreferencesModule<ProfileFiles, MacLoginItem>;

struct ApplicationPreferencesState {
    /// Serializes every mutation *and* its publication effects (watches,
    /// layout, tray refresh, UI emit). Acquired only on blocking workers —
    /// never on the menu/main thread. Lock order: publication -> preferences
    /// -> policy.
    publication: Mutex<()>,
    preferences: Mutex<PreferencesModule>,
    telemetry_interval: tokio::sync::watch::Sender<u32>,
    layout_density: tokio::sync::watch::Sender<LayoutDensity>,
    dynamic_island_display: tokio::sync::watch::Sender<
        crate::application_preferences::contract::DynamicIslandDisplayMode,
    >,
}

#[derive(Default)]
struct TrayRuntimeCache {
    latest_snapshot: Mutex<Option<HardwareTelemetrySnapshot>>,
    temp_history: Mutex<Vec<f64>>,
    last_tray_rect: Mutex<Option<tauri::Rect>>,
    last_status_line: Mutex<Option<String>>,
    last_tray_title: Mutex<Option<String>>,
    last_tray_icon_hash: AtomicU64,
    island_collapsed: AtomicBool,
    active_display_index: std::sync::atomic::AtomicUsize,
    last_collapse_unix_ms: AtomicU64,
    island_peek_until_unix_ms: AtomicU64,
    island_alert_active: AtomicBool,
    island_alert_seq: AtomicU64,
    prev_cam_or_mic_active: AtomicBool,
    keep_awake_active: AtomicBool,
    alerts: Mutex<alerts::AlertEvaluator>,
}

impl TrayRuntimeCache {
    fn record_snapshot(&self, snapshot: &HardwareTelemetrySnapshot) -> Vec<f64> {
        *self.latest_snapshot.lock().unwrap() = Some(snapshot.clone());
        let mut history = self.temp_history.lock().unwrap();
        if let Availability::Available { value } = &snapshot.temperatures {
            if let Some(cpu) = value.cpu_celsius {
                history.push(cpu);
                if history.len() > MAX_TRAY_HISTORY_POINTS {
                    let excess = history.len() - MAX_TRAY_HISTORY_POINTS;
                    history.drain(..excess);
                }
            }
        }
        history.clone()
    }
}

type PolicyModule = ThermalPolicyState<ProfileFiles, ProductionFanActuation>;
type PolicyState = Arc<Mutex<PolicyModule>>;

// An async IPC response does not offload synchronous OS calls or lock waits.
async fn blocking_command<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn fan_actuation_status() -> Result<FanActuationStatus, String> {
    blocking_command(|| Ok(client::status())).await
}

pub(crate) fn telemetry_snapshot() -> HardwareTelemetrySnapshot {
    hardware_telemetry::capture_now(client::cached_status())
}

pub(crate) fn tray_telemetry_snapshot(include_processes: bool) -> HardwareTelemetrySnapshot {
    if include_processes {
        hardware_telemetry::capture_now(client::cached_status())
    } else {
        hardware_telemetry::capture_tray_now(client::cached_status())
    }
}

#[tauri::command]
async fn fetch_telemetry(
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
) -> Result<HardwareTelemetrySnapshot, String> {
    // A tray-loop capture within the current cadence is already fresh; only a
    // missing or stale one costs a new SMC read. Future timestamps never
    // count as fresh.
    let cadence_ms = *state.telemetry_interval.borrow() as u64;
    let cached = tray_cache.latest_snapshot.lock().unwrap().clone();
    if let Some(snapshot) = cached {
        let fresh = clock::now_unix_ms()
            .checked_sub(snapshot.captured_at_unix_ms)
            .is_some_and(|age| age <= cadence_ms);
        if fresh && !matches!(snapshot.top_processes, Availability::NotPresent) {
            return Ok(snapshot);
        }
    }
    tokio::task::spawn_blocking(telemetry_snapshot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn application_preferences(
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
) -> Result<ApplicationPreferences, String> {
    let state = state.inner().clone();
    blocking_command(move || Ok(state.preferences.lock().unwrap().current())).await
}

#[tauri::command]
async fn update_application_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    change: ApplicationPreferenceChange,
) -> Result<ApplicationPreferences, String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        apply_preference_action(&app, &state, &tray_cache, PreferenceAction::Set(change))
    })
    .await
}

// Invoked once by the UI after React has committed; proves the updated app
// reached a running UI to the parent waiting on the handshake socket.
#[tauri::command]
fn confirm_update_startup() {
    app_update::ui_ready();
}

#[tauri::command]
async fn confirmed_application_preferences(
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
) -> Result<ApplicationPreferences, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.preferences.lock().unwrap().confirmed_current())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn export_configuration_bundle(
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    policy_state: tauri::State<'_, PolicyState>,
) -> Result<ConfigurationBundle, String> {
    let preferences_state = preferences_state.inner().clone();
    let policy_state = policy_state.inner().clone();
    tokio::task::spawn_blocking(move || {
        // The OS login query runs before the policy lock is ever taken, so
        // osascript is never invoked while holding it.
        let mut preferences = preferences_state.preferences.lock().unwrap();
        let confirmed = preferences.confirmed_current()?;
        let policy = policy_state.lock().unwrap();
        Ok(ConfigurationBundle {
            version: 1,
            preferences: confirmed,
            thermal_policy: policy.current(),
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn import_configuration_bundle(
    app: tauri::AppHandle,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    policy_state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    profile_files: tauri::State<'_, ProfileFiles>,
    login_item: tauri::State<'_, MacLoginItem>,
    bundle: ConfigurationBundle,
) -> Result<ConfigurationBundle, String> {
    let preferences_state = preferences_state.inner().clone();
    let policy_state = policy_state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    let profile_files = profile_files.inner().clone();
    let mut login_item = login_item.inner().clone();
    blocking_command(move || {
        let _publication = preferences_state.publication.lock().unwrap();
        let mut preferences = preferences_state.preferences.lock().unwrap();
        let mut policy = policy_state.lock().unwrap();
        if policy.has_active_blast() {
            return Err("Stop Dust Blast before importing.".into());
        }
        let previous = preferences.current();
        profile_files.import(&bundle, &mut login_item, || {
            policy.force_restore_system_auto()
        })?;
        preferences.accept_import(bundle.preferences.clone());
        policy.accept_import(bundle.thermal_policy.clone());
        let updated_prefs = bundle.preferences.clone();
        let updated_policy = bundle.thermal_policy.clone();
        drop(policy);
        drop(preferences);

        publish_preferences(
            &app,
            &preferences_state,
            &tray_cache,
            &previous,
            &updated_prefs,
        );
        tray_menu::sync_menu_policy(&app, &updated_policy.mode);
        let _ = app.emit("thermal-policy-updated", &updated_policy);

        Ok(ConfigurationBundle {
            version: 1,
            preferences: updated_prefs,
            thermal_policy: updated_policy,
        })
    })
    .await
}

#[tauri::command]
async fn thermal_policy_settings(
    state: tauri::State<'_, PolicyState>,
) -> Result<ThermalPolicySettings, String> {
    let state = state.inner().clone();
    blocking_command(move || Ok(state.lock().unwrap().current())).await
}

pub(crate) fn apply_thermal_policy_action(
    app: &tauri::AppHandle,
    state: &PolicyState,
    tray_cache: &TrayRuntimeCache,
    change: ThermalPolicyChange,
    emit_mode_alert: bool,
) -> Result<ThermalPolicySettings, String> {
    let snapshot = tray_cache.latest_snapshot.lock().unwrap().clone();
    let updated = {
        let mut policy = state.lock().unwrap();
        let updated = policy.update(change)?;
        if let Some(snapshot) = snapshot.as_ref() {
            let _ = policy.process_snapshot(snapshot);
        }
        updated
    };
    tray_menu::sync_menu_policy(app, &updated.mode);
    let _ = app.emit("thermal-policy-updated", &updated);
    if emit_mode_alert {
        native_window::trigger_island_alert(
            app,
            format!("Fan Mode · {}", updated.mode.short_label()),
            None,
            "info",
            Some("fans"),
            native_window::ISLAND_PEEK_ORDINARY_MS,
        );
    }
    Ok(updated)
}

#[tauri::command]
async fn select_thermal_policy_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    mode: ThermalPolicyMode,
) -> Result<ThermalPolicySettings, String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        apply_thermal_policy_action(
            &app,
            &state,
            &tray_cache,
            ThermalPolicyChange::SelectMode(mode),
            true,
        )
    })
    .await
}

#[tauri::command]
async fn upsert_thermal_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    rule: ThermalRule,
) -> Result<ThermalPolicySettings, String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        apply_thermal_policy_action(
            &app,
            &state,
            &tray_cache,
            ThermalPolicyChange::UpsertRule(rule),
            false,
        )
    })
    .await
}

#[tauri::command]
async fn delete_thermal_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    rule_id: String,
) -> Result<ThermalPolicySettings, String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        apply_thermal_policy_action(
            &app,
            &state,
            &tray_cache,
            ThermalPolicyChange::DeleteRule(rule_id),
            false,
        )
    })
    .await
}

#[tauri::command]
async fn set_fan_speed(
    state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    fan_id: usize,
    rpm: i32,
) -> Result<(), String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        let snapshot = tray_cache.latest_snapshot.lock().unwrap().clone();
        state.lock().unwrap().direct_actuation(
            DirectFanActuationRequest::Target { fan_id, rpm },
            snapshot.as_ref(),
        )
    })
    .await
}

#[tauri::command]
async fn set_fan_mode(
    state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    fan_id: usize,
    mode: String,
    rpm: Option<i32>,
) -> Result<(), String> {
    let state = state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        let request = match mode.as_str() {
            "auto" => DirectFanActuationRequest::SystemAuto { fan_id },
            "manual" => DirectFanActuationRequest::Target {
                fan_id,
                rpm: rpm.ok_or_else(|| "manual mode requires a target RPM".to_string())?,
            },
            _ => return Err("fan mode must be auto or manual".into()),
        };
        // SystemAuto is allowed with no telemetry; a manual Target is
        // validated against the most recent tray snapshot cloned before the
        // policy lock.
        let snapshot = tray_cache.latest_snapshot.lock().unwrap().clone();
        state
            .lock()
            .unwrap()
            .direct_actuation(request, snapshot.as_ref())
    })
    .await
}

pub(crate) enum KeepAwakeRuleField {
    OnAc,
    Ports,
    AiAgents,
}

/// Tray menu entries that mutate application preferences: an explicit set,
/// toggles whose value is derived under the store lock, or a layout
/// reselection that re-shows/focuses the window.
pub(crate) enum PreferenceAction {
    Set(ApplicationPreferenceChange),
    ToggleTrafficLight,
    ToggleKeepAwakeRule(KeepAwakeRuleField),
    ActivateLayout(LayoutDensity),
}

/// The single writer path for preference mutations: one publication guard
/// serializes the atomic read/modify/update and every downstream publication
/// effect. Only called on blocking workers (command worker or tray-spawned
/// thread); the publication guard stays held until all effects finish, and
/// the store/policy locks are released before any UI geometry or emit.
pub(crate) fn apply_preference_action(
    app: &tauri::AppHandle,
    state: &Arc<ApplicationPreferencesState>,
    cache: &Arc<TrayRuntimeCache>,
    action: PreferenceAction,
) -> Result<ApplicationPreferences, String> {
    let _publication = state.publication.lock().unwrap();
    let tray_reselect = matches!(action, PreferenceAction::ActivateLayout(_));
    let (previous, updated) = {
        let mut preferences = state.preferences.lock().unwrap();
        let change = match action {
            PreferenceAction::Set(change) => change,
            PreferenceAction::ToggleTrafficLight => {
                ApplicationPreferenceChange::SetTrafficLightMenuBar {
                    value: !preferences.current().traffic_light_menu_bar,
                }
            }
            PreferenceAction::ToggleKeepAwakeRule(field) => {
                let mut rules = preferences.current().keep_awake_rules;
                match field {
                    KeepAwakeRuleField::OnAc => rules.while_on_ac = !rules.while_on_ac,
                    KeepAwakeRuleField::Ports => {
                        rules.while_ports_active = !rules.while_ports_active
                    }
                    KeepAwakeRuleField::AiAgents => {
                        rules.while_ai_agents_active = !rules.while_ai_agents_active
                    }
                }
                ApplicationPreferenceChange::SetKeepAwakeRules { value: rules }
            }
            PreferenceAction::ActivateLayout(density) => {
                ApplicationPreferenceChange::SetLayoutDensity { value: density }
            }
        };
        let previous = preferences.current();
        (previous, preferences.update(change)?)
    };
    publish_preferences(app, state, cache, &previous, &updated);
    if tray_reselect {
        // A real density change is shown by publish_preferences; a
        // reselection re-shows/focuses for visible feedback either way.
        if updated.layout_density == previous.layout_density {
            native_window::show_layout(app, cache, updated.layout_density);
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_focus();
        }
    }
    Ok(updated)
}

/// One runtime path for every committed preference change (command, import,
/// or tray menu): publishes watch channels, applies a real layout change,
/// refreshes the tray, syncs checkmarks/shortcuts/keep-awake rules, and
/// notifies the UI. Call after the store/policy locks are released.
pub(crate) fn publish_preferences(
    app: &tauri::AppHandle,
    state: &ApplicationPreferencesState,
    cache: &TrayRuntimeCache,
    previous: &ApplicationPreferences,
    updated: &ApplicationPreferences,
) {
    if updated.telemetry_interval_ms != previous.telemetry_interval_ms {
        state
            .telemetry_interval
            .send_replace(updated.telemetry_interval_ms);
    }
    if updated.dynamic_island_display != previous.dynamic_island_display {
        state.dynamic_island_display.send_if_modified(|current| {
            if *current != updated.dynamic_island_display {
                *current = updated.dynamic_island_display;
                true
            } else {
                false
            }
        });
    }
    if updated.layout_density != previous.layout_density {
        state.layout_density.send_if_modified(|current| {
            if *current != updated.layout_density {
                *current = updated.layout_density;
                true
            } else {
                false
            }
        });
        native_window::show_layout(app, cache, updated.layout_density);
        native_window::sync_island_displays(
            app,
            cache,
            updated.layout_density,
            updated.dynamic_island_display,
        );
    } else if updated.dynamic_island_display != previous.dynamic_island_display {
        native_window::sync_island_displays(
            app,
            cache,
            updated.layout_density,
            updated.dynamic_island_display,
        );
    }
    crate::tray_menu::refresh_tray_from_cache(app, cache, updated);
    crate::tray_menu::sync_menu_prefs(app, updated);
    if updated.global_shortcut != previous.global_shortcut {
        native_window::apply_global_shortcut(app, updated.global_shortcut);
    }
    if updated.keep_awake_rules != previous.keep_awake_rules {
        if let Some(awake_state) = app.try_state::<Arc<Mutex<awake::KeepAwakeManager>>>() {
            let next_rules = updated.keep_awake_rules;
            if let Ok(status) = awake::apply_rule_change(&awake_state, move |committed| {
                *committed = next_rules;
            }) {
                crate::tray_menu::sync_menu_awake(app, &status);
            }
        }
    }
    let _ = app.emit("preferences-updated", updated);
}

#[tauri::command]
async fn install_fan_actuation_helper(app: tauri::AppHandle) -> Result<FanActuationStatus, String> {
    blocking_command(move || {
        let resource_directory = app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join("fan-actuation");
        fan_actuation::installer::install(&resource_directory)?;
        Ok(client::status())
    })
    .await
}

#[tauri::command]
fn toggle_popover(app: tauri::AppHandle) {
    native_window::toggle_main_window_visibility(&app);
}

#[tauri::command]
fn set_island_collapsed(
    app: tauri::AppHandle,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    collapsed: bool,
) {
    native_window::clear_island_alert_state(&app, &tray_cache);
    if collapsed {
        tray_cache
            .last_collapse_unix_ms
            .store(clock::now_unix_ms(), Ordering::SeqCst);
    } else {
        tray_cache
            .island_peek_until_unix_ms
            .store(0, Ordering::SeqCst);
    }
    tray_cache
        .island_collapsed
        .store(collapsed, Ordering::SeqCst);
    let density = *preferences_state.layout_density.borrow();
    native_window::apply_layout_window_geometry(&app, density, collapsed);
}

#[tauri::command]
fn dismiss_dynamic_island(app: tauri::AppHandle) {
    native_window::hide_dynamic_island_window(&app);
}

#[tauri::command]
fn expand_island_on_display(
    app: tauri::AppHandle,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    display_index: usize,
    focus: Option<bool>,
) {
    let density = *preferences_state.layout_density.borrow();
    if density != LayoutDensity::DynamicIsland {
        return;
    }
    let display_mode = *preferences_state.dynamic_island_display.borrow();
    native_window::expand_island_on_display(
        &app,
        &tray_cache,
        display_mode,
        display_index,
        focus.unwrap_or(true),
    );
}

#[tauri::command]
async fn terminate_process(pid: u32) -> Result<(), String> {
    blocking_command(move || system_extras::terminate_user_process(pid)).await
}

#[tauri::command]
async fn inspect_process(pid: u32) -> Result<system_extras::ProcessInspection, String> {
    blocking_command(move || system_extras::inspect_process_details(pid)).await
}

#[tauri::command]
async fn cleanup_orphaned_servers() -> Result<u32, String> {
    blocking_command(system_extras::cleanup_orphaned_dev_servers).await
}

#[tauri::command]
async fn reveal_in_finder(path: String) -> Result<(), String> {
    blocking_command(move || system_extras::reveal_in_finder(&path)).await
}

#[tauri::command]
async fn fetch_listening_ports() -> Result<Vec<ports::ListeningPort>, String> {
    blocking_command(ports::list_listening_ports).await
}

#[tauri::command]
async fn fetch_awake_status(
    state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
) -> Result<awake::AwakeStatus, String> {
    let state = state.inner().clone();
    blocking_command(move || Ok(state.lock().unwrap().status())).await
}

pub(crate) fn apply_keep_awake_action(
    app: &tauri::AppHandle,
    state: &Mutex<awake::KeepAwakeManager>,
    enabled: bool,
    duration_seconds: Option<u64>,
) -> Result<awake::AwakeStatus, String> {
    let mut manager = state.lock().unwrap();
    manager.set_keep_awake(enabled, duration_seconds)?;
    let status = manager.status();
    drop(manager);
    tray_menu::sync_menu_awake(app, &status);
    native_window::trigger_island_alert(
        app,
        if status.keep_awake_active {
            "Keep Awake On"
        } else {
            "Keep Awake Off"
        },
        status.remaining_seconds.map(|s| format!("{}m", s.div_ceil(60))),
        if status.keep_awake_active {
            "good"
        } else {
            "info"
        },
        Some("awake"),
        native_window::ISLAND_PEEK_ORDINARY_MS,
    );
    Ok(status)
}

#[tauri::command]
async fn set_keep_awake(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
    enabled: bool,
    duration_seconds: Option<u64>,
) -> Result<awake::AwakeStatus, String> {
    let state = state.inner().clone();
    blocking_command(move || apply_keep_awake_action(&app, &state, enabled, duration_seconds)).await
}

#[tauri::command]
async fn set_keep_awake_rules(
    app: tauri::AppHandle,
    awake_state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    rules: awake::KeepAwakeRules,
) -> Result<awake::AwakeStatus, String> {
    let awake_state = awake_state.inner().clone();
    let preferences_state = preferences_state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || {
        apply_preference_action(
            &app,
            &preferences_state,
            &tray_cache,
            PreferenceAction::Set(ApplicationPreferenceChange::SetKeepAwakeRules { value: rules }),
        )?;
        Ok(awake_state.lock().unwrap().status())
    })
    .await
}

#[tauri::command]
async fn fetch_system_extras(
    app: tauri::AppHandle,
) -> Result<system_extras::SystemExtrasSnapshot, String> {
    let snap = tokio::task::spawn_blocking(system_extras::collect_system_extras)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("pill-extras-sync", &snap);
    Ok(snap)
}

#[tauri::command]
async fn fetch_pill_extras(
    app: tauri::AppHandle,
) -> Result<system_extras::SystemExtrasSnapshot, String> {
    let snap = tokio::task::spawn_blocking(system_extras::collect_pill_extras)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("pill-extras-sync", &snap);
    Ok(snap)
}

#[tauri::command]
async fn fetch_devices_snapshot() -> Result<system_extras::DevicesSnapshot, String> {
    tokio::task::spawn_blocking(system_extras::collect_devices_snapshot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_speed_test() -> Result<system_extras::SpeedTestResult, String> {
    tokio::task::spawn_blocking(system_extras::run_network_quality)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn update_audio_volume(
    output_volume: Option<u8>,
    input_volume: Option<u8>,
    muted: Option<bool>,
) -> Result<system_extras::AudioSnapshot, String> {
    tokio::task::spawn_blocking(move || {
        system_extras::set_audio_volume(output_volume, input_volume, muted)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clean_rebuildable_cache(
    cache_id: String,
) -> Result<Vec<system_extras::RebuildableCacheItem>, String> {
    tokio::task::spawn_blocking(move || system_extras::clean_rebuildable_cache(&cache_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn unload_ollama_model(name: String) -> Result<Vec<system_extras::OllamaModelItem>, String> {
    tokio::task::spawn_blocking(move || system_extras::unload_ollama_model(&name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn control_container(
    id: String,
    action: String,
) -> Result<Vec<system_extras::ContainerItem>, String> {
    tokio::task::spawn_blocking(move || system_extras::control_container(&id, &action))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_app_connections(
    process_name: String,
) -> Result<Vec<system_extras::AppConnectionItem>, String> {
    tokio::task::spawn_blocking(move || system_extras::list_app_connections(&process_name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn run_system_maintenance(action: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || system_extras::run_maintenance_action(&action))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) fn execute_quick_switch_action(
    app: &tauri::AppHandle,
    switch_id: &str,
) -> Result<(system_extras::QuickSwitchState, String), String> {
    let switch_label = system_extras::quick_switch_label(switch_id);
    let res = system_extras::run_quick_switch(switch_id)?;
    native_window::trigger_island_alert(
        app,
        switch_label,
        Some("Toggled".to_string()),
        "info",
        Some("system"),
        native_window::ISLAND_PEEK_ORDINARY_MS,
    );
    Ok(res)
}

#[tauri::command]
async fn run_quick_switch(
    app: tauri::AppHandle,
    switch_id: String,
) -> Result<(system_extras::QuickSwitchState, String), String> {
    blocking_command(move || execute_quick_switch_action(&app, &switch_id)).await
}

#[tauri::command]
async fn probe_http_ports(ports: Vec<u16>) -> Result<Vec<system_extras::PortHttpProbe>, String> {
    tokio::task::spawn_blocking(move || system_extras::probe_localhost_ports(&ports))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_disk_benchmark() -> Result<system_extras::DiskBenchmarkResult, String> {
    tokio::task::spawn_blocking(system_extras::run_nvme_benchmark)
        .await
        .map_err(|e| e.to_string())?
}

// The override lives entirely inside the policy state: it owns a token plus
// its targets while persisted settings stay untouched, so the selected preset
// resumes on the next fresh sample and survives a crash.
fn launch_fan_dust_blast(
    app: tauri::AppHandle,
    policy_arc: PolicyState,
    snapshot: HardwareTelemetrySnapshot,
) -> Result<(), String> {
    let blast_id = policy_arc.lock().unwrap().start_dust_blast(&snapshot)?;

    tauri::async_runtime::spawn(async move {
        for sec in (1..=15).rev() {
            // One short critical section decides the tick: emit this
            // countdown's seconds only while the policy still owns the token,
            // and emit the terminal 0 while the lock is held only when no
            // blast is active at all — a newer session is never zeroed.
            let still_active = {
                let policy = policy_arc.lock().unwrap();
                if policy.dust_blast_is_active(blast_id) {
                    let _ = app.emit("dust-blast-tick", sec);
                    true
                } else {
                    if !policy.has_active_blast() {
                        let _ = app.emit("dust-blast-tick", 0);
                    }
                    false
                }
            };
            if !still_active {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        // The final restore runs off the async executor under one policy
        // lock: finish only while our token is still the owner, emit 0 when
        // no blast is active (including one already cleared by a stale-data
        // recovery), and leave a newer blast untouched.
        let policy_arc = policy_arc.clone();
        let app = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let mut policy = policy_arc.lock().unwrap();
            if policy.dust_blast_is_active(blast_id) {
                let _ = policy.finish_dust_blast(blast_id);
                let _ = app.emit("dust-blast-tick", 0);
            } else if !policy.has_active_blast() {
                let _ = app.emit("dust-blast-tick", 0);
            }
        })
        .await;
    });

    Ok(())
}

pub(crate) fn start_dust_blast_action(
    app: &tauri::AppHandle,
    policy_arc: PolicyState,
    tray_cache: &TrayRuntimeCache,
) -> Result<(), String> {
    let snapshot = tray_cache
        .latest_snapshot
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Dust Blast requires hardware telemetry".to_string())?;
    launch_fan_dust_blast(app.clone(), policy_arc, snapshot)?;
    native_window::trigger_island_alert(
        app,
        "Fan Dust Blast",
        Some("100% RPM · 15s".to_string()),
        "warn",
        Some("fans"),
        native_window::ISLAND_PEEK_BLAST_MS,
    );
    Ok(())
}

#[tauri::command]
async fn start_fan_dust_blast(
    app: tauri::AppHandle,
    policy_state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
) -> Result<(), String> {
    let policy_arc = policy_state.inner().clone();
    let tray_cache = tray_cache.inner().clone();
    blocking_command(move || start_dust_blast_action(&app, policy_arc, &tray_cache)).await
}

#[tauri::command]
async fn cancel_fan_dust_blast(policy_state: tauri::State<'_, PolicyState>) -> Result<(), String> {
    let policy_arc = policy_state.inner().clone();
    blocking_command(move || policy_arc.lock().unwrap().cancel_dust_blast()).await
}

#[tauri::command]
async fn control_media_playback(
    action: String,
) -> Result<Option<system_extras::NowPlayingItem>, String> {
    blocking_command(move || system_extras::control_media_playback(&action)).await
}

#[tauri::command]
async fn trigger_custom_alert(
    app: tauri::AppHandle,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    title: String,
    detail: Option<String>,
    tone: String,
) -> Result<(), String> {
    let preferences_state = preferences_state.inner().clone();
    blocking_command(move || {
        let tone_static: &'static str = match tone.as_str() {
            "danger" => "danger",
            "warn" => "warn",
            "good" => "good",
            _ => "info",
        };
        let prefs = preferences_state.preferences.lock().unwrap().current();
        if prefs.layout_density == LayoutDensity::DynamicIsland {
            native_window::trigger_island_alert(
                &app,
                title.clone(),
                detail.clone(),
                tone_static,
                Some("awake"),
                native_window::ISLAND_PEEK_ALERT_MS,
            );
        }
        if prefs.alerts.enabled && prefs.alerts.native_notifications {
            alerts::send_macos_notification(&title, detail.as_deref());
        }
        Ok(())
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app_update::capture_launch_marker();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_positioner::init())
        .invoke_handler(tauri::generate_handler![
            fan_actuation_status,
            fetch_telemetry,
            thermal_policy_settings,
            select_thermal_policy_mode,
            upsert_thermal_rule,
            delete_thermal_rule,
            application_preferences,
            confirmed_application_preferences,
            update_application_preferences,
            export_configuration_bundle,
            import_configuration_bundle,
            set_fan_speed,
            set_fan_mode,
            install_fan_actuation_helper,
            toggle_popover,
            set_island_collapsed,
            dismiss_dynamic_island,
            expand_island_on_display,
            terminate_process,
            inspect_process,
            cleanup_orphaned_servers,
            reveal_in_finder,
            fetch_listening_ports,
            fetch_awake_status,
            set_keep_awake,
            set_keep_awake_rules,
            fetch_system_extras,
            fetch_pill_extras,
            fetch_devices_snapshot,
            run_speed_test,
            update_audio_volume,
            clean_rebuildable_cache,
            unload_ollama_model,
            control_container,
            fetch_app_connections,
            run_system_maintenance,
            run_quick_switch,
            probe_http_ports,
            run_disk_benchmark,
            start_fan_dust_blast,
            cancel_fan_dust_blast,
            control_media_playback,
            trigger_custom_alert,
            check_for_app_update,
            prepare_app_update,
            install_prepared_app_update,
            discard_prepared_app_update,
            confirm_update_startup
        ])
        .setup(move |_app| {
            #[cfg(target_os = "macos")]
            _app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let app_handle = _app.handle().clone();
            let login_item = MacLoginItem::running()?;
            let profile_files = ProfileFiles::new(
                _app.path()
                    .app_data_dir()
                    .map_err(|error| error.to_string())?,
            );
            profile_files
                .recover(&mut login_item.clone())
                .map_err(|error| format!("configuration import recovery failed: {error}"))?;
            _app.manage(profile_files.clone());
            let preferences =
                ApplicationPreferencesModule::load(profile_files.clone(), login_item.clone())?;
            let (telemetry_interval, mut telemetry_interval_updates) =
                tokio::sync::watch::channel(preferences.current().telemetry_interval_ms);
            let (layout_density, _) =
                tokio::sync::watch::channel(preferences.current().layout_density);
            let (dynamic_island_display, _) =
                tokio::sync::watch::channel(preferences.current().dynamic_island_display);
            let preferences_state = Arc::new(ApplicationPreferencesState {
                publication: Mutex::new(()),
                preferences: Mutex::new(preferences),
                telemetry_interval,
                layout_density,
                dynamic_island_display,
            });
            _app.manage(preferences_state.clone());
            _app.manage(login_item);

            let policy = ThermalPolicyState::load(profile_files, ProductionFanActuation);
            let policy_state = Arc::new(Mutex::new(policy));
            _app.manage(policy_state.clone());

            let tray_cache = Arc::new(TrayRuntimeCache::default());
            _app.manage(tray_cache.clone());

            let initial_prefs = preferences_state.preferences.lock().unwrap().current();
            let awake_state = Arc::new(Mutex::new(awake::KeepAwakeManager::with_rules(
                initial_prefs.keep_awake_rules,
            )));
            _app.manage(awake_state.clone());

            {
                // The will-sleep callback restores fans under one policy lock,
                // in the same order as any other forced restore.
                let policy = policy_state.clone();
                power_management::register_power_events(move || {
                    policy
                        .lock()
                        .map_err(|e| e.to_string())?
                        .force_restore_system_auto()
                });
            }

            let initial_policy_mode = policy_state.lock().unwrap().current().mode;
            let initial_awake = awake_state.lock().unwrap().status();
            if initial_prefs.keep_awake_rules.any_enabled() {
                let startup_awake = awake_state.clone();
                let startup_app = app_handle.clone();
                let startup_rules = initial_prefs.keep_awake_rules;
                tauri::async_runtime::spawn(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        if let Ok(status) =
                            awake::apply_rule_change(&startup_awake, |r| *r = startup_rules)
                        {
                            tray_menu::sync_menu_awake(&startup_app, &status);
                        }
                    })
                    .await;
                });
            }
            tray_cache
                .keep_awake_active
                .store(initial_awake.keep_awake_active, Ordering::SeqCst);
            native_window::apply_global_shortcut(&app_handle, initial_prefs.global_shortcut);
            tray_menu::install(_app, &initial_prefs, initial_policy_mode, &initial_awake)?;

            let initial_snapshot = telemetry_snapshot();
            let initial_history = tray_cache.record_snapshot(&initial_snapshot);
            tray_menu::update_tray_presentation(
                &app_handle,
                &initial_snapshot,
                &initial_prefs,
                &initial_history,
            );
            if initial_prefs.layout_density == LayoutDensity::DynamicIsland {
                tray_cache.island_collapsed.store(true, Ordering::SeqCst);
                native_window::sync_island_displays(
                    &app_handle,
                    &tray_cache,
                    LayoutDensity::DynamicIsland,
                    initial_prefs.dynamic_island_display,
                );
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                }
                let _ = app_handle.emit("island-collapsed-sync", true);
            } else {
                native_window::apply_layout_window_geometry(
                    &app_handle,
                    initial_prefs.layout_density,
                    false,
                );
            }

            let hover_app = app_handle.clone();
            let hover_layout = preferences_state.layout_density.subscribe();
            let hover_display_mode = preferences_state.dynamic_island_display.subscribe();
            let awake_state_loop = awake_state.clone();
            let cursor_cache = tray_cache.clone();
            tauri::async_runtime::spawn(async move {
                let mut tick_count: u64 = 0;
                loop {
                    application_preferences::cadence::wait_for_next_tick(
                        &mut telemetry_interval_updates,
                    )
                    .await;
                    if power_management::is_sleeping() {
                        continue;
                    }
                    let policy = policy_state.clone();
                    let prefs_state = preferences_state.clone();
                    let win_visible = app_handle
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    let current_layout = *prefs_state.layout_density.borrow();
                    let need_procs = win_visible
                        && !(current_layout == LayoutDensity::DynamicIsland
                            && tray_cache.island_collapsed.load(Ordering::Relaxed));
                    // Capture, the policy decision, and the committed-prefs
                    // snapshot run in one ordered blocking step; the possibly
                    // contended preferences mutex is never taken on the async
                    // executor, and no guard crosses an await.
                    let (data, current_prefs) = match tokio::task::spawn_blocking(move || {
                        let snapshot = tray_telemetry_snapshot(need_procs);
                        let current_prefs = prefs_state.preferences.lock().unwrap().current();
                        {
                            let mut policy_guard = policy.lock().unwrap();
                            policy_guard
                                .apply_auto_thermal_triggers(current_prefs.auto_thermal, &snapshot);
                            let _ = policy_guard.process_snapshot(&snapshot);
                        }
                        (snapshot, current_prefs)
                    })
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => continue,
                    };

                    tick_count = tick_count.wrapping_add(1);
                    let history = tray_cache.record_snapshot(&data);
                    tray_menu::update_tray_presentation(
                        &app_handle,
                        &data,
                        &current_prefs,
                        &history,
                    );

                    {
                        let cached_extras = system_extras::latest_cached_snapshot();
                        let fired = tray_cache.alerts.lock().unwrap().evaluate(
                            data.captured_at_unix_ms,
                            &current_prefs.alerts,
                            &data,
                            cached_extras.as_ref(),
                        );
                        for alert in fired {
                            if current_prefs.layout_density == LayoutDensity::DynamicIsland {
                                native_window::trigger_island_alert(
                                    &app_handle,
                                    alert.title.clone(),
                                    alert.detail.clone(),
                                    alert.tone,
                                    alert.tab,
                                    native_window::ISLAND_PEEK_ALERT_MS,
                                );
                            }
                            if current_prefs.alerts.native_notifications {
                                alerts::send_macos_notification(&alert.title, alert.detail.as_deref());
                            }
                        }
                    }

                    if tick_count.is_multiple_of(4) {
                        let rules = awake_state_loop.lock().unwrap().rules();
                        let has_rules = rules.any_enabled();
                        let was_active = tray_cache.keep_awake_active.load(Ordering::Relaxed);
                        let is_island =
                            current_prefs.layout_density == LayoutDensity::DynamicIsland;
                        if has_rules || was_active || is_island {
                            let awake_arc = awake_state_loop.clone();
                            let peek_app = app_handle.clone();
                            let peek_cache = tray_cache.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                if has_rules {
                                    let conditions = awake::RuleConditions::observe(&rules);
                                    let mut manager = awake_arc.lock().unwrap();
                                    // A rules change committed while the
                                    // probes ran must not be evaluated with
                                    // the stale observation.
                                    if manager.rules() == rules {
                                        let _ = manager.evaluate_rules(conditions);
                                    }
                                    let status = manager.status();
                                    drop(manager);
                                    tray_menu::sync_menu_awake(&peek_app, &status);
                                } else if was_active {
                                    let mut manager = awake_arc.lock().unwrap();
                                    if !manager.is_active_fast() {
                                        let status = manager.status();
                                        drop(manager);
                                        tray_menu::sync_menu_awake(&peek_app, &status);
                                    }
                                }
                                if is_island {
                                    let cam_or_mic = system_extras::is_camera_or_mic_active();
                                    let prev = peek_cache
                                        .prev_cam_or_mic_active
                                        .swap(cam_or_mic, Ordering::SeqCst);
                                    if !prev && cam_or_mic {
                                        native_window::trigger_island_alert(
                                            &peek_app,
                                            "Camera / Mic Active",
                                            Some("In use".to_string()),
                                            "warn",
                                            Some("volume"),
                                            native_window::ISLAND_PEEK_ALERT_MS,
                                        );
                                    }
                                }
                            })
                            .await;
                        }
                    }

                    // Emit telemetry to the frontend only when the window is visible
                    if app_handle
                        .get_webview_window("main")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false)
                    {
                        let _ = app_handle.emit("telemetry-update", &data);
                    }
                }
            });

            native_window::spawn_cursor_watcher(
                hover_app,
                hover_layout,
                hover_display_mode,
                cursor_cache,
            );

            // All fallible setup work is complete; combined with the UI mount
            // this releases the update handshake acknowledgement.
            app_update::native_ready();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn check_for_app_update() -> Result<Option<app_update::UpdateOffer>, String> {
    tokio::task::spawn_blocking(app_update::check_for_update)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn prepare_app_update(version: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || app_update::prepare(&version))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn discard_prepared_app_update(version: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || app_update::discard(&version))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn install_prepared_app_update(app: tauri::AppHandle, version: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || app_update::install(&version))
        .await
        .map_err(|error| error.to_string())??;
    app.exit(0);
    Ok(())
}
