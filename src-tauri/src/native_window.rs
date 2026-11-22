// Native window geometry, cursor tracking, and Dynamic Island actions.
// Tauri window/menu registration stays in lib.rs and tray_menu.rs; this
// module owns the NSWindow-level behavior they delegate to.

use crate::application_preferences::contract::LayoutDensity;
use crate::clock::now_unix_ms;
use crate::{ApplicationPreferencesState, TrayRuntimeCache};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub(crate) const ISLAND_PEEK_ORDINARY_MS: u64 = 3_800;
pub(crate) const ISLAND_PEEK_ALERT_MS: u64 = 4_500;
pub(crate) const ISLAND_PEEK_BLAST_MS: u64 = 16_000;

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
            target_x, p.y,
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

pub(crate) fn emit_cached_telemetry(app: &tauri::AppHandle, tray_cache: &TrayRuntimeCache) {
    if let Some(snap) = tray_cache.latest_snapshot.lock().unwrap().as_ref() {
        let _ = app.emit("telemetry-update", snap);
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

pub(crate) fn trigger_island_peek(app: &tauri::AppHandle, duration_ms: u64) {
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

pub(crate) fn hide_dynamic_island_window(app: &tauri::AppHandle) {
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

/// Reset island flags, apply the density's geometry, and show the window —
/// the shared tail of every committed layout change.
pub(crate) fn show_layout(
    app: &tauri::AppHandle,
    tray_cache: &TrayRuntimeCache,
    density: LayoutDensity,
) {
    tray_cache.island_pinned.store(false, Ordering::SeqCst);
    tray_cache.island_collapsed.store(false, Ordering::SeqCst);
    tray_cache.island_peek_until_unix_ms.store(
        if density == LayoutDensity::DynamicIsland {
            now_unix_ms().saturating_add(ISLAND_PEEK_ORDINARY_MS)
        } else {
            0
        },
        Ordering::SeqCst,
    );
    apply_layout_window_geometry(app, density, false);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
    }
    let _ = app.emit("island-collapsed-sync", false);
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
            tray_cache.island_collapsed.store(false, Ordering::SeqCst);
            tray_cache.island_peek_until_unix_ms.store(
                now_unix_ms().saturating_add(ISLAND_PEEK_ALERT_MS),
                Ordering::SeqCst,
            );
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

/// Global cursor tracker for Dynamic Island dormant-in-notch wake and hover
/// retract. Spawned once from setup with the layout watch and runtime cache.
pub(crate) fn spawn_cursor_watcher(
    app: tauri::AppHandle,
    mut layout_rx: tokio::sync::watch::Receiver<LayoutDensity>,
    tray_cache: Arc<TrayRuntimeCache>,
) {
    tauri::async_runtime::spawn(async move {
        let mut inside_streak: u8 = 0;
        let mut outside_streak: u8 = 0;
        let mut cached_monitor: Option<(std::time::Instant, (f64, f64, f64, f64))> = None;
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
                    outside_streak = 0;
                    cached_monitor = None;
                    cached_visible = None;
                    continue;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)) => {}
            }
            let density = *layout_rx.borrow();
            if density != LayoutDensity::DynamicIsland {
                inside_streak = 0;
                outside_streak = 0;
                cached_visible = None;
                continue;
            }
            let Some(window) = app.get_webview_window("main") else {
                sleep_ms = 180;
                continue;
            };
            let (mon_x_pt, mon_y_pt, mon_w_pt, scale) = match cached_monitor {
                Some((ts, bounds)) if ts.elapsed() < std::time::Duration::from_secs(10) => bounds,
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
                Some((ts, vis)) if vis && ts.elapsed() < std::time::Duration::from_secs(5) => vis,
                _ => {
                    let vis = window.is_visible().unwrap_or(false);
                    cached_visible = Some((std::time::Instant::now(), vis));
                    vis
                }
            };
            if !is_visible {
                tray_cache.island_collapsed.store(true, Ordering::SeqCst);
                apply_layout_window_geometry(&app, LayoutDensity::DynamicIsland, true);
                let _ = window.show();
                cached_visible = Some((std::time::Instant::now(), true));
                let _ = app.emit("island-collapsed-sync", true);
                emit_cached_telemetry(&app, &tray_cache);
            }
            let collapsed = tray_cache.island_collapsed.load(Ordering::SeqCst);
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
                    let last_collapse = tray_cache.last_collapse_unix_ms.load(Ordering::SeqCst);
                    if inside_streak >= 2 && now_ms.saturating_sub(last_collapse) > 650 {
                        inside_streak = 0;
                        tray_cache.island_collapsed.store(false, Ordering::SeqCst);
                        tray_cache
                            .island_peek_until_unix_ms
                            .store(0, Ordering::SeqCst);
                        apply_layout_window_geometry(&app, LayoutDensity::DynamicIsland, false);
                        let _ = window.show();
                        let _ = app.emit("island-collapsed-sync", false);
                        emit_cached_telemetry(&app, &tray_cache);
                    }
                } else {
                    inside_streak = 0;
                }
            } else {
                inside_streak = 0;
                if tray_cache.island_pinned.load(Ordering::SeqCst) {
                    outside_streak = 0;
                    continue;
                }
                let peek_until = tray_cache.island_peek_until_unix_ms.load(Ordering::SeqCst);
                if now_ms < peek_until {
                    outside_streak = 0;
                    continue;
                }
                let is_outside = (cx - mon_center_x).abs() > 295.0 || cy > mon_y_pt + 690.0;
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

#[cfg(test)]
mod tests {
    use super::wait_for_island_layout;
    use crate::application_preferences::contract::LayoutDensity;

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
