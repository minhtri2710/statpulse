// Native window geometry, cursor tracking, multi-display sync, and Dynamic Island actions.
// Tauri window/menu registration stays in lib.rs and tray_menu.rs; this
// module owns the NSWindow-level behavior they delegate to.

use crate::application_preferences::contract::{DynamicIslandDisplayMode, LayoutDensity};
use crate::clock::now_unix_ms;
use crate::{ApplicationPreferencesState, TrayRuntimeCache};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub(crate) const ISLAND_PEEK_ORDINARY_MS: u64 = 3_000;
pub(crate) const ISLAND_PEEK_ALERT_MS: u64 = 4_200;
pub(crate) const ISLAND_PEEK_BLAST_MS: u64 = 4_000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayInfo {
    pub(crate) index: usize,
    pub(crate) cocoa_x: f64,
    pub(crate) cocoa_y: f64,
    pub(crate) cocoa_w: f64,
    pub(crate) cocoa_h: f64,
    pub(crate) cg_x: f64,
    pub(crate) cg_y: f64,
    pub(crate) has_notch: bool,
}

#[derive(Clone, Copy, serde::Serialize)]
pub(crate) struct IslandDisplayContext {
    pub(crate) has_notch: bool,
    pub(crate) display_index: usize,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct IslandAlert {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) detail: Option<String>,
    pub(crate) tone: &'static str,
    pub(crate) tab: Option<&'static str>,
    pub(crate) duration_ms: u64,
}

pub(crate) fn island_pill_dimensions(has_notch: bool, alert_active: bool) -> (f64, f64) {
    match (has_notch, alert_active) {
        (true, false) => (488.0, 32.0),
        (true, true) => (488.0, 58.0),
        (false, false) => (330.0, 32.0),
        (false, true) => (330.0, 58.0),
    }
}

pub(crate) fn island_window_dimensions(
    has_notch: bool,
    is_collapsed: bool,
    alert_active: bool,
) -> (f64, f64) {
    if is_collapsed {
        island_pill_dimensions(has_notch, alert_active)
    } else {
        ISLAND_EXPANDED_SIZE
    }
}

pub(crate) fn display_containing_cg_point(
    displays: &[DisplayInfo],
    cx: f64,
    cy: f64,
) -> Option<usize> {
    displays
        .iter()
        .find(|d| cx >= d.cg_x && cx < d.cg_x + d.cocoa_w && cy >= d.cg_y && cy < d.cg_y + d.cocoa_h)
        .map(|d| d.index)
}

pub(crate) fn point_in_display_pill(
    d: &DisplayInfo,
    cx: f64,
    cy: f64,
    pill_w: f64,
    pill_h: f64,
) -> bool {
    let center_x = d.cg_x + d.cocoa_w / 2.0;
    (cx - center_x).abs() <= pill_w / 2.0 && cy >= d.cg_y - 2.0 && cy <= d.cg_y + pill_h + 2.0
}

pub(crate) const ISLAND_EXPANDED_SIZE: (f64, f64) = (560.0, 680.0);
pub(crate) const POPOVER_COMFORTABLE_SIZE: (f64, f64) = (420.0, 700.0);
pub(crate) const POPOVER_COMPACT_SIZE: (f64, f64) = (410.0, 640.0);

pub(crate) const FALLBACK_DISPLAY: DisplayInfo = DisplayInfo {
    index: 0,
    cocoa_x: 0.0,
    cocoa_y: 0.0,
    cocoa_w: 1728.0,
    cocoa_h: 1117.0,
    cg_x: 0.0,
    cg_y: 0.0,
    has_notch: true,
};

pub(crate) fn set_island_collapsed_state(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    collapsed: bool,
) {
    tray_cache
        .island_collapsed
        .store(collapsed, Ordering::SeqCst);
    let _ = app.emit("island-collapsed-sync", collapsed);
}

pub(crate) fn clear_island_alert_state(app: &tauri::AppHandle, tray_cache: &TrayRuntimeCache) {
    tray_cache
        .island_alert_active
        .store(false, Ordering::SeqCst);
    tray_cache.island_alert_seq.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit("island-alert-clear", 0u64);
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
#[repr(C)]
#[derive(Clone, Copy)]
struct MacNsEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

#[cfg(target_os = "macos")]
unsafe fn objc_read_nsrect(obj: *const std::ffi::c_void, sel: *const std::ffi::c_void) -> MacNsRect {
    #[link(name = "objc")]
    extern "C" {
        fn objc_msgSend();
        #[cfg(target_arch = "x86_64")]
        fn objc_msgSend_stret();
    }
    #[cfg(target_arch = "aarch64")]
    {
        let f: extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> MacNsRect =
            std::mem::transmute(objc_msgSend as *const ());
        f(obj, sel)
    }
    #[cfg(target_arch = "x86_64")]
    {
        let mut rect = std::mem::zeroed::<MacNsRect>();
        let f: extern "C" fn(*mut MacNsRect, *const std::ffi::c_void, *const std::ffi::c_void) =
            std::mem::transmute(objc_msgSend_stret as *const ());
        f(&mut rect, obj, sel);
        rect
    }
}

#[cfg(target_os = "macos")]
unsafe fn objc_read_edge_insets(
    obj: *const std::ffi::c_void,
    sel: *const std::ffi::c_void,
) -> MacNsEdgeInsets {
    #[link(name = "objc")]
    extern "C" {
        fn objc_msgSend();
        #[cfg(target_arch = "x86_64")]
        fn objc_msgSend_stret();
    }
    #[cfg(target_arch = "aarch64")]
    {
        let f: extern "C" fn(*const std::ffi::c_void, *const std::ffi::c_void) -> MacNsEdgeInsets =
            std::mem::transmute(objc_msgSend as *const ());
        f(obj, sel)
    }
    #[cfg(target_arch = "x86_64")]
    {
        let mut insets = std::mem::zeroed::<MacNsEdgeInsets>();
        let f: extern "C" fn(
            *mut MacNsEdgeInsets,
            *const std::ffi::c_void,
            *const std::ffi::c_void,
        ) = std::mem::transmute(objc_msgSend_stret as *const ());
        f(&mut insets, obj, sel);
        insets
    }
}

#[cfg(target_os = "macos")]
unsafe fn read_screens_objc() -> Vec<DisplayInfo> {
    use std::ffi::{c_char, c_void};
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> *const c_void;
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend();
    }
    let cls = objc_getClass(c"NSScreen".as_ptr());
    if cls.is_null() {
        return Vec::new();
    }
    let sel_screens = sel_registerName(c"screens".as_ptr());
    let fn_id: extern "C" fn(*const c_void, *const c_void) -> *const c_void =
        std::mem::transmute(objc_msgSend as *const ());
    let screens = fn_id(cls, sel_screens);
    if screens.is_null() {
        return Vec::new();
    }
    let sel_count = sel_registerName(c"count".as_ptr());
    let fn_count: extern "C" fn(*const c_void, *const c_void) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    let count = fn_count(screens, sel_count);
    if count == 0 {
        return Vec::new();
    }

    let sel_obj_at = sel_registerName(c"objectAtIndex:".as_ptr());
    let fn_obj_at: extern "C" fn(*const c_void, *const c_void, usize) -> *const c_void =
        std::mem::transmute(objc_msgSend as *const ());
    let sel_frame = sel_registerName(c"frame".as_ptr());
    let sel_safe = sel_registerName(c"safeAreaInsets".as_ptr());
    let sel_responds = sel_registerName(c"respondsToSelector:".as_ptr());
    let fn_responds: extern "C" fn(*const c_void, *const c_void, *const c_void) -> i8 =
        std::mem::transmute(objc_msgSend as *const ());

    let mut raw: Vec<(MacNsRect, bool)> = Vec::with_capacity(count);
    for i in 0..count {
        let scr = fn_obj_at(screens, sel_obj_at, i);
        if scr.is_null() {
            continue;
        }
        let rect = objc_read_nsrect(scr, sel_frame);
        let has_notch = if fn_responds(scr, sel_responds, sel_safe) != 0 {
            objc_read_edge_insets(scr, sel_safe).top > 0.0
        } else {
            false
        };
        raw.push((rect, has_notch));
    }

    let primary_h = raw.first().map(|(r, _)| r.size.height).unwrap_or(1117.0);
    raw.into_iter()
        .enumerate()
        .map(|(index, (rect, has_notch))| {
            let cocoa_top = rect.origin.y + rect.size.height;
            let cg_y = primary_h - cocoa_top;
            DisplayInfo {
                index,
                cocoa_x: rect.origin.x,
                cocoa_y: rect.origin.y,
                cocoa_w: rect.size.width,
                cocoa_h: rect.size.height,
                cg_x: rect.origin.x,
                cg_y,
                has_notch,
            }
        })
        .collect()
}

fn fallback_displays(app: &tauri::AppHandle) -> Vec<DisplayInfo> {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(monitors) = window.available_monitors() {
            if !monitors.is_empty() {
                let primary_h = monitors
                    .first()
                    .map(|m| m.size().height as f64 / m.scale_factor().max(1.0))
                    .unwrap_or(1117.0);
                return monitors
                    .into_iter()
                    .enumerate()
                    .map(|(index, m)| {
                        let scale = m.scale_factor().max(1.0);
                        let p = m.position();
                        let s = m.size();
                        let cg_x = p.x as f64 / scale;
                        let cg_y = p.y as f64 / scale;
                        let cocoa_w = s.width as f64 / scale;
                        let cocoa_h = s.height as f64 / scale;
                        let cocoa_y = primary_h - (cg_y + cocoa_h);
                        DisplayInfo {
                            index,
                            cocoa_x: cg_x,
                            cocoa_y,
                            cocoa_w,
                            cocoa_h,
                            cg_x,
                            cg_y,
                            has_notch: index == 0,
                        }
                    })
                    .collect();
            }
        }
    }
    vec![DisplayInfo {
        index: 0,
        cocoa_x: 0.0,
        cocoa_y: 0.0,
        cocoa_w: 1728.0,
        cocoa_h: 1117.0,
        cg_x: 0.0,
        cg_y: 0.0,
        has_notch: true,
    }]
}

#[cfg(target_os = "macos")]
pub(crate) fn query_macos_displays(app: &tauri::AppHandle) -> Vec<DisplayInfo> {
    let screens = if unsafe { libc::pthread_main_np() } != 0 {
        unsafe { read_screens_objc() }
    } else if let Some(window) = app.get_webview_window("main") {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        if window
            .run_on_main_thread(move || {
                let _ = tx.send(unsafe { read_screens_objc() });
            })
            .is_ok()
        {
            rx.recv_timeout(std::time::Duration::from_millis(250))
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    if screens.is_empty() {
        fallback_displays(app)
    } else {
        screens
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn query_macos_displays(app: &tauri::AppHandle) -> Vec<DisplayInfo> {
    fallback_displays(app)
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
            let sel = sel_registerName(c"setLevel:".as_ptr());
            let func: extern "C" fn(*mut c_void, *const c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            func(ns_win, sel, level);

            if !is_dynamic_island {
                let sel_beh = sel_registerName(c"setCollectionBehavior:".as_ptr());
                let fn_beh: extern "C" fn(*mut c_void, *const c_void, usize) =
                    std::mem::transmute(objc_msgSend as *const ());
                fn_beh(ns_win, sel_beh, 0);
            }

            let sel_opaque = sel_registerName(c"setOpaque:".as_ptr());
            let fn_opaque: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_opaque(ns_win, sel_opaque, 0);

            let sel_shadow = sel_registerName(c"setHasShadow:".as_ptr());
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
fn apply_macos_display_top_center_frame(
    window: &tauri::WebviewWindow,
    display: &DisplayInfo,
    width: f64,
    height: f64,
) {
    use std::ffi::{c_char, c_void};
    #[link(name = "objc")]
    extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        fn objc_msgSend();
    }
    let target_x = display.cocoa_x + (display.cocoa_w - width) / 2.0;
    // Cocoa screen origin (0, 0) is bottom-left of primary screen; display top bezel is cocoa_y + cocoa_h.
    let target_y = (display.cocoa_y + display.cocoa_h) - height;
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ns_win) = w.ns_window() else {
            return;
        };
        unsafe {
            let sel_level = sel_registerName(c"setLevel:".as_ptr());
            let fn_level: extern "C" fn(*mut c_void, *const c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_level(ns_win, sel_level, 26);

            // CanJoinAllSpaces (1) | FullScreenAuxiliary (256)
            let sel_beh = sel_registerName(c"setCollectionBehavior:".as_ptr());
            let fn_beh: extern "C" fn(*mut c_void, *const c_void, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_beh(ns_win, sel_beh, (1 << 0) | (1 << 8));

            let sel_opaque = sel_registerName(c"setOpaque:".as_ptr());
            let fn_opaque: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_opaque(ns_win, sel_opaque, 0);

            // Disable AppKit rectangular window shadow so curved CSS shadow renders cleanly
            let sel_shadow = sel_registerName(c"setHasShadow:".as_ptr());
            let fn_shadow: extern "C" fn(*mut c_void, *const c_void, i8) =
                std::mem::transmute(objc_msgSend as *const ());
            fn_shadow(ns_win, sel_shadow, 0);

            let sel_frame = sel_registerName(c"setFrame:display:".as_ptr());
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
fn apply_macos_display_top_center_frame(
    window: &tauri::WebviewWindow,
    display: &DisplayInfo,
    width: f64,
    height: f64,
) {
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let target_x = ((display.cg_x + (display.cocoa_w - width) / 2.0) * scale).round() as i32;
    let target_y = (display.cg_y * scale).round() as i32;
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        target_x, target_y,
    )));
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
            (
                p.x as f64,
                p.y as f64,
                s.width as f64,
                m.scale_factor().max(1.0),
            )
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
    let last_rect = *tray_cache.last_tray_rect.lock().unwrap();
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

pub(crate) fn emit_cached_telemetry(app: &tauri::AppHandle, tray_cache: &TrayRuntimeCache) {
    if let Some(snap) = tray_cache.latest_snapshot.lock().unwrap().as_ref() {
        let _ = app.emit("telemetry-update", snap);
    }
}

pub(crate) fn close_all_island_mirrors(app: &tauri::AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("island-mirror-") {
            let _ = win.close();
        }
    }
}

fn resolved_active_display(
    displays: &[DisplayInfo],
    tray_cache: &TrayRuntimeCache,
    display_mode: DynamicIslandDisplayMode,
) -> (usize, DisplayInfo) {
    let mut active_idx = tray_cache.active_display_index.load(Ordering::SeqCst);
    if display_mode == DynamicIslandDisplayMode::Primary || active_idx >= displays.len() {
        active_idx = 0;
        tray_cache.active_display_index.store(0, Ordering::SeqCst);
    }
    let d = displays.get(active_idx).cloned().unwrap_or(FALLBACK_DISPLAY);
    (active_idx, d)
}

pub(crate) fn sync_island_displays(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    density: LayoutDensity,
    display_mode: DynamicIslandDisplayMode,
) {
    if density != LayoutDensity::DynamicIsland {
        close_all_island_mirrors(app);
        return;
    }
    let displays = query_macos_displays(app);
    let (active_idx, active_display) = resolved_active_display(&displays, tray_cache, display_mode);
    let is_collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
    let alert_active = tray_cache.island_alert_active.load(Ordering::SeqCst);

    if let Some(main_win) = app.get_webview_window("main") {
        let (w, h) = island_window_dimensions(active_display.has_notch, is_collapsed, alert_active);
        apply_macos_display_top_center_frame(&main_win, &active_display, w, h);
        let _ = app.emit_to(
            "main",
            "island-display-context",
            IslandDisplayContext {
                has_notch: active_display.has_notch,
                display_index: active_idx,
            },
        );
    }

    if display_mode != DynamicIslandDisplayMode::AllDisplays || displays.len() <= 1 {
        close_all_island_mirrors(app);
        return;
    }

    for (label, win) in app.webview_windows() {
        if let Some(suffix) = label.strip_prefix("island-mirror-") {
            let stale = suffix
                .parse::<usize>()
                .map(|idx| idx == active_idx || idx >= displays.len())
                .unwrap_or(true);
            if stale {
                let _ = win.close();
            }
        }
    }

    for display in &displays {
        if display.index == active_idx {
            continue;
        }
        let label = format!("island-mirror-{}", display.index);
        let (w, h) = island_pill_dimensions(display.has_notch, alert_active);
        let url_path = format!(
            "index.html?role=island-mirror&display={}&notch={}",
            display.index,
            if display.has_notch { 1 } else { 0 }
        );
        let mirror = if let Some(existing) = app.get_webview_window(&label) {
            existing
        } else {
            match tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::App(url_path.into()),
            )
            .title("StatPulse Island")
            .inner_size(w, h)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .shadow(false)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()
            {
                Ok(win) => win,
                Err(_) => continue,
            }
        };
        apply_macos_display_top_center_frame(&mirror, display, w, h);
        let _ = mirror.show();
        let _ = app.emit_to(
            &label,
            "island-display-context",
            IslandDisplayContext {
                has_notch: display.has_notch,
                display_index: display.index,
            },
        );
    }
}

pub(crate) fn apply_layout_window_geometry(
    app: &tauri::AppHandle,
    density: LayoutDensity,
    island_collapsed: bool,
) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
    let is_island = density == LayoutDensity::DynamicIsland;
    if is_island {
        let display_mode = app
            .try_state::<Arc<ApplicationPreferencesState>>()
            .map(|s| *s.dynamic_island_display.borrow())
            .unwrap_or_default();
        let displays = query_macos_displays(app);
        let (active_idx, active_display) =
            resolved_active_display(&displays, &tray_cache, display_mode);
        let alert_active = island_collapsed && tray_cache.island_alert_active.load(Ordering::SeqCst);
        let (width, height) =
            island_window_dimensions(active_display.has_notch, island_collapsed, alert_active);
        apply_macos_display_top_center_frame(&window, &active_display, width, height);
        let _ = app.emit_to(
            "main",
            "island-display-context",
            IslandDisplayContext {
                has_notch: active_display.has_notch,
                display_index: active_idx,
            },
        );
    } else {
        close_all_island_mirrors(app);
        let (width, height) = match density {
            LayoutDensity::Compact => POPOVER_COMPACT_SIZE,
            _ => POPOVER_COMFORTABLE_SIZE,
        };
        set_macos_window_notch_level(&window, false);
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
        position_window_below_tray_or_mouse(&window, &tray_cache, Some(width));
    }
}

fn apply_alert_frame_on_pills(app: &tauri::AppHandle, alert_active: bool) {
    let Some(prefs_state) = app.try_state::<Arc<ApplicationPreferencesState>>() else {
        return;
    };
    if *prefs_state.layout_density.borrow() != LayoutDensity::DynamicIsland {
        return;
    }
    let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() else {
        return;
    };
    let display_mode = *prefs_state.dynamic_island_display.borrow();
    let displays = query_macos_displays(app);
    let (active_idx, active_display) = resolved_active_display(&displays, &tray_cache, display_mode);

    if tray_cache.island_collapsed.load(Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            let (w, h) = island_pill_dimensions(active_display.has_notch, alert_active);
            apply_macos_display_top_center_frame(&window, &active_display, w, h);
            if !window.is_visible().unwrap_or(false) {
                let _ = window.show();
            }
        }
    }

    if display_mode == DynamicIslandDisplayMode::AllDisplays {
        for display in &displays {
            if display.index == active_idx {
                continue;
            }
            let label = format!("island-mirror-{}", display.index);
            if let Some(mirror) = app.get_webview_window(&label) {
                let (w, h) = island_pill_dimensions(display.has_notch, alert_active);
                apply_macos_display_top_center_frame(&mirror, display, w, h);
            }
        }
    }
}

pub(crate) fn trigger_island_alert(
    app: &tauri::AppHandle,
    title: impl Into<String>,
    detail: Option<String>,
    tone: &'static str,
    tab: Option<&'static str>,
    duration_ms: u64,
) {
    let Some(prefs_state) = app.try_state::<Arc<ApplicationPreferencesState>>() else {
        return;
    };
    let density = *prefs_state.layout_density.borrow();
    if density != LayoutDensity::DynamicIsland {
        return;
    }
    let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() else {
        return;
    };
    let seq = tray_cache
        .island_alert_seq
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let alert = IslandAlert {
        id: seq,
        title: title.into(),
        detail,
        tone,
        tab,
        duration_ms,
    };
    let _ = app.emit("island-alert", &alert);

    let is_collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
    let has_mirrors = *prefs_state.dynamic_island_display.borrow()
        == DynamicIslandDisplayMode::AllDisplays
        && query_macos_displays(app).len() > 1;
    if is_collapsed || has_mirrors {
        tray_cache
            .island_alert_active
            .store(true, Ordering::SeqCst);
        apply_alert_frame_on_pills(app, true);
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
        let Some(prefs_state) = app_handle.try_state::<Arc<ApplicationPreferencesState>>() else {
            return;
        };
        if *prefs_state.layout_density.borrow() != LayoutDensity::DynamicIsland {
            return;
        }
        let Some(tray_cache) = app_handle.try_state::<Arc<TrayRuntimeCache>>() else {
            return;
        };
        if tray_cache.island_alert_seq.load(Ordering::SeqCst) != seq {
            return;
        }
        if tray_cache
            .island_alert_active
            .swap(false, Ordering::SeqCst)
        {
            apply_alert_frame_on_pills(&app_handle, false);
        }
        let _ = app_handle.emit("island-alert-clear", seq);
    });
}

pub(crate) fn expand_island_on_display(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    display_mode: DynamicIslandDisplayMode,
    display_index: usize,
    focus: bool,
) {
    let displays = query_macos_displays(app);
    let clamped = if display_mode == DynamicIslandDisplayMode::Primary || displays.is_empty() {
        0
    } else {
        display_index.min(displays.len() - 1)
    };
    tray_cache
        .active_display_index
        .store(clamped, Ordering::SeqCst);
    clear_island_alert_state(app, tray_cache);
    tray_cache
        .island_peek_until_unix_ms
        .store(0, Ordering::SeqCst);
    set_island_collapsed_state(app, tray_cache, false);

    sync_island_displays(app, tray_cache, LayoutDensity::DynamicIsland, display_mode);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        if focus {
            let _ = window.set_focus();
        }
    }
    emit_cached_telemetry(app, tray_cache);
}

pub(crate) fn hide_dynamic_island_window(app: &tauri::AppHandle) {
    if let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() {
        tray_cache
            .last_collapse_unix_ms
            .store(now_unix_ms(), Ordering::SeqCst);
        tray_cache
            .island_peek_until_unix_ms
            .store(0, Ordering::SeqCst);
        clear_island_alert_state(app, &tray_cache);
        set_island_collapsed_state(app, &tray_cache, true);
    }
    apply_layout_window_geometry(app, LayoutDensity::DynamicIsland, true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }
}

/// Reset island flags, apply the density's geometry, and show the window —
/// the shared tail of every committed layout change.
pub(crate) fn show_layout(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    density: LayoutDensity,
) {
    clear_island_alert_state(app, tray_cache);
    tray_cache.island_peek_until_unix_ms.store(
        if density == LayoutDensity::DynamicIsland {
            now_unix_ms().saturating_add(ISLAND_PEEK_ORDINARY_MS)
        } else {
            0
        },
        Ordering::SeqCst,
    );
    set_island_collapsed_state(app, tray_cache, false);
    apply_layout_window_geometry(app, density, false);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
    }
}

pub(crate) fn toggle_main_window_visibility(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let is_visible = window.is_visible().unwrap_or(false);
    let tray_cache = app.state::<Arc<TrayRuntimeCache>>();
    let prefs_state = app.state::<Arc<ApplicationPreferencesState>>();
    let density = *prefs_state.layout_density.borrow();
    if density == LayoutDensity::DynamicIsland {
        let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
        if is_visible && !collapsed {
            hide_dynamic_island_window(app);
        } else {
            let display_mode = *prefs_state.dynamic_island_display.borrow();
            let target_display = if display_mode == DynamicIslandDisplayMode::FollowCursor {
                if let Some((cx, cy)) = macos_global_cursor_points() {
                    let displays = query_macos_displays(app);
                    display_containing_cg_point(&displays, cx, cy)
                        .unwrap_or_else(|| tray_cache.active_display_index.load(Ordering::SeqCst))
                } else {
                    tray_cache.active_display_index.load(Ordering::SeqCst)
                }
            } else {
                tray_cache.active_display_index.load(Ordering::SeqCst)
            };
            expand_island_on_display(app, &tray_cache, display_mode, target_display, false);
            tray_cache.island_peek_until_unix_ms.store(
                now_unix_ms().saturating_add(ISLAND_PEEK_ALERT_MS),
                Ordering::SeqCst,
            );
            let _ = window.set_focus();
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

// Waits until the layout becomes the Dynamic Island; returns false when the
// preference publisher is gone. No timed wakes while parked.
pub(crate) async fn wait_for_island_layout(
    receiver: &mut tokio::sync::watch::Receiver<LayoutDensity>,
) -> bool {
    if *receiver.borrow() == LayoutDensity::DynamicIsland {
        return true;
    }
    receiver
        .wait_for(|density| *density == LayoutDensity::DynamicIsland)
        .await
        .is_ok()
}

/// Global cursor tracker for Dynamic Island dormant-in-notch wake, hover
/// retract, and multi-display adaptation. Spawned once from setup.
pub(crate) fn spawn_cursor_watcher(
    app: tauri::AppHandle,
    mut layout_rx: tokio::sync::watch::Receiver<LayoutDensity>,
    mut display_mode_rx: tokio::sync::watch::Receiver<DynamicIslandDisplayMode>,
    tray_cache: Arc<TrayRuntimeCache>,
) {
    tauri::async_runtime::spawn(async move {
        let mut inside_streak: u8 = 0;
        let mut inside_display: Option<usize> = None;
        let mut outside_streak: u8 = 0;
        let mut follow_streak: u8 = 0;
        let mut follow_target: Option<usize> = None;
        let mut prev_displays: Vec<DisplayInfo> = Vec::new();
        let mut last_display_poll: Option<std::time::Instant> = None;
        let mut cached_visible: Option<(std::time::Instant, bool)> = None;
        let mut sleep_ms: u64 = 120;
        loop {
            // Parked with no timed wakes until the layout switches to
            // the island; false means the publisher is gone.
            if !wait_for_island_layout(&mut layout_rx).await {
                break;
            }
            tokio::select! {
                changed = layout_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    inside_streak = 0;
                    inside_display = None;
                    outside_streak = 0;
                    follow_streak = 0;
                    follow_target = None;
                    last_display_poll = None;
                    cached_visible = None;
                    continue;
                }
                changed = display_mode_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    inside_streak = 0;
                    inside_display = None;
                    outside_streak = 0;
                    follow_streak = 0;
                    follow_target = None;
                    last_display_poll = None;
                    continue;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)) => {}
            }
            if crate::power_management::is_sleeping() {
                sleep_ms = 500;
                continue;
            }
            let density = *layout_rx.borrow();
            if density != LayoutDensity::DynamicIsland {
                inside_streak = 0;
                outside_streak = 0;
                cached_visible = None;
                continue;
            }
            let display_mode = *display_mode_rx.borrow();
            let Some(window) = app.get_webview_window("main") else {
                sleep_ms = 180;
                continue;
            };

            let should_refresh_displays = prev_displays.is_empty()
                || last_display_poll
                    .map(|ts| ts.elapsed() >= std::time::Duration::from_millis(1500))
                    .unwrap_or(true);
            if should_refresh_displays {
                let latest = query_macos_displays(&app);
                last_display_poll = Some(std::time::Instant::now());
                if latest != prev_displays {
                    let was_empty = prev_displays.is_empty();
                    prev_displays = latest;
                    if !was_empty {
                        sync_island_displays(&app, &tray_cache, density, display_mode);
                    }
                }
            }
            if prev_displays.is_empty() {
                sleep_ms = 180;
                continue;
            }

            let scale = window.scale_factor().unwrap_or(2.0).max(1.0);
            let (cx, cy) = if let Some((px, py)) = macos_global_cursor_points() {
                (px, py)
            } else if let Ok(cursor) = window.cursor_position() {
                (cursor.x / scale, cursor.y / scale)
            } else {
                sleep_ms = 180;
                continue;
            };

            let is_visible = match cached_visible {
                Some((ts, vis)) if vis && ts.elapsed() < std::time::Duration::from_secs(5) => vis,
                _ => {
                    let vis = window.is_visible().unwrap_or(false);
                    cached_visible = Some((std::time::Instant::now(), vis));
                    vis
                }
            };
            if !is_visible {
                set_island_collapsed_state(&app, &tray_cache, true);
                sync_island_displays(&app, &tray_cache, density, display_mode);
                let _ = window.show();
                cached_visible = Some((std::time::Instant::now(), true));
                emit_cached_telemetry(&app, &tray_cache);
            }

            let (active_idx, active_display) =
                resolved_active_display(&prev_displays, &tray_cache, display_mode);
            let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);

            let near_top_bezel = match display_mode {
                DynamicIslandDisplayMode::Primary => cy <= active_display.cg_y + 120.0,
                _ => prev_displays.iter().any(|d| cy <= d.cg_y + 120.0),
            };
            sleep_ms = if collapsed
                && !near_top_bezel
                && display_mode != DynamicIslandDisplayMode::FollowCursor
            {
                160
            } else if collapsed && !near_top_bezel {
                120
            } else {
                75
            };
            let now_ms = now_unix_ms();

            if collapsed {
                outside_streak = 0;

                if display_mode == DynamicIslandDisplayMode::FollowCursor && prev_displays.len() > 1
                {
                    let hovered_mon = display_containing_cg_point(&prev_displays, cx, cy);
                    if let Some(target_idx) = hovered_mon {
                        if target_idx != active_idx {
                            if follow_target == Some(target_idx) {
                                follow_streak = follow_streak.saturating_add(1);
                            } else {
                                follow_target = Some(target_idx);
                                follow_streak = 1;
                            }
                            if follow_streak >= 2 {
                                follow_streak = 0;
                                follow_target = None;
                                tray_cache
                                    .active_display_index
                                    .store(target_idx, Ordering::SeqCst);
                                sync_island_displays(&app, &tray_cache, density, display_mode);
                            }
                        } else {
                            follow_streak = 0;
                            follow_target = None;
                        }
                    }
                } else {
                    follow_streak = 0;
                    follow_target = None;
                }

                let alert_active = tray_cache.island_alert_active.load(Ordering::SeqCst);
                let hit_display = if display_mode == DynamicIslandDisplayMode::AllDisplays {
                    prev_displays.iter().find(|d| {
                        let (pw, ph) = island_pill_dimensions(d.has_notch, alert_active);
                        point_in_display_pill(d, cx, cy, pw, ph)
                    })
                } else {
                    let (pw, ph) =
                        island_pill_dimensions(active_display.has_notch, alert_active);
                    point_in_display_pill(&active_display, cx, cy, pw, ph)
                        .then_some(&active_display)
                };

                if let Some(hit) = hit_display {
                    if inside_display == Some(hit.index) {
                        inside_streak = inside_streak.saturating_add(1);
                    } else {
                        inside_display = Some(hit.index);
                        inside_streak = 1;
                    }
                    let last_collapse = tray_cache.last_collapse_unix_ms.load(Ordering::SeqCst);
                    if inside_streak >= 2 && now_ms.saturating_sub(last_collapse) > 400 {
                        inside_streak = 0;
                        inside_display = None;
                        expand_island_on_display(&app, &tray_cache, display_mode, hit.index, false);
                    }
                } else {
                    inside_streak = 0;
                    inside_display = None;
                }
            } else {
                inside_streak = 0;
                inside_display = None;
                follow_streak = 0;
                follow_target = None;
                let peek_until = tray_cache.island_peek_until_unix_ms.load(Ordering::SeqCst);
                if now_ms < peek_until {
                    outside_streak = 0;
                    continue;
                }
                let active_center_x = active_display.cg_x + active_display.cocoa_w / 2.0;
                let is_outside = (cx - active_center_x).abs() > 295.0
                    || cy < active_display.cg_y - 10.0
                    || cy > active_display.cg_y + 690.0;
                if is_outside {
                    outside_streak = outside_streak.saturating_add(1);
                    if outside_streak >= 3 {
                        outside_streak = 0;
                        let _ = app.emit("island-request-hide", true);
                    }
                } else {
                    outside_streak = 0;
                }
            }
        }
    });
}

static GLOBAL_HOTKEY_APP: std::sync::OnceLock<std::sync::Mutex<Option<tauri::AppHandle>>> =
    std::sync::OnceLock::new();

extern "C" fn on_global_hotkey_pressed() {
    let Some(lock) = GLOBAL_HOTKEY_APP.get() else {
        return;
    };
    let Ok(guard) = lock.lock() else {
        return;
    };
    let Some(app) = guard.clone() else {
        return;
    };
    drop(guard);
    tauri::async_runtime::spawn(async move {
        let Some(prefs_state) = app.try_state::<Arc<ApplicationPreferencesState>>() else {
            return;
        };
        let density = *prefs_state.layout_density.borrow();
        if density == LayoutDensity::DynamicIsland {
            let Some(tray_cache) = app.try_state::<Arc<TrayRuntimeCache>>() else {
                return;
            };
            let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
            if collapsed {
                let display_mode = *prefs_state.dynamic_island_display.borrow();
                let active_idx = tray_cache.active_display_index.load(Ordering::SeqCst);
                expand_island_on_display(&app, &tray_cache, display_mode, active_idx, true);
            } else {
                clear_island_alert_state(&app, &tray_cache);
                tray_cache
                    .last_collapse_unix_ms
                    .store(now_unix_ms(), Ordering::SeqCst);
                set_island_collapsed_state(&app, &tray_cache, true);
                apply_layout_window_geometry(&app, density, true);
            }
        } else {
            toggle_main_window_visibility(&app);
        }
    });
}

pub(crate) fn apply_global_shortcut(
    app: &tauri::AppHandle,
    prefs: &crate::application_preferences::contract::ApplicationPreferences,
) {
    use crate::application_preferences::contract::GlobalShortcutPreset;
    let cell = GLOBAL_HOTKEY_APP.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut guard) = cell.lock() {
        *guard = Some(app.clone());
    }
    let binding = match prefs.global_shortcut {
        GlobalShortcutPreset::Disabled => None,
        GlobalShortcutPreset::OptSpace => Some((0x31, 4)),
        GlobalShortcutPreset::CmdShiftS => Some((0x01, 1 | 2)),
        GlobalShortcutPreset::Custom => prefs
            .global_shortcut_custom
            .as_ref()
            .map(|c| (c.key_code, c.modifiers)),
    };
    crate::macos_bridge::register_global_hotkey(binding, on_global_hotkey_pressed);
}

extern "C" fn on_mic_mute_hotkey_pressed() {
    let Some(lock) = GLOBAL_HOTKEY_APP.get() else {
        return;
    };
    let Ok(guard) = lock.lock() else {
        return;
    };
    let Some(app) = guard.clone() else {
        return;
    };
    drop(guard);
    tauri::async_runtime::spawn(async move {
        if let Ok(Ok(audio)) =
            tokio::task::spawn_blocking(crate::system_extras::toggle_mic_mute).await
        {
            let _ = app.emit("audio-snapshot-updated", &audio);
            trigger_island_alert(
                &app,
                if audio.input_muted {
                    "Mic Muted"
                } else {
                    "Mic Unmuted"
                },
                None,
                if audio.input_muted { "warn" } else { "good" },
                Some("volume"),
                ISLAND_PEEK_ORDINARY_MS,
            );
        }
    });
}

pub(crate) fn apply_mic_mute_shortcut(
    app: &tauri::AppHandle,
    prefs: &crate::application_preferences::contract::ApplicationPreferences,
) {
    let cell = GLOBAL_HOTKEY_APP.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut guard) = cell.lock() {
        *guard = Some(app.clone());
    }
    let binding = if prefs.mic_mute_shortcut {
        Some(
            prefs
                .mic_mute_shortcut_custom
                .as_ref()
                .map(|c| (c.key_code, c.modifiers))
                .unwrap_or((0x2E, 1 | 2)),
        )
    } else {
        None
    };
    crate::macos_bridge::register_mic_mute_hotkey(binding, on_mic_mute_hotkey_pressed);
}

#[cfg(test)]
mod tests {
    use super::{
        display_containing_cg_point, island_pill_dimensions, island_window_dimensions,
        point_in_display_pill, wait_for_island_layout, DisplayInfo, ISLAND_EXPANDED_SIZE,
    };
    use crate::application_preferences::contract::LayoutDensity;

    #[test]
    fn island_window_dimensions_switch_between_pill_and_expanded() {
        assert_eq!(island_window_dimensions(true, true, false), (488.0, 32.0));
        assert_eq!(island_window_dimensions(true, true, true), (488.0, 58.0));
        assert_eq!(island_window_dimensions(true, false, false), ISLAND_EXPANDED_SIZE);
        assert_eq!(island_window_dimensions(false, false, true), ISLAND_EXPANDED_SIZE);
    }

    #[test]
    fn multi_display_cg_hit_testing_locates_screen_and_pill() {
        let displays = vec![
            DisplayInfo {
                index: 0,
                cocoa_x: 0.0,
                cocoa_y: 0.0,
                cocoa_w: 1728.0,
                cocoa_h: 1117.0,
                cg_x: 0.0,
                cg_y: 0.0,
                has_notch: true,
            },
            DisplayInfo {
                index: 1,
                cocoa_x: 1728.0,
                cocoa_y: 0.0,
                cocoa_w: 2560.0,
                cocoa_h: 1440.0,
                cg_x: 1728.0,
                cg_y: -323.0,
                has_notch: false,
            },
        ];

        assert_eq!(display_containing_cg_point(&displays, 864.0, 500.0), Some(0));
        assert_eq!(
            display_containing_cg_point(&displays, 3008.0, -310.0),
            Some(1)
        );
        assert_eq!(display_containing_cg_point(&displays, 5000.0, 0.0), None);

        let (pw, ph) = island_pill_dimensions(false, false);
        // External monitor center X = 1728 + 1280 = 3008, top cg_y = -323
        assert!(point_in_display_pill(&displays[1], 3008.0, -310.0, pw, ph));
        assert!(!point_in_display_pill(&displays[1], 3008.0, 0.0, pw, ph));
    }

    #[tokio::test(start_paused = true)]
    async fn island_layout_wait_is_quiescent_until_a_real_switch() {
        let (tx, mut rx) = tokio::sync::watch::channel(LayoutDensity::Comfortable);
        let wait = tokio::spawn(async move { wait_for_island_layout(&mut rx).await });
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        // Virtual time passing must not produce a spurious activation.
        assert!(!wait.is_finished());
        tx.send(LayoutDensity::DynamicIsland).unwrap();
        assert!(wait.await.unwrap());

        // An island layout at startup must not be missed.
        let (_tx, mut rx) = tokio::sync::watch::channel(LayoutDensity::DynamicIsland);
        assert!(wait_for_island_layout(&mut rx).await);

        // A closed publisher cancels the wait instead of parking forever.
        let (tx, mut rx) = tokio::sync::watch::channel(LayoutDensity::Compact);
        drop(tx);
        assert!(!wait_for_island_layout(&mut rx).await);
    }
}
