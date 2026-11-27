// Tray menu construction, event handling, and menu/checkbox sync.
// Native window geometry and island actions live in native_window.rs; this
// module owns the AppKit-facing menu and the state that feeds it.

use crate::application_preferences::contract::{
    ApplicationPreferences, LayoutDensity, MenuBarStyle,
};
use crate::awake::{self, AwakeStatus};
use crate::hardware_telemetry::contract::{Availability, HardwareTelemetrySnapshot};
use crate::native_window::toggle_main_window_visibility;
use crate::thermal_policy::contract::{presets, ThermalPolicyMode};
use crate::thermal_policy::state::ThermalPolicyChange;
use crate::{
    menu_bar, power_management, system_extras, ApplicationPreferencesState, PolicyState,
    TrayRuntimeCache,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

const TRAY_ID: &str = "statpulse-tray";

pub(crate) const STYLES: &[(&str, &str, MenuBarStyle)] = &[
    (
        "style_compact_text",
        "Compact Temp",
        MenuBarStyle::CompactText,
    ),
    (
        "style_combined_temp_fan",
        "Temp + Fan RPM",
        MenuBarStyle::CombinedTempFan,
    ),
    (
        "style_combined_temp_power",
        "Temp + Power (W)",
        MenuBarStyle::CombinedTempPower,
    ),
    (
        "style_stacked_readout",
        "Stacked 2-Line",
        MenuBarStyle::StackedReadout,
    ),
    (
        "style_tachometer_gauge",
        "Tachometer",
        MenuBarStyle::TachometerGauge,
    ),
    (
        "style_sparkline_mini",
        "Live Sparkline",
        MenuBarStyle::SparklineMini,
    ),
    (
        "style_per_core_bars",
        "Per-Core Bars",
        MenuBarStyle::PerCoreBars,
    ),
    ("style_bar_chart", "Bar Chart", MenuBarStyle::BarChart),
    ("style_pie", "Pie Gauge", MenuBarStyle::Pie),
    ("style_ring", "Ring Progress", MenuBarStyle::Ring),
    ("style_meter", "Segmented Meter", MenuBarStyle::Meter),
    (
        "style_up_down_graph",
        "Up / Down Graph",
        MenuBarStyle::UpDownGraph,
    ),
    (
        "style_battery_icon",
        "Battery + Temp",
        MenuBarStyle::BatteryIcon,
    ),
];

pub(crate) struct TrayMenuHandles {
    status_item: MenuItem<tauri::Wry>,
    policy_auto: CheckMenuItem<tauri::Wry>,
    policy_quiet: CheckMenuItem<tauri::Wry>,
    policy_perf: CheckMenuItem<tauri::Wry>,
    policy_custom: CheckMenuItem<tauri::Wry>,
    layout_comfortable: CheckMenuItem<tauri::Wry>,
    layout_compact: CheckMenuItem<tauri::Wry>,
    layout_dynamic_island: CheckMenuItem<tauri::Wry>,
    style_items: Vec<(MenuBarStyle, CheckMenuItem<tauri::Wry>)>,
    traffic_light: CheckMenuItem<tauri::Wry>,
    awake_indefinite: CheckMenuItem<tauri::Wry>,
    awake_30m: CheckMenuItem<tauri::Wry>,
    awake_1h: CheckMenuItem<tauri::Wry>,
    awake_2h: CheckMenuItem<tauri::Wry>,
    awake_rule_ac: CheckMenuItem<tauri::Wry>,
    awake_rule_ports: CheckMenuItem<tauri::Wry>,
    awake_rule_ai: CheckMenuItem<tauri::Wry>,
}

fn format_tray_status_line(
    snapshot: &HardwareTelemetrySnapshot,
    preferences: &ApplicationPreferences,
    keep_awake_active: bool,
) -> String {
    let unit = preferences.temperature_unit;
    let fmt_t = |c: Option<f64>| crate::menu_bar::format_temperature(c, unit);
    let (cpu, gpu) = match &snapshot.temperatures {
        Availability::Available { value } => (fmt_t(value.cpu_celsius), fmt_t(value.gpu_celsius)),
        _ => (fmt_t(None), fmt_t(None)),
    };
    let fan_part = match &snapshot.fans {
        Availability::Available { value } if !value.is_empty() => {
            let max_rpm = value.iter().map(|f| f.speed_rpm).max().unwrap_or(0);
            if max_rpm == 0 {
                "Fan Silent".to_string()
            } else {
                format!("Fan {max_rpm} RPM")
            }
        }
        _ => "Fan --".to_string(),
    };
    let power_part = match &snapshot.battery {
        Availability::Available { value } => value
            .power_watts
            .map(|w| format!(" · {w:.0}W"))
            .unwrap_or_default(),
        _ => String::new(),
    };
    let awake_part = if keep_awake_active {
        " · ☕ Awake"
    } else {
        ""
    };
    format!("CPU {cpu} · GPU {gpu} · {fan_part}{power_part}{awake_part}")
}

pub(crate) fn sync_menu_policy(app: &tauri::AppHandle, mode: &ThermalPolicyMode) {
    let Some(handles) = app.try_state::<TrayMenuHandles>() else {
        return;
    };
    let _ = handles
        .policy_auto
        .set_checked(*mode == ThermalPolicyMode::SystemAuto);
    let _ = handles
        .policy_quiet
        .set_checked(*mode == ThermalPolicyMode::Quiet);
    let _ = handles
        .policy_perf
        .set_checked(*mode == ThermalPolicyMode::Performance);
    let _ = handles
        .policy_custom
        .set_checked(*mode == ThermalPolicyMode::Custom);
}

// Checkmark sync only: cadence/density watches publish after the store
// commits on the shared writer path.
pub(crate) fn sync_menu_prefs(app: &tauri::AppHandle, prefs: &ApplicationPreferences) {
    let Some(handles) = app.try_state::<TrayMenuHandles>() else {
        return;
    };
    let _ = handles
        .layout_comfortable
        .set_checked(prefs.layout_density == LayoutDensity::Comfortable);
    let _ = handles
        .layout_compact
        .set_checked(prefs.layout_density == LayoutDensity::Compact);
    let _ = handles
        .layout_dynamic_island
        .set_checked(prefs.layout_density == LayoutDensity::DynamicIsland);
    for (style, item) in &handles.style_items {
        let _ = item.set_checked(*style == prefs.menu_bar_style);
    }
    let _ = handles
        .traffic_light
        .set_checked(prefs.traffic_light_menu_bar);
}

pub(crate) fn sync_menu_awake(app: &tauri::AppHandle, status: &AwakeStatus) {
    if let Some(handles) = app.try_state::<TrayMenuHandles>() {
        let _ = handles.awake_indefinite.set_checked(status.is_indefinite());
        let _ = handles
            .awake_30m
            .set_checked(status.keep_awake_active && status.duration_seconds == Some(1800));
        let _ = handles
            .awake_1h
            .set_checked(status.keep_awake_active && status.duration_seconds == Some(3600));
        let _ = handles
            .awake_2h
            .set_checked(status.keep_awake_active && status.duration_seconds == Some(7200));
        let _ = handles.awake_rule_ac.set_checked(status.rules.while_on_ac);
        let _ = handles
            .awake_rule_ports
            .set_checked(status.rules.while_ports_active);
        let _ = handles
            .awake_rule_ai
            .set_checked(status.rules.while_ai_agents_active);
    }
    let _ = app.emit("awake-status-updated", status);
    if let Some(cache) = app.try_state::<Arc<TrayRuntimeCache>>() {
        let prev = cache
            .keep_awake_active
            .swap(status.keep_awake_active, Ordering::SeqCst);
        if prev != status.keep_awake_active {
            if let Some(prefs_state) = app.try_state::<Arc<ApplicationPreferencesState>>() {
                let prefs = prefs_state.preferences.lock().unwrap().current();
                refresh_tray_from_cache(app, &cache, &prefs);
            }
        }
    }
}

fn fnv1a_tray_icon(rgba: &[u8], width: u32, height: u32, is_template: bool) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in width
        .to_le_bytes()
        .into_iter()
        .chain(height.to_le_bytes())
        .chain([u8::from(is_template)])
    {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for &b in rgba {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 || hash == u64::MAX {
        1
    } else {
        hash
    }
}

pub(crate) fn update_tray_presentation(
    app: &tauri::AppHandle,
    snapshot: &HardwareTelemetrySnapshot,
    preferences: &ApplicationPreferences,
    history: &[f64],
) {
    let tray_cache = app.try_state::<Arc<TrayRuntimeCache>>();
    let keep_awake_active = tray_cache
        .as_ref()
        .map(|c| c.keep_awake_active.load(Ordering::Relaxed))
        .unwrap_or(false);
    let status_line = format_tray_status_line(snapshot, preferences, keep_awake_active);
    if let Some(handles) = app.try_state::<TrayMenuHandles>() {
        let should_update_status = tray_cache
            .as_ref()
            .and_then(|c| c.last_status_line.lock().ok())
            .map(|mut prev| {
                if prev.as_deref() == Some(status_line.as_str()) {
                    false
                } else {
                    *prev = Some(status_line.clone());
                    true
                }
            })
            .unwrap_or(true);
        if should_update_status {
            let _ = handles.status_item.set_text(status_line);
        }
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let presentation = menu_bar::render_menu_bar(snapshot, preferences, history);
    let tray_title = if keep_awake_active {
        if presentation.title.is_empty() {
            "☕".to_string()
        } else {
            format!("{} ☕", presentation.title)
        }
    } else {
        presentation.title
    };
    let should_update_title = tray_cache
        .as_ref()
        .and_then(|c| c.last_tray_title.lock().ok())
        .map(|mut prev| {
            if prev.as_deref() == Some(tray_title.as_str()) {
                false
            } else {
                *prev = Some(tray_title.clone());
                true
            }
        })
        .unwrap_or(true);
    if should_update_title {
        let _ = tray.set_title(Some(tray_title.as_str()));
    }
    if let Some((rgba, width, height)) = presentation.icon_rgba {
        let icon_hash = fnv1a_tray_icon(&rgba, width, height, presentation.icon_is_template);
        let changed = tray_cache
            .as_ref()
            .map(|c| c.last_tray_icon_hash.swap(icon_hash, Ordering::SeqCst) != icon_hash)
            .unwrap_or(true);
        if changed {
            let image = tauri::image::Image::new_owned(rgba, width, height);
            let _ = tray.set_icon_with_as_template(Some(image), presentation.icon_is_template);
        }
    } else {
        let changed = tray_cache
            .as_ref()
            .map(|c| c.last_tray_icon_hash.swap(u64::MAX, Ordering::SeqCst) != u64::MAX)
            .unwrap_or(true);
        if changed {
            let _ = tray.set_icon_with_as_template(None, false);
        }
    }
}

pub(crate) fn refresh_tray_from_cache(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    preferences: &ApplicationPreferences,
) {
    let snapshot = tray_cache
        .latest_snapshot
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(crate::telemetry_snapshot);
    let history = tray_cache.temp_history.lock().unwrap().clone();
    update_tray_presentation(app, &snapshot, preferences, &history);
}

pub(crate) fn install(
    app: &mut tauri::App,
    initial_prefs: &ApplicationPreferences,
    initial_policy_mode: ThermalPolicyMode,
    initial_awake: &AwakeStatus,
) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(
        app,
        "status_header",
        "CPU --°C · GPU --°C · Fan --",
        false,
        None::<&str>,
    )?;
    let show_i = MenuItem::with_id(app, "show", "Toggle StatPulse", true, None::<&str>)?;

    let policy_auto = CheckMenuItem::with_id(
        app,
        "policy_system_auto",
        "System Auto",
        true,
        initial_policy_mode == ThermalPolicyMode::SystemAuto,
        None::<&str>,
    )?;
    let policy_quiet = CheckMenuItem::with_id(
        app,
        "policy_quiet",
        format!(
            "Quiet ({}–{}°C)",
            presets().quiet.low_celsius as i64,
            presets().quiet.high_celsius as i64
        ),
        true,
        initial_policy_mode == ThermalPolicyMode::Quiet,
        None::<&str>,
    )?;
    let policy_perf = CheckMenuItem::with_id(
        app,
        "policy_performance",
        format!(
            "Performance ({}–{}°C)",
            presets().performance.low_celsius as i64,
            presets().performance.high_celsius as i64
        ),
        true,
        initial_policy_mode == ThermalPolicyMode::Performance,
        None::<&str>,
    )?;
    let policy_custom = CheckMenuItem::with_id(
        app,
        "policy_custom",
        "Custom Rules",
        true,
        initial_policy_mode == ThermalPolicyMode::Custom,
        None::<&str>,
    )?;
    let fan_dust_blast = MenuItem::with_id(
        app,
        "fan_dust_blast",
        "15s Fan Dust Blast (100% Max RPM)",
        true,
        None::<&str>,
    )?;
    let fan_dust_blast_stop = MenuItem::with_id(
        app,
        "fan_dust_blast_stop",
        "Stop Dust Blast",
        true,
        None::<&str>,
    )?;
    let thermal_sep = PredefinedMenuItem::separator(app)?;
    let thermal_submenu = Submenu::with_items(
        app,
        "Thermal · Fans",
        true,
        &[
            &policy_auto,
            &policy_quiet,
            &policy_perf,
            &policy_custom,
            &thermal_sep,
            &fan_dust_blast,
            &fan_dust_blast_stop,
        ],
    )?;

    let awake_indefinite = CheckMenuItem::with_id(
        app,
        "awake_indefinite",
        "Keep Awake: Indefinitely",
        true,
        initial_awake.is_indefinite(),
        None::<&str>,
    )?;
    let awake_30m = CheckMenuItem::with_id(
        app,
        "awake_30m",
        "Keep Awake: 30 Minutes",
        true,
        initial_awake.keep_awake_active && initial_awake.duration_seconds == Some(1800),
        None::<&str>,
    )?;
    let awake_1h = CheckMenuItem::with_id(
        app,
        "awake_1h",
        "Keep Awake: 1 Hour",
        true,
        initial_awake.keep_awake_active && initial_awake.duration_seconds == Some(3600),
        None::<&str>,
    )?;
    let awake_2h = CheckMenuItem::with_id(
        app,
        "awake_2h",
        "Keep Awake: 2 Hours",
        true,
        initial_awake.keep_awake_active && initial_awake.duration_seconds == Some(7200),
        None::<&str>,
    )?;
    let awake_off = MenuItem::with_id(app, "awake_off", "Disable Keep Awake", true, None::<&str>)?;
    let awake_sep = PredefinedMenuItem::separator(app)?;
    let awake_rule_ac = CheckMenuItem::with_id(
        app,
        "awake_rule_ac",
        "Auto: While on AC Power",
        true,
        initial_awake.rules.while_on_ac,
        None::<&str>,
    )?;
    let awake_rule_ports = CheckMenuItem::with_id(
        app,
        "awake_rule_ports",
        "Auto: While Dev Ports Open",
        true,
        initial_awake.rules.while_ports_active,
        None::<&str>,
    )?;
    let awake_rule_ai = CheckMenuItem::with_id(
        app,
        "awake_rule_ai",
        "Auto: While AI Agents Active",
        true,
        initial_awake.rules.while_ai_agents_active,
        None::<&str>,
    )?;
    let awake_submenu = Submenu::with_items(
        app,
        "Keep Awake (Caffeinate)",
        true,
        &[
            &awake_indefinite,
            &awake_30m,
            &awake_1h,
            &awake_2h,
            &awake_off,
            &awake_sep,
            &awake_rule_ac,
            &awake_rule_ports,
            &awake_rule_ai,
        ],
    )?;

    let switch_dark = MenuItem::with_id(
        app,
        "toggle_dark_mode",
        "Toggle macOS Dark Mode",
        true,
        None::<&str>,
    )?;
    let switch_hidden = MenuItem::with_id(
        app,
        "toggle_hidden_files",
        "Toggle Hidden Finder Files",
        true,
        None::<&str>,
    )?;
    let switch_desktop = MenuItem::with_id(
        app,
        "toggle_desktop_icons",
        "Toggle Desktop Icons",
        true,
        None::<&str>,
    )?;
    let switch_sep = PredefinedMenuItem::separator(app)?;
    let switch_clipboard = MenuItem::with_id(
        app,
        "clear_clipboard",
        "Clear Clipboard",
        true,
        None::<&str>,
    )?;
    let switch_trash = MenuItem::with_id(app, "empty_trash", "Empty Trash", true, None::<&str>)?;
    let switches_submenu = Submenu::with_items(
        app,
        "Quick Switches",
        true,
        &[
            &switch_dark,
            &switch_hidden,
            &switch_desktop,
            &switch_sep,
            &switch_clipboard,
            &switch_trash,
        ],
    )?;

    let maint_orphans = MenuItem::with_id(
        app,
        "maint_cleanup_orphans",
        "Clean Orphaned Dev Servers",
        true,
        None::<&str>,
    )?;
    let maint_dns = MenuItem::with_id(
        app,
        "maint_flush_dns",
        "Flush DNS Cache",
        true,
        None::<&str>,
    )?;
    let maint_ram = MenuItem::with_id(
        app,
        "maint_purge_ram",
        "Purge Inactive RAM",
        true,
        None::<&str>,
    )?;
    let maint_submenu = Submenu::with_items(
        app,
        "Developer · Maintenance",
        true,
        &[&maint_orphans, &maint_dns, &maint_ram],
    )?;

    let mut style_items = Vec::with_capacity(STYLES.len());
    for (id, label, variant) in STYLES {
        let item = CheckMenuItem::with_id(
            app,
            *id,
            *label,
            true,
            initial_prefs.menu_bar_style == *variant,
            None::<&str>,
        )?;
        style_items.push((*variant, item));
    }
    let traffic_light = CheckMenuItem::with_id(
        app,
        "toggle_traffic_light",
        "Traffic Light Coloring",
        true,
        initial_prefs.traffic_light_menu_bar,
        None::<&str>,
    )?;
    let layout_comfortable = CheckMenuItem::with_id(
        app,
        "layout_comfortable",
        "Layout: Comfortable",
        true,
        initial_prefs.layout_density == LayoutDensity::Comfortable,
        None::<&str>,
    )?;
    let layout_compact = CheckMenuItem::with_id(
        app,
        "layout_compact",
        "Layout: Compact",
        true,
        initial_prefs.layout_density == LayoutDensity::Compact,
        None::<&str>,
    )?;
    let layout_dynamic_island = CheckMenuItem::with_id(
        app,
        "layout_dynamic_island",
        "Layout: Dynamic Island",
        true,
        initial_prefs.layout_density == LayoutDensity::DynamicIsland,
        None::<&str>,
    )?;
    let layout_submenu = Submenu::with_items(
        app,
        "Layout · Mode",
        true,
        &[&layout_comfortable, &layout_compact, &layout_dynamic_island],
    )?;

    let style_sep = PredefinedMenuItem::separator(app)?;
    let mut menu_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = style_items
        .iter()
        .map(|(_, item)| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    menu_items.extend([
        &style_sep as &dyn tauri::menu::IsMenuItem<tauri::Wry>,
        &traffic_light,
    ]);
    let style_submenu = Submenu::with_items(app, "Menu Bar Style", true, &menu_items)?;

    let sep_1 = PredefinedMenuItem::separator(app)?;
    let sep_2 = PredefinedMenuItem::separator(app)?;
    let sep_3 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit StatPulse", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &sep_1,
            &show_i,
            &sep_2,
            &thermal_submenu,
            &awake_submenu,
            &switches_submenu,
            &maint_submenu,
            &layout_submenu,
            &style_submenu,
            &sep_3,
            &quit_i,
        ],
    )?;

    app.manage(TrayMenuHandles {
        status_item,
        policy_auto,
        policy_quiet,
        policy_perf,
        policy_custom,
        layout_comfortable,
        layout_compact,
        layout_dynamic_island,
        style_items,
        traffic_light,
        awake_indefinite,
        awake_30m,
        awake_1h,
        awake_2h,
        awake_rule_ac,
        awake_rule_ports,
        awake_rule_ai,
    });

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("StatPulse — macOS System, Developer & Thermal Monitor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            match &event {
                TrayIconEvent::Click { rect, .. }
                | TrayIconEvent::DoubleClick { rect, .. }
                | TrayIconEvent::Enter { rect, .. }
                | TrayIconEvent::Move { rect, .. } => {
                    let cache = tray.app_handle().state::<Arc<TrayRuntimeCache>>();
                    *cache.last_tray_rect.lock().unwrap() = Some(*rect);
                }
                _ => {}
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window_visibility(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            // Preference mutations share one writer path: state and app
            // handle are cloned only when an arm actually submits work, and
            // the publication lock is taken on the spawned worker, never on
            // the menu thread.
            let apply_preference = |action| {
                let preferences_state =
                    app.state::<Arc<ApplicationPreferencesState>>().inner().clone();
                let tray_cache = app.state::<Arc<TrayRuntimeCache>>().inner().clone();
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    let _ = crate::apply_preference_action(
                        &app_handle,
                        &preferences_state,
                        &tray_cache,
                        action,
                    );
                });
            };
            match id {
                "quit" => {
                    app.state::<Arc<Mutex<awake::KeepAwakeManager>>>()
                        .lock()
                        .unwrap()
                        .stop();
                    power_management::restore_system_auto();
                    app.exit(0);
                }
                "show" => {
                    toggle_main_window_visibility(app);
                }
                "policy_system_auto"
                | "policy_quiet"
                | "policy_performance"
                | "policy_custom" => {
                    let mode = match id {
                        "policy_system_auto" => ThermalPolicyMode::SystemAuto,
                        "policy_quiet" => ThermalPolicyMode::Quiet,
                        "policy_performance" => ThermalPolicyMode::Performance,
                        _ => ThermalPolicyMode::Custom,
                    };
                    let policy_state = app.state::<PolicyState>().inner().clone();
                    let tray_cache = app.state::<Arc<TrayRuntimeCache>>().inner().clone();
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let _ = crate::apply_thermal_policy_action(
                            &app_handle,
                            &policy_state,
                            &tray_cache,
                            ThermalPolicyChange::SelectMode(mode),
                            true,
                        );
                    });
                }
                "fan_dust_blast" => {
                    let policy_arc = app.state::<PolicyState>().inner().clone();
                    let tray_cache = app.state::<Arc<TrayRuntimeCache>>().inner().clone();
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let _ = crate::start_dust_blast_action(
                            &app_handle,
                            policy_arc,
                            &tray_cache,
                        );
                    });
                }
                "fan_dust_blast_stop" => {
                    let policy_arc = app.state::<PolicyState>().inner().clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let _ = policy_arc.lock().unwrap().cancel_dust_blast();
                    });
                }
                "awake_indefinite" | "awake_30m" | "awake_1h" | "awake_2h" | "awake_off" => {
                    // pmset control is blocking; keep it off the menu thread.
                    let state = app
                        .state::<Arc<Mutex<awake::KeepAwakeManager>>>()
                        .inner()
                        .clone();
                    let app_handle = app.clone();
                    let id_owned = id.to_string();
                    tauri::async_runtime::spawn_blocking(move || {
                        let (enabled, secs) = match id_owned.as_str() {
                            "awake_indefinite" => (!state.lock().unwrap().is_indefinite(), None),
                            "awake_30m" => (true, Some(1800)),
                            "awake_1h" => (true, Some(3600)),
                            "awake_2h" => (true, Some(7200)),
                            _ => (false, None),
                        };
                        let _ =
                            crate::apply_keep_awake_action(&app_handle, &state, enabled, secs);
                    });
                }
                "awake_rule_ac" | "awake_rule_ports" | "awake_rule_ai" => {
                    let field = match id {
                        "awake_rule_ac" => crate::KeepAwakeRuleField::OnAc,
                        "awake_rule_ports" => crate::KeepAwakeRuleField::Ports,
                        _ => crate::KeepAwakeRuleField::AiAgents,
                    };
                    apply_preference(crate::PreferenceAction::ToggleKeepAwakeRule(field));
                }
                "toggle_dark_mode"
                | "toggle_hidden_files"
                | "toggle_desktop_icons"
                | "clear_clipboard"
                | "empty_trash" => {
                    if let Some(switch_id) =
                        crate::application_preferences::contract::QuickSwitchId::from_id(id)
                    {
                        let app_handle = app.clone();
                        std::thread::spawn(move || {
                            let _ = crate::execute_quick_switch_action(&app_handle, switch_id);
                        });
                    }
                }
                "maint_cleanup_orphans" | "maint_flush_dns" | "maint_purge_ram" => {
                    let action = id.to_string();
                    std::thread::spawn(move || match action.as_str() {
                        "maint_cleanup_orphans" => {
                            let _ = system_extras::cleanup_orphaned_dev_servers();
                        }
                        "maint_flush_dns" => {
                            let _ = system_extras::run_maintenance_action("flush_dns");
                        }
                        "maint_purge_ram" => {
                            let _ = system_extras::run_maintenance_action("purge_ram");
                        }
                        _ => {}
                    });
                }
                "layout_comfortable" | "layout_compact" | "layout_dynamic_island" => {
                    let density = match id {
                        "layout_compact" => LayoutDensity::Compact,
                        "layout_dynamic_island" => LayoutDensity::DynamicIsland,
                        _ => LayoutDensity::Comfortable,
                    };
                    apply_preference(crate::PreferenceAction::ActivateLayout(density));
                }
                "toggle_traffic_light" => {
                    apply_preference(crate::PreferenceAction::ToggleTrafficLight);
                }
                _ => {
                    let Some((_, _, style)) = STYLES.iter().find(|(key, _, _)| *key == id)
                    else {
                        return;
                    };
                    let style = *style;
                    apply_preference(crate::PreferenceAction::Set(
                        crate::application_preferences::contract::ApplicationPreferenceChange::SetMenuBarStyle { value: style },
                    ));
                }
            }
        })
        .build(app)?;

    Ok(())
}
