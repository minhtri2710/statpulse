pub mod application_preferences;
pub mod awake;
pub mod fan_actuation;
pub mod hardware_telemetry;
pub mod mcp;
pub mod menu_bar;
pub mod ports;
pub mod power_management;
pub mod smc;
pub mod system_extras;
pub mod thermal_policy;

use application_preferences::adapters::{TauriAutostartAdapter, TauriPreferencesStore};
use application_preferences::contract::{
    ApplicationPreferenceChange, ApplicationPreferences, ConfigurationBundle, LayoutDensity,
    MenuBarStyle, TemperatureUnit,
};
use application_preferences::preferences::ApplicationPreferencesModule;
use fan_actuation::client::{self, ActuationStatus};
use hardware_telemetry::contract::{Availability, FanActuationStatus, HardwareTelemetrySnapshot};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use thermal_policy::adapters::{ProductionFanActuation, TauriSettingsStore};
use thermal_policy::contract::{ThermalPolicyMode, ThermalPolicySettings, ThermalRule};
use thermal_policy::state::{DirectFanActuationRequest, ThermalPolicyChange, ThermalPolicyState};

const MAX_TRAY_HISTORY_POINTS: usize = 24;

type PreferencesModule = ApplicationPreferencesModule<
    TauriPreferencesStore<tauri::Wry>,
    TauriAutostartAdapter<tauri::Wry>,
>;

struct ApplicationPreferencesState {
    preferences: Mutex<PreferencesModule>,
    telemetry_interval: tokio::sync::watch::Sender<u32>,
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
    island_pinned: AtomicBool,
    last_collapse_unix_ms: AtomicU64,
    island_peek_until_unix_ms: AtomicU64,
    last_temp_peek_unix_ms: AtomicU64,
    prev_cam_or_mic_active: AtomicBool,
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

type PolicyModule = ThermalPolicyState<TauriSettingsStore<tauri::Wry>, ProductionFanActuation>;
type PolicyState = Arc<Mutex<PolicyModule>>;

struct TrayMenuHandles {
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
    awake_rule_ac: CheckMenuItem<tauri::Wry>,
    awake_rule_ports: CheckMenuItem<tauri::Wry>,
    awake_rule_ai: CheckMenuItem<tauri::Wry>,
}

fn format_tray_status_line(
    snapshot: &HardwareTelemetrySnapshot,
    preferences: &ApplicationPreferences,
) -> String {
    let unit_suffix = match preferences.temperature_unit {
        TemperatureUnit::Celsius => "°C",
        TemperatureUnit::Fahrenheit => "°F",
    };
    let fmt_t = |c: Option<f64>| match c {
        Some(val) => {
            let disp = match preferences.temperature_unit {
                TemperatureUnit::Celsius => val,
                TemperatureUnit::Fahrenheit => val * 9.0 / 5.0 + 32.0,
            };
            format!("{disp:.0}{unit_suffix}")
        }
        None => format!("--{unit_suffix}"),
    };
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
            .map(|w| format!(" · {w:.1}W"))
            .unwrap_or_default(),
        _ => String::new(),
    };
    format!("CPU {cpu} · GPU {gpu} · {fan_part}{power_part}")
}

fn sync_tray_menu_policy(app: &tauri::AppHandle, mode: &ThermalPolicyMode) {
    let Some(handles) = app.try_state::<TrayMenuHandles>() else {
        return;
    };
    let _ = handles.policy_auto.set_checked(*mode == ThermalPolicyMode::SystemAuto);
    let _ = handles.policy_quiet.set_checked(*mode == ThermalPolicyMode::Quiet);
    let _ = handles.policy_perf.set_checked(*mode == ThermalPolicyMode::Performance);
    let _ = handles.policy_custom.set_checked(*mode == ThermalPolicyMode::Custom);
}

fn sync_tray_menu_prefs(app: &tauri::AppHandle, prefs: &ApplicationPreferences) {
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
    let _ = handles.traffic_light.set_checked(prefs.traffic_light_menu_bar);
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacNsPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacNsSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacNsRect {
    origin: MacNsPoint,
    size: MacNsSize,
}

#[cfg(target_os = "macos")]
fn set_macos_window_notch_level(window: &tauri::WebviewWindow, is_dynamic_island: bool) {
    use std::ffi::{c_char, c_void};
    #[link(name = "objc")]
    extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend();
    }
    let level: isize = if is_dynamic_island { 26 } else { 3 };
    let has_shadow: i8 = if is_dynamic_island { 0 } else { 1 };
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ns_win) = w.ns_window() else {
            return;
        };
        unsafe {
            let sel = sel_registerName(b"setLevel:\0".as_ptr() as *const c_char);
            let func: extern "C" fn(*mut c_void, *const c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            func(ns_win, sel, level);

            let sel_opaque = sel_registerName(b"setOpaque:\0".as_ptr() as *const c_char);
            let fn_opaque: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_opaque(ns_win, sel_opaque, 0);

            let sel_shadow = sel_registerName(b"setHasShadow:\0".as_ptr() as *const c_char);
            let fn_shadow: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_shadow(ns_win, sel_shadow, has_shadow);
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn set_macos_window_notch_level(_window: &tauri::WebviewWindow, _is_dynamic_island: bool) {}

#[cfg(target_os = "macos")]
fn macos_global_cursor_points() -> Option<(f64, f64)> {
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreate(source: *const c_void) -> *const c_void;
        fn CGEventGetLocation(event: *const c_void) -> MacNsPoint;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let pt = CGEventGetLocation(event);
        CFRelease(event);
        Some((pt.x, pt.y))
    }
}

#[cfg(not(target_os = "macos"))]
fn macos_global_cursor_points() -> Option<(f64, f64)> {
    None
}

#[cfg(target_os = "macos")]
fn apply_macos_top_center_notch_frame(window: &tauri::WebviewWindow, width: f64, height: f64) {
    use std::ffi::{c_char, c_void};
    #[link(name = "objc")]
    extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend();
    }
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten());
    let (mon_x_pt, mon_w_pt, mon_h_pt) = match monitor {
        Some(m) => {
            let p = m.position();
            let s = m.size();
            let scale = m.scale_factor().max(1.0);
            (
                p.x as f64 / scale,
                s.width as f64 / scale,
                s.height as f64 / scale,
            )
        }
        None => (0.0, 1728.0, 1117.0),
    };
    let target_x = mon_x_pt + (mon_w_pt - width) / 2.0;
    // Cocoa screen origin (0, 0) is bottom-left of primary screen, so Y = mon_h_pt - height anchors flush at top bezel (CG Y = 0).
    let target_y = mon_h_pt - height;
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ns_win) = w.ns_window() else {
            return;
        };
        unsafe {
            let sel_level = sel_registerName(b"setLevel:\0".as_ptr() as *const c_char);
            let fn_level: extern "C" fn(*mut c_void, *const c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_level(ns_win, sel_level, 26);

            // CanJoinAllSpaces (1) | FullScreenAuxiliary (256)
            let sel_beh = sel_registerName(b"setCollectionBehavior:\0".as_ptr() as *const c_char);
            let fn_beh: extern "C" fn(*mut c_void, *const c_void, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_beh(ns_win, sel_beh, (1 << 0) | (1 << 8));

            let sel_opaque = sel_registerName(b"setOpaque:\0".as_ptr() as *const c_char);
            let fn_opaque: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_opaque(ns_win, sel_opaque, 0);

            // Disable AppKit rectangular window shadow so curved CSS shadow renders cleanly
            let sel_shadow = sel_registerName(b"setHasShadow:\0".as_ptr() as *const c_char);
            let fn_shadow: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_shadow(ns_win, sel_shadow, 0);

            let sel_frame = sel_registerName(b"setFrame:display:\0".as_ptr() as *const c_char);
            let fn_frame: extern "C" fn(*mut c_void, *const c_void, MacNsRect, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            let rect = MacNsRect {
                origin: MacNsPoint {
                    x: target_x,
                    y: target_y,
                },
                size: MacNsSize { width, height },
            };
            fn_frame(ns_win, sel_frame, rect, 1);
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn apply_macos_top_center_notch_frame(window: &tauri::WebviewWindow, width: f64, height: f64) {
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten());
    if let Some(m) = monitor {
        let p = m.position();
        let s = m.size();
        let scale = m.scale_factor().max(1.0);
        let target_x = (p.x as f64 + (s.width as f64 - width * scale) / 2.0).round() as i32;
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            target_x,
            p.y,
        )));
    }
}

fn position_window_below_tray_or_mouse(
    window: &tauri::WebviewWindow,
    tray_cache: &TrayRuntimeCache,
    expected_logical_width: Option<f64>,
) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let (mon_x, mon_y, mon_w, scale) = match monitor {
        Some(m) => {
            let p = m.position();
            let s = m.size();
            (p.x as f64, p.y as f64, s.width as f64, m.scale_factor().max(1.0))
        }
        None => (0.0, 0.0, 1440.0, 2.0),
    };
    let mon_right = mon_x + mon_w;
    let win_w = expected_logical_width
        .map(|w| w * scale)
        .unwrap_or_else(|| {
            window
                .outer_size()
                .map(|s| s.width as f64)
                .unwrap_or(420.0 * scale)
        });
    let last_rect = tray_cache.last_tray_rect.lock().unwrap().clone();
    let cursor = window.cursor_position().ok();

    let (anchor_x, anchor_y) = match (cursor, last_rect) {
        (Some(c), Some(rect)) if c.y <= mon_y + 90.0 * scale => {
            let r_pos = rect.position.to_physical::<f64>(scale);
            let r_size = rect.size.to_physical::<f64>(scale);
            (c.x, r_pos.y + r_size.height + 4.0 * scale)
        }
        (Some(c), None) if c.y <= mon_y + 90.0 * scale => (c.x, mon_y + 34.0 * scale),
        (_, Some(rect)) => {
            let r_pos = rect.position.to_physical::<f64>(scale);
            let r_size = rect.size.to_physical::<f64>(scale);
            (
                r_pos.x + r_size.width / 2.0,
                r_pos.y + r_size.height + 4.0 * scale,
            )
        }
        (Some(c), None) => (c.x, mon_y + 34.0 * scale),
        (None, None) => (mon_right - win_w / 2.0 - 24.0 * scale, mon_y + 34.0 * scale),
    };

    let min_x = mon_x + 8.0 * scale;
    let max_x = (mon_right - win_w - 8.0 * scale).max(min_x);
    let target_x = (anchor_x - win_w / 2.0).clamp(min_x, max_x);
    let target_y = anchor_y.max(mon_y + 28.0 * scale);
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        target_x.round() as i32,
        target_y.round() as i32,
    )));
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn trigger_island_peek(app: &tauri::AppHandle, duration_ms: u64, _collapsed_pill: bool) {
    let Some(prefs_state) = app.try_state::<Arc<ApplicationPreferencesState>>() else {
        return;
    };
    let density = prefs_state.preferences.lock().unwrap().current().layout_density;
    if density != LayoutDensity::DynamicIsland {
        return;
    }
    let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() else {
        return;
    };
    tray_cache
        .island_peek_until_unix_ms
        .store(now_unix_ms().saturating_add(duration_ms), Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let is_collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
        let is_visible = window.is_visible().unwrap_or(false);
        if is_collapsed || !is_visible {
            tray_cache.island_collapsed.store(false, Ordering::SeqCst);
            apply_layout_window_geometry(app, LayoutDensity::DynamicIsland, false);
            let _ = window.show();
            let _ = app.emit("island-collapsed-sync", false);
            emit_cached_telemetry(app, &tray_cache);
        }
    }
}

fn hide_dynamic_island_window(app: &tauri::AppHandle) {
    if let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() {
        tray_cache
            .last_collapse_unix_ms
            .store(now_unix_ms(), Ordering::SeqCst);
        tray_cache
            .island_peek_until_unix_ms
            .store(0, Ordering::SeqCst);
        tray_cache.island_pinned.store(false, Ordering::SeqCst);
        tray_cache.island_collapsed.store(true, Ordering::SeqCst);
    }
    apply_layout_window_geometry(app, LayoutDensity::DynamicIsland, true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }
    let _ = app.emit("island-collapsed-sync", true);
}

fn apply_layout_window_geometry(
    app: &tauri::AppHandle,
    density: LayoutDensity,
    island_collapsed: bool,
) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let is_island = density == LayoutDensity::DynamicIsland;
    let (width, height) = match (density, island_collapsed) {
        (LayoutDensity::DynamicIsland, true) => (488.0_f64, 32.0_f64),
        (LayoutDensity::DynamicIsland, false) => (560.0_f64, 680.0_f64),
        (LayoutDensity::Compact, _) => (410.0_f64, 640.0_f64),
        (LayoutDensity::Comfortable, _) => (420.0_f64, 700.0_f64),
    };
    if is_island {
        apply_macos_top_center_notch_frame(&window, width, height);
    } else {
        set_macos_window_notch_level(&window, false);
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
        let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
        position_window_below_tray_or_mouse(&window, &tray_cache, Some(width));
    }
}

fn sync_tray_menu_awake(app: &tauri::AppHandle, status: &awake::AwakeStatus) {
    let Some(handles) = app.try_state::<TrayMenuHandles>() else {
        return;
    };
    let _ = handles
        .awake_indefinite
        .set_checked(status.keep_awake_active && status.remaining_seconds.is_none());
    let _ = handles.awake_rule_ac.set_checked(status.rules.while_on_ac);
    let _ = handles.awake_rule_ports.set_checked(status.rules.while_ports_active);
    let _ = handles
        .awake_rule_ai
        .set_checked(status.rules.while_ai_agents_active);
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

fn update_tray_presentation(
    app: &tauri::AppHandle,
    snapshot: &HardwareTelemetrySnapshot,
    preferences: &ApplicationPreferences,
    history: &[f64],
) {
    let tray_cache = app.try_state::<Arc<TrayRuntimeCache>>();
    let status_line = format_tray_status_line(snapshot, preferences);
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
    let Some(tray) = app.tray_by_id("superfan-tray") else {
        return;
    };
    let presentation = menu_bar::render_menu_bar(snapshot, preferences, history);
    let should_update_title = tray_cache
        .as_ref()
        .and_then(|c| c.last_tray_title.lock().ok())
        .map(|mut prev| {
            if prev.as_deref() == Some(presentation.title.as_str()) {
                false
            } else {
                *prev = Some(presentation.title.clone());
                true
            }
        })
        .unwrap_or(true);
    if should_update_title {
        let _ = tray.set_title(Some(presentation.title.as_str()));
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

fn refresh_tray_from_cache(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    preferences: &ApplicationPreferences,
) {
    let snapshot = tray_cache
        .latest_snapshot
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(telemetry_snapshot);
    let history = tray_cache.temp_history.lock().unwrap().clone();
    update_tray_presentation(app, &snapshot, preferences, &history);
}

static FAN_ACTUATION_STATUS_CACHE: std::sync::OnceLock<
    Mutex<Option<(std::time::Instant, ActuationStatus)>>,
> = std::sync::OnceLock::new();

fn cached_fan_actuation_status() -> ActuationStatus {
    let cache = FAN_ACTUATION_STATUS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((ts, ref cached)) = *guard {
            if ts.elapsed() < std::time::Duration::from_secs(5) {
                return cached.clone();
            }
        }
    }
    let fresh = client::status();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((std::time::Instant::now(), fresh.clone()));
    }
    fresh
}

#[tauri::command]
fn fan_actuation_status() -> ActuationStatus {
    let fresh = client::status();
    let cache = FAN_ACTUATION_STATUS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((std::time::Instant::now(), fresh.clone()));
    }
    fresh
}

fn telemetry_snapshot() -> HardwareTelemetrySnapshot {
    let fan_actuation_status = match cached_fan_actuation_status() {
        ActuationStatus::NotRegistered => FanActuationStatus::NotRegistered,
        ActuationStatus::Ready => FanActuationStatus::Ready,
        ActuationStatus::Unavailable => FanActuationStatus::Unavailable,
    };
    let captured_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    hardware_telemetry::capture(fan_actuation_status, captured_at_unix_ms)
}

#[tauri::command]
async fn fetch_telemetry() -> Result<HardwareTelemetrySnapshot, String> {
    tokio::task::spawn_blocking(telemetry_snapshot)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn application_preferences(
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
) -> ApplicationPreferences {
    state.preferences.lock().unwrap().current()
}

#[tauri::command]
fn update_application_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    change: ApplicationPreferenceChange,
) -> Result<ApplicationPreferences, String> {
    let mut preferences = state.preferences.lock().unwrap();
    let previous = preferences.current();
    let updated = preferences.update(change)?;
    drop(preferences);

    if updated.telemetry_interval_ms != previous.telemetry_interval_ms {
        state
            .telemetry_interval
            .send_replace(updated.telemetry_interval_ms);
    }
    if updated.layout_density != previous.layout_density {
        tray_cache.island_pinned.store(false, Ordering::SeqCst);
        tray_cache.island_collapsed.store(false, Ordering::SeqCst);
        if updated.layout_density == LayoutDensity::DynamicIsland {
            tray_cache
                .island_peek_until_unix_ms
                .store(now_unix_ms().saturating_add(3_800), Ordering::SeqCst);
        } else {
            tray_cache
                .island_peek_until_unix_ms
                .store(0, Ordering::SeqCst);
        }
        apply_layout_window_geometry(&app, updated.layout_density, false);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
        }
        let _ = app.emit("island-collapsed-sync", false);
    }
    refresh_tray_from_cache(&app, &tray_cache, &updated);
    sync_tray_menu_prefs(&app, &updated);
    Ok(updated)
}

#[tauri::command]
fn export_configuration_bundle(
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    policy_state: tauri::State<'_, PolicyState>,
) -> ConfigurationBundle {
    ConfigurationBundle {
        version: 1,
        preferences: preferences_state.preferences.lock().unwrap().current(),
        thermal_policy: policy_state.lock().unwrap().current(),
    }
}

#[tauri::command]
fn import_configuration_bundle(
    app: tauri::AppHandle,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    policy_state: tauri::State<'_, PolicyState>,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    bundle: ConfigurationBundle,
) -> Result<ConfigurationBundle, String> {
    if bundle.version != 1 {
        return Err(format!(
            "Unsupported configuration bundle version: {}",
            bundle.version
        ));
    }
    application_preferences::preferences::validate(&bundle.preferences)?;
    thermal_policy::settings::validate(&bundle.thermal_policy)?;

    let updated_policy = policy_state
        .lock()
        .unwrap()
        .update(ThermalPolicyChange::Replace(bundle.thermal_policy))?;

    let mut preferences = preferences_state.preferences.lock().unwrap();
    let previous = preferences.current();
    let updated_prefs = preferences.replace(bundle.preferences)?;
    drop(preferences);

    if updated_prefs.telemetry_interval_ms != previous.telemetry_interval_ms {
        preferences_state
            .telemetry_interval
            .send_replace(updated_prefs.telemetry_interval_ms);
    }
    if updated_prefs.layout_density != previous.layout_density {
        tray_cache.island_pinned.store(false, Ordering::SeqCst);
        tray_cache.island_collapsed.store(false, Ordering::SeqCst);
        if updated_prefs.layout_density == LayoutDensity::DynamicIsland {
            tray_cache
                .island_peek_until_unix_ms
                .store(now_unix_ms().saturating_add(3_800), Ordering::SeqCst);
        } else {
            tray_cache
                .island_peek_until_unix_ms
                .store(0, Ordering::SeqCst);
        }
        apply_layout_window_geometry(&app, updated_prefs.layout_density, false);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
        }
        let _ = app.emit("island-collapsed-sync", false);
    }

    refresh_tray_from_cache(&app, &tray_cache, &updated_prefs);
    sync_tray_menu_prefs(&app, &updated_prefs);
    sync_tray_menu_policy(&app, &updated_policy.mode);
    let _ = app.emit("preferences-updated", &updated_prefs);
    let _ = app.emit("thermal-policy-updated", &updated_policy);

    Ok(ConfigurationBundle {
        version: 1,
        preferences: updated_prefs,
        thermal_policy: updated_policy,
    })
}

#[tauri::command]
fn thermal_policy_settings(state: tauri::State<'_, PolicyState>) -> ThermalPolicySettings {
    state.lock().unwrap().current()
}

#[tauri::command]
fn select_thermal_policy_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    mode: ThermalPolicyMode,
) -> Result<ThermalPolicySettings, String> {
    let updated = state
        .lock()
        .unwrap()
        .update(ThermalPolicyChange::SelectMode(mode))?;
    sync_tray_menu_policy(&app, &updated.mode);
    trigger_island_peek(&app, 3_800, true);
    Ok(updated)
}

#[tauri::command]
fn upsert_thermal_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    rule: ThermalRule,
) -> Result<ThermalPolicySettings, String> {
    let updated = state
        .lock()
        .unwrap()
        .update(ThermalPolicyChange::UpsertRule(rule))?;
    sync_tray_menu_policy(&app, &updated.mode);
    Ok(updated)
}

#[tauri::command]
fn delete_thermal_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, PolicyState>,
    rule_id: String,
) -> Result<ThermalPolicySettings, String> {
    let updated = state
        .lock()
        .unwrap()
        .update(ThermalPolicyChange::DeleteRule(rule_id))?;
    sync_tray_menu_policy(&app, &updated.mode);
    Ok(updated)
}

#[tauri::command]
fn set_fan_speed(
    state: tauri::State<'_, PolicyState>,
    fan_id: usize,
    rpm: i32,
) -> Result<(), String> {
    state
        .lock()
        .unwrap()
        .direct_actuation(DirectFanActuationRequest::Target { fan_id, rpm })
}

#[tauri::command]
fn set_fan_mode(
    state: tauri::State<'_, PolicyState>,
    fan_id: usize,
    mode: String,
    rpm: Option<i32>,
) -> Result<(), String> {
    let request = match mode.as_str() {
        "auto" => DirectFanActuationRequest::SystemAuto { fan_id },
        "manual" => DirectFanActuationRequest::Target {
            fan_id,
            rpm: rpm.ok_or_else(|| "manual mode requires a target RPM".to_string())?,
        },
        _ => return Err("fan mode must be auto or manual".into()),
    };
    state.lock().unwrap().direct_actuation(request)
}

#[tauri::command]
fn install_fan_actuation_helper(app: tauri::AppHandle) -> Result<ActuationStatus, String> {
    let resource_directory = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("fan-actuation");
    fan_actuation::installer::install(&resource_directory)?;
    Ok(fan_actuation_status())
}

fn emit_cached_telemetry(app: &tauri::AppHandle, tray_cache: &TrayRuntimeCache) {
    if let Some(snap) = tray_cache.latest_snapshot.lock().unwrap().as_ref() {
        let _ = app.emit("telemetry-update", snap);
    }
}

fn toggle_main_window_visibility(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
    let prefs_state = app.state::<Arc<ApplicationPreferencesState>>();
    let density = prefs_state.preferences.lock().unwrap().current().layout_density;
    if density == LayoutDensity::DynamicIsland {
        let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
        if is_visible && !collapsed {
            hide_dynamic_island_window(app);
        } else {
            tray_cache.island_collapsed.store(false, Ordering::SeqCst);
            tray_cache
                .island_peek_until_unix_ms
                .store(now_unix_ms().saturating_add(4_500), Ordering::SeqCst);
            apply_layout_window_geometry(app, density, false);
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit("island-collapsed-sync", false);
            emit_cached_telemetry(app, &tray_cache);
        }
    } else if is_visible {
        let _ = window.hide();
    } else {
        let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
        apply_layout_window_geometry(app, density, collapsed);
        let _ = window.show();
        let _ = window.set_focus();
        emit_cached_telemetry(app, &tray_cache);
    }
}

#[tauri::command]
fn toggle_popover(app: tauri::AppHandle) {
    toggle_main_window_visibility(&app);
}

#[tauri::command]
fn set_island_collapsed(
    app: tauri::AppHandle,
    tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>,
    preferences_state: tauri::State<'_, Arc<ApplicationPreferencesState>>,
    collapsed: bool,
) {
    if collapsed {
        tray_cache
            .last_collapse_unix_ms
            .store(now_unix_ms(), Ordering::SeqCst);
        tray_cache.island_pinned.store(false, Ordering::SeqCst);
    } else {
        tray_cache
            .island_peek_until_unix_ms
            .store(0, Ordering::SeqCst);
    }
    tray_cache.island_collapsed.store(collapsed, Ordering::SeqCst);
    let density = preferences_state.preferences.lock().unwrap().current().layout_density;
    apply_layout_window_geometry(&app, density, collapsed);
}

#[tauri::command]
fn set_island_pinned(tray_cache: tauri::State<'_, Arc<TrayRuntimeCache>>, pinned: bool) {
    tray_cache.island_pinned.store(pinned, Ordering::SeqCst);
}

#[tauri::command]
fn dismiss_dynamic_island(app: tauri::AppHandle) {
    hide_dynamic_island_window(&app);
}

#[tauri::command]
fn terminate_process(pid: u32) -> Result<(), String> {
    system_extras::terminate_user_process(pid)
}

#[tauri::command]
async fn inspect_process(pid: u32) -> Result<system_extras::ProcessInspection, String> {
    tokio::task::spawn_blocking(move || system_extras::inspect_process_details(pid))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn cleanup_orphaned_servers() -> Result<u32, String> {
    tokio::task::spawn_blocking(system_extras::cleanup_orphaned_dev_servers)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if !target.exists() {
        return Err(format!("Path does not exist: {path}"));
    }
    let mut cmd = std::process::Command::new("/usr/bin/open");
    if target.is_file() {
        cmd.arg("-R").arg(&path);
    } else {
        cmd.arg(&path);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to reveal in Finder: {e}"))?;
    if !status.success() {
        return Err("Finder failed to open path.".into());
    }
    Ok(())
}

#[tauri::command]
async fn fetch_listening_ports() -> Result<Vec<ports::ListeningPort>, String> {
    tokio::task::spawn_blocking(ports::list_listening_ports)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_awake_status(
    state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
) -> Result<awake::AwakeStatus, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.lock().unwrap().status())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_keep_awake(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
    enabled: bool,
    duration_seconds: Option<u64>,
) -> Result<awake::AwakeStatus, String> {
    let mut manager = state.lock().unwrap();
    manager.set_keep_awake(enabled, duration_seconds)?;
    let status = manager.status();
    drop(manager);
    sync_tray_menu_awake(&app, &status);
    trigger_island_peek(&app, 3_800, true);
    Ok(status)
}

#[tauri::command]
async fn set_keep_awake_rules(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<Mutex<awake::KeepAwakeManager>>>,
    rules: awake::KeepAwakeRules,
) -> Result<awake::AwakeStatus, String> {
    let state = state.inner().clone();
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let on_ac = if rules.while_on_ac {
            awake::is_ac_power_connected()
        } else {
            false
        };
        let active_ports = if rules.while_ports_active {
            ports::list_listening_ports()
                .map(|list| list.into_iter().filter(|p| (1024..=10000).contains(&p.port)).count())
                .unwrap_or(0)
        } else {
            0
        };
        let ai_active = if rules.while_ai_agents_active {
            !system_extras::collect_system_extras().agents.is_empty()
        } else {
            false
        };
        let mut manager = state.lock().unwrap();
        manager.set_rules(rules, on_ac, active_ports, ai_active)?;
        let status = manager.status();
        drop(manager);
        sync_tray_menu_awake(&app_handle, &status);
        Ok(status)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_system_extras() -> Result<system_extras::SystemExtrasSnapshot, String> {
    tokio::task::spawn_blocking(system_extras::collect_system_extras)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn fetch_pill_extras() -> Result<system_extras::SystemExtrasSnapshot, String> {
    tokio::task::spawn_blocking(system_extras::collect_pill_extras)
        .await
        .map_err(|e| e.to_string())
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

#[tauri::command]
async fn run_quick_switch(
    app: tauri::AppHandle,
    switch_id: String,
) -> Result<(system_extras::QuickSwitchState, String), String> {
    let res = tokio::task::spawn_blocking(move || system_extras::run_quick_switch(&switch_id))
        .await
        .map_err(|e| e.to_string())??;
    trigger_island_peek(&app, 3_800, true);
    Ok(res)
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

static DUST_BLAST_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn launch_fan_dust_blast(app: tauri::AppHandle, policy_arc: PolicyState) -> Result<(), String> {
    if DUST_BLAST_ACTIVE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("15s Dust Blast is already running.".into());
    }
    let prev_mode = policy_arc.lock().unwrap().current().mode;
    if prev_mode != ThermalPolicyMode::SystemAuto {
        let _ = policy_arc
            .lock()
            .unwrap()
            .update(ThermalPolicyChange::SelectMode(ThermalPolicyMode::SystemAuto));
        sync_tray_menu_policy(&app, &ThermalPolicyMode::SystemAuto);
    }

    let snap = telemetry_snapshot();
    let fan_targets: Vec<(usize, i32)> = match snap.fans {
        Availability::Available { value } => value
            .into_iter()
            .filter_map(|f| Some((f.id, f.max_speed_rpm?)))
            .collect(),
        _ => Vec::new(),
    };
    if fan_targets.is_empty() {
        DUST_BLAST_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        return Err("No controllable fans detected for Dust Blast.".into());
    }

    tauri::async_runtime::spawn(async move {
        for sec in (1..=15).rev() {
            if !DUST_BLAST_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            for &(fan_id, max_rpm) in &fan_targets {
                let _ = policy_arc
                    .lock()
                    .unwrap()
                    .direct_actuation(DirectFanActuationRequest::Target {
                        fan_id,
                        rpm: max_rpm,
                    });
            }
            let _ = app.emit("dust-blast-tick", sec);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        DUST_BLAST_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        for &(fan_id, _) in &fan_targets {
            let _ = policy_arc
                .lock()
                .unwrap()
                .direct_actuation(DirectFanActuationRequest::SystemAuto { fan_id });
        }
        if prev_mode != ThermalPolicyMode::SystemAuto {
            let updated = policy_arc
                .lock()
                .unwrap()
                .update(ThermalPolicyChange::SelectMode(prev_mode));
            if let Ok(settings) = updated {
                sync_tray_menu_policy(&app, &settings.mode);
                let _ = app.emit("thermal-policy-updated", &settings);
            }
        }
        let _ = app.emit("dust-blast-tick", 0);
    });

    Ok(())
}

#[tauri::command]
async fn start_fan_dust_blast(
    app: tauri::AppHandle,
    policy_state: tauri::State<'_, PolicyState>,
) -> Result<(), String> {
    let peek_app = app.clone();
    launch_fan_dust_blast(app, policy_state.inner().clone())?;
    trigger_island_peek(&peek_app, 16_000, true);
    Ok(())
}

#[tauri::command]
fn cancel_fan_dust_blast() {
    DUST_BLAST_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::AppleScript,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            fan_actuation_status,
            fetch_telemetry,
            thermal_policy_settings,
            select_thermal_policy_mode,
            upsert_thermal_rule,
            delete_thermal_rule,
            application_preferences,
            update_application_preferences,
            export_configuration_bundle,
            import_configuration_bundle,
            set_fan_speed,
            set_fan_mode,
            install_fan_actuation_helper,
            toggle_popover,
            set_island_collapsed,
            set_island_pinned,
            dismiss_dynamic_island,
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
            prepare_app_update,
            install_prepared_app_update
        ])
        .setup(move |_app| {
            #[cfg(target_os = "macos")]
            _app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let app_handle = _app.handle().clone();
            let preferences = ApplicationPreferencesModule::load(
                TauriPreferencesStore::new(app_handle.clone()),
                TauriAutostartAdapter::new(app_handle.clone()),
            )?;
            let (telemetry_interval, mut telemetry_interval_updates) =
                tokio::sync::watch::channel(preferences.current().telemetry_interval_ms);
            let preferences_state = Arc::new(ApplicationPreferencesState {
                preferences: Mutex::new(preferences),
                telemetry_interval,
            });
            _app.manage(preferences_state.clone());

            let policy = ThermalPolicyState::load(
                TauriSettingsStore::new(app_handle.clone()),
                ProductionFanActuation,
            );
            let policy_state = Arc::new(Mutex::new(policy));
            _app.manage(policy_state.clone());

            let tray_cache = Arc::new(TrayRuntimeCache::default());
            _app.manage(tray_cache.clone());

            let awake_state = Arc::new(Mutex::new(awake::KeepAwakeManager::default()));
            _app.manage(awake_state.clone());

            power_management::register_power_events(policy_state.clone());

            // Setup Tray Icon + Quick Action Menu
            let initial_policy_mode = policy_state.lock().unwrap().current().mode;
            let initial_prefs = preferences_state.preferences.lock().unwrap().current();
            let initial_awake = awake_state.lock().unwrap().status();

            let status_item = MenuItem::with_id(
                _app,
                "status_header",
                "CPU --°C · GPU --°C · Fan --",
                false,
                None::<&str>,
            )?;
            let show_i = MenuItem::with_id(_app, "show", "Toggle StatPulse", true, None::<&str>)?;

            let policy_auto = CheckMenuItem::with_id(
                _app,
                "policy_system_auto",
                "System Auto",
                true,
                initial_policy_mode == ThermalPolicyMode::SystemAuto,
                None::<&str>,
            )?;
            let policy_quiet = CheckMenuItem::with_id(
                _app,
                "policy_quiet",
                "Quiet (50–85°C)",
                true,
                initial_policy_mode == ThermalPolicyMode::Quiet,
                None::<&str>,
            )?;
            let policy_perf = CheckMenuItem::with_id(
                _app,
                "policy_performance",
                "Performance (40–75°C)",
                true,
                initial_policy_mode == ThermalPolicyMode::Performance,
                None::<&str>,
            )?;
            let policy_custom = CheckMenuItem::with_id(
                _app,
                "policy_custom",
                "Custom Rules",
                true,
                initial_policy_mode == ThermalPolicyMode::Custom,
                None::<&str>,
            )?;
            let fan_dust_blast = MenuItem::with_id(
                _app,
                "fan_dust_blast",
                "15s Fan Dust Blast (100% Max RPM)",
                true,
                None::<&str>,
            )?;
            let fan_dust_blast_stop = MenuItem::with_id(
                _app,
                "fan_dust_blast_stop",
                "Stop Dust Blast",
                true,
                None::<&str>,
            )?;
            let thermal_sep = PredefinedMenuItem::separator(_app)?;
            let thermal_submenu = Submenu::with_items(
                _app,
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
                _app,
                "awake_indefinite",
                "Keep Awake: Indefinitely",
                true,
                initial_awake.keep_awake_active && initial_awake.remaining_seconds.is_none(),
                None::<&str>,
            )?;
            let awake_30m =
                MenuItem::with_id(_app, "awake_30m", "Keep Awake: 30 Minutes", true, None::<&str>)?;
            let awake_1h =
                MenuItem::with_id(_app, "awake_1h", "Keep Awake: 1 Hour", true, None::<&str>)?;
            let awake_2h =
                MenuItem::with_id(_app, "awake_2h", "Keep Awake: 2 Hours", true, None::<&str>)?;
            let awake_off =
                MenuItem::with_id(_app, "awake_off", "Disable Keep Awake", true, None::<&str>)?;
            let awake_sep = PredefinedMenuItem::separator(_app)?;
            let awake_rule_ac = CheckMenuItem::with_id(
                _app,
                "awake_rule_ac",
                "Auto: While on AC Power",
                true,
                initial_awake.rules.while_on_ac,
                None::<&str>,
            )?;
            let awake_rule_ports = CheckMenuItem::with_id(
                _app,
                "awake_rule_ports",
                "Auto: While Dev Ports Open",
                true,
                initial_awake.rules.while_ports_active,
                None::<&str>,
            )?;
            let awake_rule_ai = CheckMenuItem::with_id(
                _app,
                "awake_rule_ai",
                "Auto: While AI Agents Active",
                true,
                initial_awake.rules.while_ai_agents_active,
                None::<&str>,
            )?;
            let awake_submenu = Submenu::with_items(
                _app,
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
                _app,
                "switch_dark_mode",
                "Toggle macOS Dark Mode",
                true,
                None::<&str>,
            )?;
            let switch_hidden = MenuItem::with_id(
                _app,
                "switch_hidden_files",
                "Toggle Hidden Finder Files",
                true,
                None::<&str>,
            )?;
            let switch_desktop = MenuItem::with_id(
                _app,
                "switch_desktop_icons",
                "Toggle Desktop Icons",
                true,
                None::<&str>,
            )?;
            let switch_sep = PredefinedMenuItem::separator(_app)?;
            let switch_clipboard = MenuItem::with_id(
                _app,
                "switch_clear_clipboard",
                "Clear Clipboard",
                true,
                None::<&str>,
            )?;
            let switch_trash =
                MenuItem::with_id(_app, "switch_empty_trash", "Empty Trash", true, None::<&str>)?;
            let switches_submenu = Submenu::with_items(
                _app,
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
                _app,
                "maint_cleanup_orphans",
                "Clean Orphaned Dev Servers",
                true,
                None::<&str>,
            )?;
            let maint_dns =
                MenuItem::with_id(_app, "maint_flush_dns", "Flush DNS Cache", true, None::<&str>)?;
            let maint_ram = MenuItem::with_id(
                _app,
                "maint_purge_ram",
                "Purge Inactive RAM",
                true,
                None::<&str>,
            )?;
            let maint_submenu = Submenu::with_items(
                _app,
                "Developer · Maintenance",
                true,
                &[&maint_orphans, &maint_dns, &maint_ram],
            )?;

            let style_specs = [
                ("style_compact_text", "Compact Temp", MenuBarStyle::CompactText),
                ("style_combined_temp_fan", "Temp + Fan RPM", MenuBarStyle::CombinedTempFan),
                ("style_combined_temp_power", "Temp + Power (W)", MenuBarStyle::CombinedTempPower),
                ("style_stacked_readout", "Stacked 2-Line", MenuBarStyle::StackedReadout),
                ("style_tachometer_gauge", "Tachometer", MenuBarStyle::TachometerGauge),
                ("style_sparkline_mini", "Live Sparkline", MenuBarStyle::SparklineMini),
                ("style_per_core_bars", "Per-Core Bars", MenuBarStyle::PerCoreBars),
                ("style_bar_chart", "Bar Chart", MenuBarStyle::BarChart),
                ("style_pie", "Pie Gauge", MenuBarStyle::Pie),
                ("style_ring", "Ring Progress", MenuBarStyle::Ring),
                ("style_meter", "Segmented Meter", MenuBarStyle::Meter),
                ("style_up_down_graph", "Up / Down Graph", MenuBarStyle::UpDownGraph),
                ("style_battery_icon", "Battery + Temp", MenuBarStyle::BatteryIcon),
            ];
            let mut style_items = Vec::with_capacity(style_specs.len());
            for (id, label, variant) in style_specs {
                let item = CheckMenuItem::with_id(
                    _app,
                    id,
                    label,
                    true,
                    initial_prefs.menu_bar_style == variant,
                    None::<&str>,
                )?;
                style_items.push((variant, item));
            }
            let traffic_light = CheckMenuItem::with_id(
                _app,
                "toggle_traffic_light",
                "Traffic Light Coloring",
                true,
                initial_prefs.traffic_light_menu_bar,
                None::<&str>,
            )?;
            let layout_comfortable = CheckMenuItem::with_id(
                _app,
                "layout_comfortable",
                "Layout: Comfortable",
                true,
                initial_prefs.layout_density == LayoutDensity::Comfortable,
                None::<&str>,
            )?;
            let layout_compact = CheckMenuItem::with_id(
                _app,
                "layout_compact",
                "Layout: Compact",
                true,
                initial_prefs.layout_density == LayoutDensity::Compact,
                None::<&str>,
            )?;
            let layout_dynamic_island = CheckMenuItem::with_id(
                _app,
                "layout_dynamic_island",
                "Layout: Dynamic Island",
                true,
                initial_prefs.layout_density == LayoutDensity::DynamicIsland,
                None::<&str>,
            )?;
            let layout_submenu = Submenu::with_items(
                _app,
                "Layout · Mode",
                true,
                &[&layout_comfortable, &layout_compact, &layout_dynamic_island],
            )?;

            let style_sep = PredefinedMenuItem::separator(_app)?;
            let style_submenu = Submenu::with_items(
                _app,
                "Menu Bar Style",
                true,
                &[
                    &style_items[0].1,
                    &style_items[1].1,
                    &style_items[2].1,
                    &style_items[3].1,
                    &style_items[4].1,
                    &style_items[5].1,
                    &style_items[6].1,
                    &style_items[7].1,
                    &style_items[8].1,
                    &style_items[9].1,
                    &style_items[10].1,
                    &style_items[11].1,
                    &style_items[12].1,
                    &style_sep,
                    &traffic_light,
                ],
            )?;

            let sep_1 = PredefinedMenuItem::separator(_app)?;
            let sep_2 = PredefinedMenuItem::separator(_app)?;
            let sep_3 = PredefinedMenuItem::separator(_app)?;
            let quit_i = MenuItem::with_id(_app, "quit", "Quit StatPulse", true, None::<&str>)?;

            let menu = Menu::with_items(
                _app,
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

            _app.manage(TrayMenuHandles {
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
                awake_rule_ac,
                awake_rule_ports,
                awake_rule_ai,
            });

            let _tray = TrayIconBuilder::with_id("superfan-tray")
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
                            let policy_state = app.state::<PolicyState>();
                            let updated = policy_state
                                .lock()
                                .unwrap()
                                .update(ThermalPolicyChange::SelectMode(mode));
                            if let Ok(settings) = updated {
                                let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
                                if let Some(snapshot) =
                                    tray_cache.latest_snapshot.lock().unwrap().clone()
                                {
                                    let _ = policy_state
                                        .lock()
                                        .unwrap()
                                        .process_snapshot(&snapshot, snapshot.captured_at_unix_ms);
                                }
                                sync_tray_menu_policy(app, &settings.mode);
                                trigger_island_peek(app, 3_800, true);
                                let _ = app.emit("thermal-policy-updated", &settings);
                            }
                        }
                        "fan_dust_blast" => {
                            let policy_arc = app.state::<PolicyState>().inner().clone();
                            if launch_fan_dust_blast(app.clone(), policy_arc).is_ok() {
                                trigger_island_peek(app, 16_000, true);
                            }
                        }
                        "fan_dust_blast_stop" => {
                            DUST_BLAST_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                        "awake_indefinite" | "awake_30m" | "awake_1h" | "awake_2h" | "awake_off" => {
                            let (enabled, secs) = match id {
                                "awake_indefinite" => {
                                    let currently = app
                                        .state::<Arc<Mutex<awake::KeepAwakeManager>>>()
                                        .lock()
                                        .unwrap()
                                        .status();
                                    let already_indef =
                                        currently.keep_awake_active && currently.remaining_seconds.is_none();
                                    (!already_indef, None)
                                }
                                "awake_30m" => (true, Some(1800)),
                                "awake_1h" => (true, Some(3600)),
                                "awake_2h" => (true, Some(7200)),
                                _ => (false, None),
                            };
                            let state = app.state::<Arc<Mutex<awake::KeepAwakeManager>>>();
                            let mut manager = state.lock().unwrap();
                            if manager.set_keep_awake(enabled, secs).is_ok() {
                                let status = manager.status();
                                drop(manager);
                                sync_tray_menu_awake(app, &status);
                                trigger_island_peek(app, 3_800, true);
                            }
                        }
                        "awake_rule_ac" | "awake_rule_ports" | "awake_rule_ai" => {
                            let app_handle = app.clone();
                            let id_owned = id.to_string();
                            let state = app.state::<Arc<Mutex<awake::KeepAwakeManager>>>().inner().clone();
                            std::thread::spawn(move || {
                                let mut rules = state.lock().unwrap().status().rules;
                                match id_owned.as_str() {
                                    "awake_rule_ac" => rules.while_on_ac = !rules.while_on_ac,
                                    "awake_rule_ports" => {
                                        rules.while_ports_active = !rules.while_ports_active
                                    }
                                    "awake_rule_ai" => {
                                        rules.while_ai_agents_active = !rules.while_ai_agents_active
                                    }
                                    _ => {}
                                }
                                let on_ac = if rules.while_on_ac {
                                    awake::is_ac_power_connected()
                                } else {
                                    false
                                };
                                let dev_ports = if rules.while_ports_active {
                                    ports::list_listening_ports()
                                        .map(|l| l.into_iter().filter(|p| (1024..=10000).contains(&p.port)).count())
                                        .unwrap_or(0)
                                } else {
                                    0
                                };
                                let ai_active = if rules.while_ai_agents_active {
                                    !system_extras::collect_system_extras().agents.is_empty()
                                } else {
                                    false
                                };
                                let mut manager = state.lock().unwrap();
                                if manager.set_rules(rules, on_ac, dev_ports, ai_active).is_ok() {
                                    let status = manager.status();
                                    drop(manager);
                                    sync_tray_menu_awake(&app_handle, &status);
                                }
                            });
                        }
                        "switch_dark_mode"
                        | "switch_hidden_files"
                        | "switch_desktop_icons"
                        | "switch_clear_clipboard"
                        | "switch_empty_trash" => {
                            let switch_id = match id {
                                "switch_dark_mode" => "toggle_dark_mode",
                                "switch_hidden_files" => "toggle_hidden_files",
                                "switch_desktop_icons" => "toggle_desktop_icons",
                                "switch_clear_clipboard" => "clear_clipboard",
                                _ => "empty_trash",
                            };
                            let app_handle = app.clone();
                            std::thread::spawn(move || {
                                let _ = system_extras::run_quick_switch(switch_id);
                                trigger_island_peek(&app_handle, 3_800, true);
                            });
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
                            let preferences_state =
                                app.state::<Arc<ApplicationPreferencesState>>();
                            let updated = preferences_state
                                .preferences
                                .lock()
                                .unwrap()
                                .update(ApplicationPreferenceChange::SetLayoutDensity { value: density });
                            if let Ok(prefs) = updated {
                                let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
                                tray_cache.island_pinned.store(false, Ordering::SeqCst);
                                tray_cache.island_collapsed.store(false, Ordering::SeqCst);
                                if prefs.layout_density == LayoutDensity::DynamicIsland {
                                    tray_cache
                                        .island_peek_until_unix_ms
                                        .store(now_unix_ms().saturating_add(3_800), Ordering::SeqCst);
                                } else {
                                    tray_cache
                                        .island_peek_until_unix_ms
                                        .store(0, Ordering::SeqCst);
                                }
                                refresh_tray_from_cache(app, &tray_cache, &prefs);
                                sync_tray_menu_prefs(app, &prefs);
                                apply_layout_window_geometry(app, prefs.layout_density, false);
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                                let _ = app.emit("island-collapsed-sync", false);
                                let _ = app.emit("preferences-updated", &prefs);
                            }
                        }
                        "toggle_traffic_light" => {
                            let preferences_state =
                                app.state::<Arc<ApplicationPreferencesState>>();
                            let mut prefs_guard = preferences_state.preferences.lock().unwrap();
                            let next_val = !prefs_guard.current().traffic_light_menu_bar;
                            let updated = prefs_guard.update(
                                ApplicationPreferenceChange::SetTrafficLightMenuBar { value: next_val },
                            );
                            drop(prefs_guard);
                            if let Ok(prefs) = updated {
                                let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
                                refresh_tray_from_cache(app, &tray_cache, &prefs);
                                sync_tray_menu_prefs(app, &prefs);
                                let _ = app.emit("preferences-updated", &prefs);
                            }
                        }
                        "style_compact_text"
                        | "style_combined_temp_fan"
                        | "style_combined_temp_power"
                        | "style_stacked_readout"
                        | "style_tachometer_gauge"
                        | "style_sparkline_mini"
                        | "style_per_core_bars"
                        | "style_bar_chart"
                        | "style_pie"
                        | "style_ring"
                        | "style_meter"
                        | "style_up_down_graph"
                        | "style_battery_icon" => {
                            let style = match id {
                                "style_compact_text" => MenuBarStyle::CompactText,
                                "style_combined_temp_fan" => MenuBarStyle::CombinedTempFan,
                                "style_combined_temp_power" => MenuBarStyle::CombinedTempPower,
                                "style_stacked_readout" => MenuBarStyle::StackedReadout,
                                "style_tachometer_gauge" => MenuBarStyle::TachometerGauge,
                                "style_sparkline_mini" => MenuBarStyle::SparklineMini,
                                "style_per_core_bars" => MenuBarStyle::PerCoreBars,
                                "style_bar_chart" => MenuBarStyle::BarChart,
                                "style_pie" => MenuBarStyle::Pie,
                                "style_ring" => MenuBarStyle::Ring,
                                "style_meter" => MenuBarStyle::Meter,
                                "style_up_down_graph" => MenuBarStyle::UpDownGraph,
                                _ => MenuBarStyle::BatteryIcon,
                            };
                            let preferences_state =
                                app.state::<Arc<ApplicationPreferencesState>>();
                            let updated = preferences_state
                                .preferences
                                .lock()
                                .unwrap()
                                .update(ApplicationPreferenceChange::SetMenuBarStyle { value: style });
                            if let Ok(prefs) = updated {
                                let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
                                refresh_tray_from_cache(app, &tray_cache, &prefs);
                                sync_tray_menu_prefs(app, &prefs);
                                let _ = app.emit("preferences-updated", &prefs);
                            }
                        }
                        _ => {}
                    }
                })
                .build(_app)?;

            let initial_snapshot = telemetry_snapshot();
            let initial_history = tray_cache.record_snapshot(&initial_snapshot);
            update_tray_presentation(&app_handle, &initial_snapshot, &initial_prefs, &initial_history);
            if initial_prefs.layout_density == LayoutDensity::DynamicIsland {
                tray_cache.island_collapsed.store(true, Ordering::SeqCst);
                apply_layout_window_geometry(&app_handle, LayoutDensity::DynamicIsland, true);
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                }
                let _ = app_handle.emit("island-collapsed-sync", true);
            } else {
                apply_layout_window_geometry(&app_handle, initial_prefs.layout_density, false);
            }

            let hover_app = app_handle.clone();
            let hover_prefs = preferences_state.clone();
            let hover_cache = tray_cache.clone();
            let awake_state_loop = awake_state.clone();
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
                    let data = match tokio::task::spawn_blocking(telemetry_snapshot).await {
                        Ok(snap) => snap,
                        Err(_) => continue,
                    };
                    let now_unix_ms = data.captured_at_unix_ms;
                    let _ = policy_state
                        .lock()
                        .unwrap()
                        .process_snapshot(&data, now_unix_ms);

                    tick_count = tick_count.wrapping_add(1);
                    let history = tray_cache.record_snapshot(&data);
                    let current_prefs = preferences_state.preferences.lock().unwrap().current();
                    update_tray_presentation(&app_handle, &data, &current_prefs, &history);

                    if current_prefs.layout_density == LayoutDensity::DynamicIsland {
                        if let Availability::Available { value } = &data.temperatures {
                            if value.cpu_celsius.is_some_and(|c| c >= 82.0) {
                                let now = data.captured_at_unix_ms;
                                let last = tray_cache.last_temp_peek_unix_ms.load(Ordering::SeqCst);
                                if now.saturating_sub(last) > 45_000 {
                                    tray_cache
                                        .last_temp_peek_unix_ms
                                        .store(now, Ordering::SeqCst);
                                    trigger_island_peek(&app_handle, 4_500, true);
                                }
                            }
                        }
                    }

                    if tick_count % 4 == 0 {
                        let rules = awake_state_loop.lock().unwrap().status().rules;
                        let is_island = current_prefs.layout_density == LayoutDensity::DynamicIsland;
                        if rules.while_on_ac
                            || rules.while_ports_active
                            || rules.while_ai_agents_active
                            || is_island
                        {
                            let awake_arc = awake_state_loop.clone();
                            let peek_app = app_handle.clone();
                            let peek_cache = tray_cache.clone();
                            let _ = tokio::task::spawn_blocking(move || {
                                if rules.while_on_ac
                                    || rules.while_ports_active
                                    || rules.while_ai_agents_active
                                {
                                    let on_ac = if rules.while_on_ac {
                                        awake::is_ac_power_connected()
                                    } else {
                                        false
                                    };
                                    let dev_ports = if rules.while_ports_active {
                                        ports::list_listening_ports()
                                            .map(|l| {
                                                l.into_iter()
                                                    .filter(|p| (1024..=10000).contains(&p.port))
                                                    .count()
                                            })
                                            .unwrap_or(0)
                                    } else {
                                        0
                                    };
                                    let ai_active = if rules.while_ai_agents_active {
                                        !system_extras::collect_system_extras().agents.is_empty()
                                    } else {
                                        false
                                    };
                                    let mut manager = awake_arc.lock().unwrap();
                                    let _ = manager.evaluate_rules(on_ac, dev_ports, ai_active);
                                    let status = manager.status();
                                    drop(manager);
                                    sync_tray_menu_awake(&peek_app, &status);
                                }
                                if is_island {
                                    let cam_or_mic = system_extras::is_camera_or_mic_active();
                                    let prev = peek_cache
                                        .prev_cam_or_mic_active
                                        .swap(cam_or_mic, Ordering::SeqCst);
                                    if !prev && cam_or_mic {
                                        trigger_island_peek(&peek_app, 4_500, true);
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

            // Global cursor tracker for Dynamic Island dormant-in-notch wake & hover retract
            tauri::async_runtime::spawn(async move {
                let mut inside_streak: u8 = 0;
                let mut outside_streak: u8 = 0;
                let mut cached_monitor: Option<(std::time::Instant, (f64, f64, f64, f64))> = None;
                let mut cached_visible: Option<(std::time::Instant, bool)> = None;
                let mut sleep_ms: u64 = 120;
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
                    let density = hover_prefs.preferences.lock().unwrap().current().layout_density;
                    if density != LayoutDensity::DynamicIsland {
                        inside_streak = 0;
                        outside_streak = 0;
                        cached_visible = None;
                        sleep_ms = 250;
                        continue;
                    }
                    let Some(window) = hover_app.get_webview_window("main") else {
                        sleep_ms = 180;
                        continue;
                    };
                    let (mon_x_pt, mon_y_pt, mon_w_pt, scale) = match cached_monitor {
                        Some((ts, bounds)) if ts.elapsed() < std::time::Duration::from_secs(10) => {
                            bounds
                        }
                        _ => {
                            let monitor = window
                                .primary_monitor()
                                .ok()
                                .flatten()
                                .or_else(|| window.current_monitor().ok().flatten());
                            let bounds = match monitor {
                                Some(m) => {
                                    let p = m.position();
                                    let s = m.size();
                                    let sf = m.scale_factor().max(1.0);
                                    (p.x as f64 / sf, p.y as f64 / sf, s.width as f64 / sf, sf)
                                }
                                None => (0.0, 0.0, 1728.0, 2.0),
                            };
                            cached_monitor = Some((std::time::Instant::now(), bounds));
                            bounds
                        }
                    };
                    let (cx, cy) = if let Some((px, py)) = macos_global_cursor_points() {
                        (px, py)
                    } else if let Ok(cursor) = window.cursor_position() {
                        (cursor.x / scale, cursor.y / scale)
                    } else {
                        sleep_ms = 180;
                        continue;
                    };
                    let mon_center_x = mon_x_pt + mon_w_pt / 2.0;

                    let is_visible = match cached_visible {
                        Some((ts, vis)) if vis && ts.elapsed() < std::time::Duration::from_secs(5) => {
                            vis
                        }
                        _ => {
                            let vis = window.is_visible().unwrap_or(false);
                            cached_visible = Some((std::time::Instant::now(), vis));
                            vis
                        }
                    };
                    if !is_visible {
                        hover_cache.island_collapsed.store(true, Ordering::SeqCst);
                        apply_layout_window_geometry(
                            &hover_app,
                            LayoutDensity::DynamicIsland,
                            true,
                        );
                        let _ = window.show();
                        cached_visible = Some((std::time::Instant::now(), true));
                        let _ = hover_app.emit("island-collapsed-sync", true);
                        emit_cached_telemetry(&hover_app, &hover_cache);
                    }
                    let collapsed = hover_cache.island_collapsed.load(Ordering::SeqCst);
                    sleep_ms = if collapsed && cy > mon_y_pt + 120.0 {
                        180
                    } else {
                        120
                    };
                    let now_ms = now_unix_ms();

                    if collapsed {
                        outside_streak = 0;
                        let in_island_bar = (cx - mon_center_x).abs() <= 244.0
                            && cy >= mon_y_pt - 2.0
                            && cy <= mon_y_pt + 33.0;
                        if in_island_bar {
                            inside_streak = inside_streak.saturating_add(1);
                            let last_collapse =
                                hover_cache.last_collapse_unix_ms.load(Ordering::SeqCst);
                            if inside_streak >= 2 && now_ms.saturating_sub(last_collapse) > 650 {
                                inside_streak = 0;
                                hover_cache.island_collapsed.store(false, Ordering::SeqCst);
                                hover_cache
                                    .island_peek_until_unix_ms
                                    .store(0, Ordering::SeqCst);
                                apply_layout_window_geometry(
                                    &hover_app,
                                    LayoutDensity::DynamicIsland,
                                    false,
                                );
                                let _ = window.show();
                                let _ = hover_app.emit("island-collapsed-sync", false);
                                emit_cached_telemetry(&hover_app, &hover_cache);
                            }
                        } else {
                            inside_streak = 0;
                        }
                    } else {
                        inside_streak = 0;
                        if hover_cache.island_pinned.load(Ordering::SeqCst) {
                            outside_streak = 0;
                            continue;
                        }
                        let peek_until = hover_cache.island_peek_until_unix_ms.load(Ordering::SeqCst);
                        if now_ms < peek_until {
                            outside_streak = 0;
                            continue;
                        }
                        let is_outside =
                            (cx - mon_center_x).abs() > 295.0 || cy > mon_y_pt + 690.0;
                        if is_outside {
                            outside_streak = outside_streak.saturating_add(1);
                            if outside_streak >= 3 {
                                outside_streak = 0;
                                let _ = hover_app.emit("island-request-hide", true);
                            }
                        } else {
                            outside_streak = 0;
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn validate_update_download_url(download_url: &str) -> Result<(), String> {
    let trimmed = download_url.trim();
    let allowed_origin = trimmed.starts_with("https://github.com/minhtri2710/statpulse/")
        || trimmed.starts_with("https://github.com/minhtri2710/superfan/")
        || trimmed.starts_with("https://objects.githubusercontent.com/");
    let path_part = trimmed.split('?').next().unwrap_or("");
    if !allowed_origin || !path_part.to_ascii_lowercase().ends_with(".dmg") {
        return Err("Update URL must be an official StatPulse / SuperFan GitHub Release DMG.".into());
    }
    Ok(())
}

fn current_app_path() -> Result<String, String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = current_exe.to_string_lossy();

    Ok(if let Some(pos) = exe_str.rfind(".app/") {
        exe_str[..pos + 4].to_string()
    } else {
        "/Applications/StatPulse.app".to_string()
    })
}

#[tauri::command]
async fn prepare_app_update(download_url: String) -> Result<(), String> {
    validate_update_download_url(&download_url)?;
    tokio::task::spawn_blocking(move || {
        let app_path = current_app_path()?;
        let mktemp_output = std::process::Command::new("/usr/bin/mktemp")
            .args(["-d", "/private/tmp/superfan-update.XXXXXX"])
            .output()
            .map_err(|e| format!("Failed to create temporary update directory: {e}"))?;
        if !mktemp_output.status.success() {
            return Err("Failed to allocate temporary update directory.".into());
        }
        let work_dir = String::from_utf8_lossy(&mktemp_output.stdout)
            .trim()
            .to_string();
        let temp_dmg = format!("{work_dir}/superfan_update.dmg");
        let mount_point = format!("{work_dir}/mount");
        let _ = std::fs::create_dir_all(&mount_point);

        let result = (|| -> Result<(), String> {
            let curl_status = std::process::Command::new("/usr/bin/curl")
                .args([
                    "-fL",
                    "-A",
                    "StatPulse-Updater",
                    "-o",
                    &temp_dmg,
                    &download_url,
                ])
                .status()
                .map_err(|e| format!("Failed to download update: {e}"))?;

            if !curl_status.success() {
                return Err("Failed to download update file from GitHub.".into());
            }

            let mount_status = std::process::Command::new("/usr/bin/hdiutil")
                .args([
                    "attach",
                    &temp_dmg,
                    "-mountpoint",
                    &mount_point,
                    "-nobrowse",
                    "-quiet",
                ])
                .status()
                .map_err(|e| format!("Failed to mount update package: {e}"))?;

            if !mount_status.success() {
                return Err("Failed to mount DMG package.".into());
            }

            let mut source_app = format!("{mount_point}/SuperFan.app");
            if let Ok(entries) = std::fs::read_dir(&mount_point) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("app") {
                        source_app = path.to_string_lossy().to_string();
                        break;
                    }
                }
            }

            if !std::path::Path::new(&source_app).is_dir() {
                return Err(format!(
                    "Updated application was not found in DMG: {source_app}"
                ));
            }

            let verify_status = std::process::Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict", &source_app])
                .status()
                .map_err(|e| format!("Failed to verify update signature: {e}"))?;

            if !verify_status.success() {
                return Err("Downloaded application failed code signature verification.".into());
            }

            let staged_app = format!("{app_path}.new");
            let _ = std::fs::remove_dir_all(&staged_app);

            let copy_status = std::process::Command::new("/bin/cp")
                .args(["-R", &source_app, &staged_app])
                .status()
                .map_err(|e| format!("Failed to stage updated app: {e}"))?;

            if !copy_status.success() {
                return Err("Failed to copy update into target folder.".into());
            }

            Ok(())
        })();

        let _ = std::process::Command::new("/usr/bin/hdiutil")
            .args(["detach", &mount_point, "-quiet", "-force"])
            .output();
        let _ = std::fs::remove_dir_all(&work_dir);

        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn install_prepared_app_update() -> Result<(), String> {
    let app_path = current_app_path()?;
    let staged_app = format!("{app_path}.new");

    if !std::path::Path::new(&staged_app).is_dir() {
        return Err("The update has not finished downloading yet.".into());
    }

    power_management::restore_system_auto();

    std::process::Command::new("/bin/sh")
        .args([
            "-c",
            "sleep 1; /bin/rm -rf -- \"$1\"; /bin/mv -- \"$2\" \"$1\"; /usr/bin/open -- \"$1\"",
            "--",
            &app_path,
            &staged_app,
        ])
        .spawn()
        .map_err(|e| format!("Failed to launch updater: {e}"))?;

    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::validate_update_download_url;

    #[test]
    fn update_download_url_requires_official_github_dmg() {
        assert!(validate_update_download_url(
            "https://github.com/minhtri2710/statpulse/releases/download/v1.2.7/StatPulse_1.2.7_universal.dmg"
        )
        .is_ok());
        assert!(validate_update_download_url(
            "https://github.com/minhtri2710/superfan/releases/download/v1.2.5/SuperFan_1.2.5_universal.dmg"
        )
        .is_ok());
        assert!(validate_update_download_url("https://evil.example.com/StatPulse.dmg").is_err());
        assert!(validate_update_download_url(
            "https://github.com/minhtri2710/statpulse/releases/download/v1.2.7/StatPulse.zip"
        )
        .is_err());
    }
}
