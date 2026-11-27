use crate::application_preferences::contract::{
    ApplicationPreferences, LayoutDensity, MenuBarStyle, PopoverLook, TemperatureUnit,
};
use crate::hardware_telemetry::contract::{Availability, HardwareTelemetrySnapshot};
use std::f32::consts::PI;

const ICON_HEIGHT: u32 = 36; // 18pt @2x Retina

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarPresentation {
    pub title: String,
    pub icon_rgba: Option<(Vec<u8>, u32, u32)>,
    pub icon_is_template: bool,
}

#[derive(Debug, Clone, Copy)]
enum ThermalSeverity {
    Cool,
    Warm,
    Hot,
}

fn classify_severity(cpu_celsius: Option<f64>) -> ThermalSeverity {
    match cpu_celsius {
        Some(temp) if temp >= 80.0 => ThermalSeverity::Hot,
        Some(temp) if temp >= 60.0 => ThermalSeverity::Warm,
        _ => ThermalSeverity::Cool,
    }
}

pub(crate) fn format_temperature(cpu_celsius: Option<f64>, unit: TemperatureUnit) -> String {
    match (cpu_celsius, unit) {
        (Some(c), TemperatureUnit::Celsius) => format!("{c:.0}°C"),
        (Some(c), TemperatureUnit::Fahrenheit) => format!("{:.0}°F", c * 9.0 / 5.0 + 32.0),
        (None, TemperatureUnit::Celsius) => "--°C".into(),
        (None, TemperatureUnit::Fahrenheit) => "--°F".into(),
    }
}

fn resolve_accent_and_template(
    cpu_celsius: Option<f64>,
    preferences: &ApplicationPreferences,
) -> ([u8; 4], bool) {
    let use_traffic_light =
        preferences.traffic_light_menu_bar || preferences.popover_look == PopoverLook::TrafficLight;
    let severity = classify_severity(cpu_celsius);

    if use_traffic_light {
        return match severity {
            ThermalSeverity::Cool => ([255, 255, 255, 240], true),
            ThermalSeverity::Warm => ([251, 191, 36, 255], false),
            ThermalSeverity::Hot => ([244, 63, 94, 255], false),
        };
    }

    match preferences.popover_look {
        PopoverLook::Classic => ([255, 255, 255, 240], true),
        PopoverLook::Terminal => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [251, 191, 36, 255],
                ThermalSeverity::Cool => [74, 222, 128, 255],
            };
            (color, false)
        }
        PopoverLook::ThermalAmber => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                _ => [251, 146, 60, 255],
            };
            (color, false)
        }
        PopoverLook::Blueprint => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [251, 191, 36, 255],
                ThermalSeverity::Cool => [56, 189, 248, 255],
            };
            (color, false)
        }
        PopoverLook::Pill => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [251, 191, 36, 255],
                ThermalSeverity::Cool => [96, 165, 250, 255],
            };
            (color, false)
        }
        PopoverLook::Neon => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [232, 121, 249, 255],
                ThermalSeverity::Cool => [34, 211, 238, 255],
            };
            (color, false)
        }
        PopoverLook::Sunset => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                _ => [251, 113, 133, 255],
            };
            (color, false)
        }
        PopoverLook::Badge => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [251, 191, 36, 255],
                ThermalSeverity::Cool => [129, 140, 248, 255],
            };
            (color, false)
        }
        PopoverLook::Mint => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                ThermalSeverity::Warm => [251, 191, 36, 255],
                ThermalSeverity::Cool => [52, 211, 153, 255],
            };
            (color, false)
        }
        PopoverLook::Bold => {
            let color = match severity {
                ThermalSeverity::Hot => [244, 63, 94, 255],
                _ => [250, 204, 21, 255],
            };
            (color, false)
        }
        PopoverLook::Catppuccin => {
            let color = match severity {
                ThermalSeverity::Hot => [243, 139, 168, 255],
                ThermalSeverity::Warm => [249, 226, 175, 255],
                ThermalSeverity::Cool => [137, 180, 250, 255],
            };
            (color, false)
        }
        PopoverLook::TokyoNight => {
            let color = match severity {
                ThermalSeverity::Hot => [247, 118, 142, 255],
                ThermalSeverity::Warm => [224, 175, 104, 255],
                ThermalSeverity::Cool => [122, 162, 247, 255],
            };
            (color, false)
        }
        PopoverLook::Nord => {
            let color = match severity {
                ThermalSeverity::Hot => [191, 97, 106, 255],
                ThermalSeverity::Warm => [235, 203, 139, 255],
                ThermalSeverity::Cool => [136, 192, 208, 255],
            };
            (color, false)
        }
        PopoverLook::Gruvbox => {
            let color = match severity {
                ThermalSeverity::Hot => [251, 73, 52, 255],
                ThermalSeverity::Warm => [254, 128, 25, 255],
                ThermalSeverity::Cool => [250, 189, 47, 255],
            };
            (color, false)
        }
        PopoverLook::RosePine => {
            let color = match severity {
                ThermalSeverity::Hot => [235, 111, 146, 255],
                ThermalSeverity::Warm => [246, 193, 119, 255],
                ThermalSeverity::Cool => [156, 207, 216, 255],
            };
            (color, false)
        }
        PopoverLook::Dracula => {
            let color = match severity {
                ThermalSeverity::Hot => [255, 85, 85, 255],
                ThermalSeverity::Warm => [255, 184, 108, 255],
                ThermalSeverity::Cool => [189, 147, 249, 255],
            };
            (color, false)
        }
        PopoverLook::LiquidGlass | PopoverLook::TrafficLight => (
            temp_to_color(cpu_celsius.unwrap_or(45.0), false, [52, 211, 153, 255]),
            false,
        ),
    }
}

fn temp_to_color(celsius: f64, is_template: bool, fallback: [u8; 4]) -> [u8; 4] {
    if is_template {
        return fallback;
    }
    if celsius >= 85.0 {
        [244, 63, 94, 255]
    } else if celsius >= 70.0 {
        [251, 191, 36, 255]
    } else if celsius >= 52.0 {
        [34, 211, 238, 255]
    } else {
        [52, 211, 153, 255]
    }
}

fn padded_sparkline_series(history_celsius: &[f64], cpu_celsius: Option<f64>) -> Vec<f64> {
    let mut series: Vec<f64> = history_celsius.to_vec();
    if series.is_empty() {
        if let Some(c) = cpu_celsius {
            series.push(c);
            series.push(c);
        }
    } else if series.len() == 1 {
        series.push(series[0]);
    }
    series
}

pub fn render_menu_bar(
    snapshot: &HardwareTelemetrySnapshot,
    preferences: &ApplicationPreferences,
    history_celsius: &[f64],
) -> MenuBarPresentation {
    let cpu_celsius = match &snapshot.temperatures {
        Availability::Available { value } => value.cpu_celsius,
        _ => None,
    };
    let sensors = match &snapshot.temperatures {
        Availability::Available { value } => value.sensors.as_slice(),
        _ => &[],
    };
    let fans = match &snapshot.fans {
        Availability::Available { value } => value.as_slice(),
        _ => &[],
    };
    let power_watts = match &snapshot.battery {
        Availability::Available { value } => value.power_watts,
        _ => None,
    };
    let charge_percent = match &snapshot.battery {
        Availability::Available { value } => value.charge_percent,
        _ => None,
    };
    let is_charging = match &snapshot.battery {
        Availability::Available { value } => value.is_charging.unwrap_or(false),
        _ => false,
    };

    let (accent, is_template) = resolve_accent_and_template(cpu_celsius, preferences);
    if preferences.layout_density == LayoutDensity::DynamicIsland {
        let (rgba, w, h) = render_dynamic_island_menu_icon(cpu_celsius, accent);
        return MenuBarPresentation {
            title: String::new(),
            icon_rgba: Some((rgba, w, h)),
            icon_is_template: is_template,
        };
    }
    let temp_label = format_temperature(cpu_celsius, preferences.temperature_unit);

    let max_fan_rpm = fans.iter().map(|fan| fan.speed_rpm).max();
    let fan_load_fraction = fans
        .iter()
        .filter_map(|fan| {
            let min = fan.min_speed_rpm?;
            let max = fan.max_speed_rpm?;
            if max <= min {
                return None;
            }
            Some(((fan.speed_rpm - min) as f32 / (max - min) as f32).clamp(0.0, 1.0))
        })
        .fold(None::<f32>, |acc, val| {
            Some(acc.map_or(val, |a| a.max(val)))
        });

    match preferences.menu_bar_style {
        MenuBarStyle::CompactText => {
            let (rgba, w, h) = render_thermometer_icon(cpu_celsius, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::CombinedTempFan => {
            let (rgba, w, h) = render_fan_icon(accent);
            let title = match max_fan_rpm {
                Some(0) => format!("{temp_label} · Silent"),
                Some(rpm) => format!("{temp_label} · {rpm} RPM"),
                None => temp_label,
            };
            MenuBarPresentation {
                title,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::CombinedTempPower => {
            let (rgba, w, h) = render_power_icon(accent);
            let title = match power_watts {
                Some(w) => format!("{temp_label} · {w:.1}W"),
                None => temp_label,
            };
            MenuBarPresentation {
                title,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::StackedReadout => {
            let top = format!("CPU {temp_label}");
            let bottom = match (max_fan_rpm, power_watts) {
                (Some(0), _) => "SILENT".into(),
                (Some(rpm), _) => format!("{rpm} RPM"),
                (None, Some(w)) => format!("{w:.1}W"),
                (None, None) => "AUTO".into(),
            };
            let (rgba, w, h) = render_stacked_badge(&top, &bottom, accent, is_template);
            MenuBarPresentation {
                title: String::new(),
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::TachometerGauge => {
            let thermal_fraction = cpu_celsius
                .map(|c| ((c as f32 - 30.0) / 65.0).clamp(0.05, 1.0))
                .unwrap_or(0.15);
            let needle_fraction = fan_load_fraction.unwrap_or(thermal_fraction);
            let (rgba, w, h) = render_tachometer_icon(needle_fraction, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::SparklineMini => {
            let series = padded_sparkline_series(history_celsius, cpu_celsius);
            let (rgba, w, h) = render_sparkline_icon(&series, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::PerCoreBars => {
            let values: Vec<f64> = if !sensors.is_empty() {
                sensors.iter().take(10).map(|s| s.celsius).collect()
            } else if let Some(c) = cpu_celsius {
                vec![c]
            } else {
                vec![]
            };
            let (rgba, w, h) = render_per_core_bars_icon(&values, accent, is_template);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::BarChart => {
            let thermal_fraction = cpu_celsius
                .map(|c| ((c as f32 - 30.0) / 65.0).clamp(0.12, 1.0))
                .unwrap_or(0.25);
            let fan_fraction = fan_load_fraction.unwrap_or(0.15).max(0.12);
            let power_fraction = power_watts
                .map(|w| (w as f32 / 90.0).clamp(0.12, 1.0))
                .unwrap_or(0.18);
            let (rgba, w, h) = render_bar_chart_icon(
                [thermal_fraction, fan_fraction, power_fraction],
                accent,
                is_template,
            );
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::Pie => {
            let fraction = cpu_celsius
                .map(|c| ((c as f32 - 25.0) / 70.0).clamp(0.1, 1.0))
                .unwrap_or(0.25);
            let (rgba, w, h) = render_pie_icon(fraction, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::Ring => {
            let fraction = cpu_celsius
                .map(|c| ((c as f32 - 25.0) / 70.0).clamp(0.1, 1.0))
                .unwrap_or(0.25);
            let (rgba, w, h) = render_ring_icon(fraction, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::Meter => {
            let fraction = cpu_celsius
                .map(|c| ((c as f32 - 30.0) / 65.0).clamp(0.1, 1.0))
                .unwrap_or(0.25);
            let (rgba, w, h) = render_meter_icon(fraction, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::UpDownGraph => {
            let series = padded_sparkline_series(history_celsius, cpu_celsius);
            let lower_load = fan_load_fraction
                .or_else(|| power_watts.map(|w| (w as f32 / 80.0).clamp(0.1, 1.0)))
                .unwrap_or(0.25);
            let (rgba, w, h) = render_up_down_graph_icon(&series, lower_load, accent);
            MenuBarPresentation {
                title: temp_label,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
        MenuBarStyle::BatteryIcon => {
            let pct = charge_percent.unwrap_or(100).clamp(0, 100) as f32 / 100.0;
            let (rgba, w, h) = render_battery_icon(pct, is_charging, accent);
            let title = match charge_percent {
                Some(p) => format!("{p}% · {temp_label}"),
                None => temp_label,
            };
            MenuBarPresentation {
                title,
                icon_rgba: Some((rgba, w, h)),
                icon_is_template: is_template,
            }
        }
    }
}

fn render_dynamic_island_menu_icon(
    cpu_celsius: Option<f64>,
    accent: [u8; 4],
) -> (Vec<u8>, u32, u32) {
    let width = 40;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let fill_bg = [accent[0], accent[1], accent[2], 38];
    canvas.fill_round_rect(3.0, 10.0, 34.0, 16.0, 8.0, fill_bg);
    canvas.stroke_round_rect(3.0, 10.0, 34.0, 16.0, 8.0, 1.6, accent);
    canvas.fill_circle(12.0, 18.0, 3.2, accent);

    let load = cpu_celsius
        .map(|c| ((c as f32 - 35.0) / 55.0).clamp(0.2, 1.0))
        .unwrap_or(0.4);
    let draw_bar = |c: &mut Canvas, x: f32, h: f32| {
        let y = 18.0 - h / 2.0;
        c.fill_round_rect(x, y, 2.2, h, 1.1, accent);
    };
    draw_bar(&mut canvas, 20.0, 4.0 + load * 6.0);
    draw_bar(&mut canvas, 24.5, 6.0 + load * 7.0);
    draw_bar(&mut canvas, 29.0, 3.5 + load * 5.0);

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_thermometer_icon(cpu_celsius: Option<f64>, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let mut canvas = Canvas::new(36, ICON_HEIGHT);
    let track = [accent[0], accent[1], accent[2], 70];

    canvas.stroke_round_rect(14.0, 4.0, 8.0, 20.0, 4.0, 2.2, accent);
    canvas.fill_circle(18.0, 26.5, 6.0, accent);
    canvas.fill_round_rect(16.0, 6.0, 4.0, 16.0, 2.0, track);

    let ratio = cpu_celsius
        .map(|t| ((t as f32 - 30.0) / 65.0).clamp(0.15, 1.0))
        .unwrap_or(0.35);
    let fill_h = 15.0 * ratio;
    canvas.fill_round_rect(16.2, 22.0 - fill_h, 3.6, fill_h + 2.0, 1.8, accent);

    canvas.draw_line(24.0, 9.0, 27.0, 9.0, 1.0, track);
    canvas.draw_line(24.0, 14.0, 26.0, 14.0, 1.0, track);
    canvas.draw_line(24.0, 19.0, 27.0, 19.0, 1.0, track);

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_fan_icon(accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let mut canvas = Canvas::new(36, ICON_HEIGHT);
    let cx = 18.0;
    let cy = 18.0;
    let track = [accent[0], accent[1], accent[2], 68];

    canvas.stroke_ring(cx, cy, 13.0, 15.2, track);
    for i in 0..4 {
        let angle = (i as f32) * (PI / 2.0) + 0.35;
        let bx = cx + angle.cos() * 6.5;
        let by = cy + angle.sin() * 6.5;
        canvas.fill_circle(bx, by, 4.2, accent);
    }
    canvas.fill_circle(cx, cy, 3.4, accent);
    (canvas.pixels, canvas.width, canvas.height)
}

fn render_power_icon(accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let mut canvas = Canvas::new(36, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 40];
    let border = [accent[0], accent[1], accent[2], 125];
    canvas.fill_round_rect(4.0, 4.0, 28.0, 28.0, 8.0, bg);
    canvas.stroke_round_rect(4.0, 4.0, 28.0, 28.0, 8.0, 1.6, border);

    canvas.draw_line(20.0, 9.5, 13.5, 18.5, 1.7, accent);
    canvas.draw_line(13.5, 18.5, 19.5, 18.5, 1.7, accent);
    canvas.draw_line(19.5, 18.5, 15.5, 26.5, 1.7, accent);
    (canvas.pixels, canvas.width, canvas.height)
}

fn render_tachometer_icon(fraction: f32, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let mut canvas = Canvas::new(42, ICON_HEIGHT);
    let cx = 21.0;
    let cy = 20.5;
    let r = 13.0;
    let track = [accent[0], accent[1], accent[2], 70];

    // 270 degree arc from 135 deg to 405 deg
    canvas.draw_arc(cx, cy, r, 2.0, 135.0, 270.0, track);
    let active_sweep = 270.0 * fraction.clamp(0.03, 1.0);
    canvas.draw_arc(cx, cy, r, 2.2, 135.0, active_sweep, accent);

    let needle_rad = (135.0 + active_sweep) * PI / 180.0;
    let nx = cx + needle_rad.cos() * (r - 2.5);
    let ny = cy + needle_rad.sin() * (r - 2.5);
    canvas.draw_line(cx, cy, nx, ny, 1.5, accent);
    canvas.fill_circle(cx, cy, 2.8, accent);

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_sparkline_icon(values: &[f64], accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let width = 68;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 30];
    let border = [accent[0], accent[1], accent[2], 125];
    let fill = [accent[0], accent[1], accent[2], 62];

    canvas.fill_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, bg);
    canvas.stroke_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, 1.4, border);

    let slice = if values.len() > 16 {
        &values[values.len() - 16..]
    } else {
        values
    };
    if slice.is_empty() {
        return (canvas.pixels, canvas.width, canvas.height);
    }

    let min_v = slice.iter().copied().fold(f64::INFINITY, f64::min);
    let max_v = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let low = (min_v - 3.0).max(20.0);
    let high = (max_v + 3.0).max(low + 15.0);
    let span = high - low;

    let left_x = 6.0_f32;
    let right_x = (width - 7) as f32;
    let top_y = 8.0_f32;
    let bot_y = 28.0_f32;
    let plot_h = bot_y - top_y;

    let mut points = Vec::with_capacity(slice.len());
    let denom = (slice.len().saturating_sub(1)).max(1) as f32;
    for (idx, v) in slice.iter().enumerate() {
        let t = if slice.len() == 1 {
            1.0
        } else {
            idx as f32 / denom
        };
        let x = left_x + t * (right_x - left_x);
        let norm = ((*v - low) / span).clamp(0.0, 1.0) as f32;
        let y = bot_y - norm * plot_h;
        points.push((x, y));
    }

    for win in points.windows(2) {
        let (x0, y0) = win[0];
        let (x1, y1) = win[1];
        let x_start = x0.floor() as i32;
        let x_end = x1.ceil() as i32;
        let dx = (x1 - x0).max(0.001);
        for px in x_start..=x_end {
            let t = ((px as f32 - x0) / dx).clamp(0.0, 1.0);
            let y_top = y0 + t * (y1 - y0);
            for py in (y_top.ceil() as i32)..=(bot_y as i32) {
                canvas.blend_pixel(px, py, fill, 1.0);
            }
        }
    }

    for win in points.windows(2) {
        canvas.draw_line(win[0].0, win[0].1, win[1].0, win[1].1, 1.25, accent);
    }

    if let Some(&(lx, ly)) = points.last() {
        canvas.fill_circle(lx, ly, 2.2, accent);
    }

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_per_core_bars_icon(
    values: &[f64],
    accent: [u8; 4],
    is_template: bool,
) -> (Vec<u8>, u32, u32) {
    let width = 64;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 30];
    let border = [accent[0], accent[1], accent[2], 125];

    canvas.fill_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, bg);
    canvas.stroke_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, 1.4, border);

    if values.is_empty() {
        return (canvas.pixels, canvas.width, canvas.height);
    }

    let count = values.len().min(10);
    let inner_x = 5.0_f32;
    let inner_w = (width - 10) as f32;
    let gap = 1.5_f32;
    let bar_w = ((inner_w - gap * (count.saturating_sub(1) as f32)) / count as f32).max(2.0);
    let bot_y = 29.0_f32;
    let max_h = 20.0_f32;

    for (idx, &celsius) in values.iter().take(count).enumerate() {
        let norm = ((celsius - 25.0) / 70.0).clamp(0.18, 1.0) as f32;
        let h = (max_h * norm).max(3.0);
        let x = inner_x + idx as f32 * (bar_w + gap);
        let y = bot_y - h;
        let color = temp_to_color(celsius, is_template, accent);
        canvas.fill_round_rect(x, y, bar_w, h, 1.0, color);
    }

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_stacked_badge(
    top: &str,
    bottom: &str,
    accent: [u8; 4],
    is_template: bool,
) -> (Vec<u8>, u32, u32) {
    let scale = 2;
    let top_upper = top.to_ascii_uppercase();
    let bot_upper = bottom.to_ascii_uppercase();
    let top_w = measure_text(&top_upper, scale);
    let bot_w = measure_text(&bot_upper, scale);
    let width = (top_w.max(bot_w) + 16).max(88);

    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 34];
    let border = [accent[0], accent[1], accent[2], 145];
    let sub_text = if is_template {
        [accent[0], accent[1], accent[2], 215]
    } else {
        [240, 245, 255, 235]
    };

    canvas.fill_round_rect(1.0, 1.5, (width - 2) as f32, 33.0, 8.0, bg);
    canvas.stroke_round_rect(1.0, 1.5, (width - 2) as f32, 33.0, 8.0, 1.4, border);

    let top_x = ((width as i32 - top_w as i32) / 2).max(4);
    let bot_x = ((width as i32 - bot_w as i32) / 2).max(4);

    draw_text(&mut canvas, top_x, 5, &top_upper, scale, accent);
    draw_text(&mut canvas, bot_x, 20, &bot_upper, scale, sub_text);

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_bar_chart_icon(
    fractions: [f32; 3],
    accent: [u8; 4],
    is_template: bool,
) -> (Vec<u8>, u32, u32) {
    let width = 48;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 30];
    let border = [accent[0], accent[1], accent[2], 125];
    let palette = if is_template {
        [
            accent,
            [accent[0], accent[1], accent[2], 200],
            [accent[0], accent[1], accent[2], 150],
        ]
    } else {
        [accent, [56, 189, 248, 255], [250, 204, 21, 255]]
    };

    canvas.fill_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, bg);
    canvas.stroke_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, 1.4, border);

    let bot_y = 29.0_f32;
    let max_h = 21.0_f32;
    for (idx, frac) in fractions.iter().enumerate() {
        let x = 7.0 + (idx as f32) * 12.0;
        let h = (max_h * frac.clamp(0.15, 1.0)).max(4.0);
        canvas.fill_round_rect(
            x,
            8.0,
            8.5,
            max_h,
            2.0,
            [accent[0], accent[1], accent[2], 35],
        );
        canvas.fill_round_rect(x, bot_y - h, 8.5, h, 2.0, palette[idx]);
    }

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_pie_icon(fraction: f32, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let width = 38;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let cx = 19.0;
    let cy = 18.0;
    let r = 13.0;
    let track = [accent[0], accent[1], accent[2], 46];
    let border = [accent[0], accent[1], accent[2], 160];

    canvas.fill_circle(cx, cy, r, track);
    canvas.fill_pie_wedge(cx, cy, r - 1.0, -PI / 2.0, fraction * 2.0 * PI, accent);
    canvas.stroke_ring(cx, cy, r - 1.2, r + 0.5, border);
    (canvas.pixels, canvas.width, canvas.height)
}

fn render_ring_icon(fraction: f32, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let width = 38;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let cx = 19.0;
    let cy = 18.0;
    let r_mid = 11.2;
    let track = [accent[0], accent[1], accent[2], 58];

    canvas.stroke_ring(cx, cy, 8.5, 14.0, track);
    let sweep = 360.0 * fraction.clamp(0.08, 1.0);
    canvas.draw_arc(cx, cy, r_mid, 2.6, -90.0, sweep, accent);
    canvas.fill_circle(cx, cy, 3.0, [accent[0], accent[1], accent[2], 190]);
    (canvas.pixels, canvas.width, canvas.height)
}

fn render_meter_icon(fraction: f32, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let width = 56;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 26];
    let border = [accent[0], accent[1], accent[2], 140];
    let off = [accent[0], accent[1], accent[2], 52];

    canvas.fill_round_rect(2.0, 9.0, (width - 4) as f32, 18.0, 5.0, bg);
    canvas.stroke_round_rect(2.0, 9.0, (width - 4) as f32, 18.0, 5.0, 1.5, border);

    let segments = 6;
    let filled = ((fraction * segments as f32).ceil() as usize).clamp(1, segments);
    let seg_w = 5.5_f32;
    let gap = 2.0_f32;
    for i in 0..segments {
        let x = 6.5 + (i as f32) * (seg_w + gap);
        let color = if i < filled { accent } else { off };
        canvas.fill_round_rect(x, 12.5, seg_w, 11.0, 1.5, color);
    }

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_up_down_graph_icon(
    values: &[f64],
    lower_load: f32,
    accent: [u8; 4],
) -> (Vec<u8>, u32, u32) {
    let width = 64;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let bg = [accent[0], accent[1], accent[2], 28];
    let border = [accent[0], accent[1], accent[2], 125];
    let mid_line = [accent[0], accent[1], accent[2], 95];
    let lower_col = [56, 189, 248, 220];

    canvas.fill_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, bg);
    canvas.stroke_round_rect(1.0, 3.0, (width - 2) as f32, 30.0, 6.0, 1.4, border);
    canvas.draw_line(5.0, 18.0, (width - 6) as f32, 18.0, 0.7, mid_line);

    let slice = if values.len() > 14 {
        &values[values.len() - 14..]
    } else {
        values
    };
    if !slice.is_empty() {
        let denom = slice.len().saturating_sub(1).max(1) as f32;
        let left_x = 6.0_f32;
        let right_x = (width - 7) as f32;
        let mut prev_top: Option<(f32, f32)> = None;
        let mut prev_bot: Option<(f32, f32)> = None;
        for (idx, &v) in slice.iter().enumerate() {
            let t = if slice.len() == 1 {
                1.0
            } else {
                idx as f32 / denom
            };
            let x = left_x + t * (right_x - left_x);
            let top_norm = ((v as f32 - 30.0) / 65.0).clamp(0.15, 1.0);
            let top_y = 17.0 - top_norm * 10.5;
            let bot_norm = (lower_load * (0.7 + 0.3 * (idx % 3) as f32 / 2.0)).clamp(0.15, 1.0);
            let bot_y = 19.0 + bot_norm * 10.5;
            if let Some((px, py)) = prev_top {
                canvas.draw_line(px, py, x, top_y, 1.15, accent);
            }
            if let Some((px, py)) = prev_bot {
                canvas.draw_line(px, py, x, bot_y, 1.15, lower_col);
            }
            prev_top = Some((x, top_y));
            prev_bot = Some((x, bot_y));
        }
    }

    (canvas.pixels, canvas.width, canvas.height)
}

fn render_battery_icon(fraction: f32, charging: bool, accent: [u8; 4]) -> (Vec<u8>, u32, u32) {
    let width = 52;
    let mut canvas = Canvas::new(width, ICON_HEIGHT);
    let border = [accent[0], accent[1], accent[2], 210];
    let track = [accent[0], accent[1], accent[2], 45];

    canvas.fill_round_rect(3.0, 9.0, 41.0, 18.0, 4.0, track);
    canvas.stroke_round_rect(3.0, 9.0, 41.0, 18.0, 4.0, 1.7, border);
    // Battery positive terminal nub
    canvas.fill_round_rect(45.5, 14.0, 3.5, 8.0, 1.5, border);

    let fill_w = (35.0 * fraction.clamp(0.08, 1.0)).max(3.5);
    let fill_color = if charging {
        [52, 211, 153, 255]
    } else if fraction <= 0.2 {
        [244, 63, 94, 255]
    } else {
        accent
    };
    canvas.fill_round_rect(6.0, 12.0, fill_w, 12.0, 2.2, fill_color);

    if charging {
        canvas.draw_line(24.0, 11.5, 20.0, 18.0, 1.3, [255, 255, 255, 240]);
        canvas.draw_line(20.0, 18.0, 24.5, 18.0, 1.3, [255, 255, 255, 240]);
        canvas.draw_line(24.5, 18.0, 21.0, 24.5, 1.3, [255, 255, 255, 240]);
    }

    (canvas.pixels, canvas.width, canvas.height)
}

struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4], coverage: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let cov = coverage.clamp(0.0, 1.0);
        if cov <= 0.001 {
            return;
        }
        let src_a = (rgba[3] as f32 / 255.0) * cov;
        let idx = ((y as u32 * self.width + x as u32) * 4) as usize;

        let dst_r = self.pixels[idx] as f32;
        let dst_g = self.pixels[idx + 1] as f32;
        let dst_b = self.pixels[idx + 2] as f32;
        let dst_a = self.pixels[idx + 3] as f32 / 255.0;

        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0001 {
            return;
        }
        let out_r = (rgba[0] as f32 * src_a + dst_r * dst_a * (1.0 - src_a)) / out_a;
        let out_g = (rgba[1] as f32 * src_a + dst_g * dst_a * (1.0 - src_a)) / out_a;
        let out_b = (rgba[2] as f32 * src_a + dst_b * dst_a * (1.0 - src_a)) / out_a;

        self.pixels[idx] = out_r.round().clamp(0.0, 255.0) as u8;
        self.pixels[idx + 1] = out_g.round().clamp(0.0, 255.0) as u8;
        self.pixels[idx + 2] = out_b.round().clamp(0.0, 255.0) as u8;
        self.pixels[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, rgba: [u8; 4]) {
        let min_x = (cx - r - 1.5).floor() as i32;
        let max_x = (cx + r + 1.5).ceil() as i32;
        let min_y = (cy - r - 1.5).floor() as i32;
        let max_y = (cy + r + 1.5).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                self.blend_pixel(x, y, rgba, cov);
            }
        }
    }

    fn fill_pie_wedge(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        start_rad: f32,
        sweep_rad: f32,
        rgba: [u8; 4],
    ) {
        let min_x = (cx - r - 1.5).floor() as i32;
        let max_x = (cx + r + 1.5).ceil() as i32;
        let min_y = (cy - r - 1.5).floor() as i32;
        let max_y = (cy + r + 1.5).ceil() as i32;
        let two_pi = 2.0 * PI;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                if cov <= 0.001 {
                    continue;
                }
                let mut angle = dy.atan2(dx) - start_rad;
                while angle < 0.0 {
                    angle += two_pi;
                }
                while angle >= two_pi {
                    angle -= two_pi;
                }
                if angle <= sweep_rad {
                    self.blend_pixel(x, y, rgba, cov);
                }
            }
        }
    }

    fn stroke_ring(&mut self, cx: f32, cy: f32, r_inner: f32, r_outer: f32, rgba: [u8; 4]) {
        let min_x = (cx - r_outer - 1.5).floor() as i32;
        let max_x = (cx + r_outer + 1.5).ceil() as i32;
        let min_y = (cy - r_outer - 1.5).floor() as i32;
        let max_y = (cy + r_outer + 1.5).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cov_out = (r_outer + 0.5 - d).clamp(0.0, 1.0);
                let cov_in = (d - r_inner + 0.5).clamp(0.0, 1.0);
                self.blend_pixel(x, y, rgba, cov_out.min(cov_in));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_arc(
        &mut self,
        cx: f32,
        cy: f32,
        r_mid: f32,
        half_thick: f32,
        start_deg: f32,
        sweep_deg: f32,
        rgba: [u8; 4],
    ) {
        let steps = (sweep_deg.abs() * 0.6).ceil().max(8.0) as usize;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let deg = start_deg + sweep_deg * (i as f32 / steps as f32);
            let rad = deg * PI / 180.0;
            let px = cx + rad.cos() * r_mid;
            let py = cy + rad.sin() * r_mid;
            if let Some((x0, y0)) = prev {
                self.draw_line(x0, y0, px, py, half_thick, rgba);
            }
            prev = Some((px, py));
        }
    }

    fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, half_thick: f32, rgba: [u8; 4]) {
        let min_x = (x0.min(x1) - half_thick - 2.0).floor() as i32;
        let max_x = (x0.max(x1) + half_thick + 2.0).ceil() as i32;
        let min_y = (y0.min(y1) - half_thick - 2.0).floor() as i32;
        let max_y = (y0.max(y1) + half_thick + 2.0).ceil() as i32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let len_sq = vx * vx + vy * vy;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if len_sq <= 0.0001 {
                    0.0
                } else {
                    (((px - x0) * vx + (py - y0) * vy) / len_sq).clamp(0.0, 1.0)
                };
                let proj_x = x0 + t * vx;
                let proj_y = y0 + t * vy;
                let dist = ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt();
                let cov = (half_thick + 0.5 - dist).clamp(0.0, 1.0);
                self.blend_pixel(x, y, rgba, cov);
            }
        }
    }

    fn round_rect_sdf(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
        let rx = r.min(w / 2.0).min(h / 2.0);
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        let qx = (px - cx).abs() - (w / 2.0 - rx);
        let qy = (py - cy).abs() - (h / 2.0 - rx);
        (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rx
    }

    fn fill_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, rgba: [u8; 4]) {
        let min_x = (x - 1.5).floor() as i32;
        let max_x = (x + w + 1.5).ceil() as i32;
        let min_y = (y - 1.5).floor() as i32;
        let max_y = (y + h + 1.5).ceil() as i32;
        for py_i in min_y..=max_y {
            for px_i in min_x..=max_x {
                let d = Self::round_rect_sdf(px_i as f32 + 0.5, py_i as f32 + 0.5, x, y, w, h, r);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.blend_pixel(px_i, py_i, rgba, cov);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_round_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        thickness: f32,
        rgba: [u8; 4],
    ) {
        let half = thickness / 2.0;
        let min_x = (x - half - 1.5).floor() as i32;
        let max_x = (x + w + half + 1.5).ceil() as i32;
        let min_y = (y - half - 1.5).floor() as i32;
        let max_y = (y + h + half + 1.5).ceil() as i32;
        for py_i in min_y..=max_y {
            for px_i in min_x..=max_x {
                let d = Self::round_rect_sdf(px_i as f32 + 0.5, py_i as f32 + 0.5, x, y, w, h, r);
                let cov = (half + 0.5 - d.abs()).clamp(0.0, 1.0);
                self.blend_pixel(px_i, py_i, rgba, cov);
            }
        }
    }
}

/// Returns (width_in_units, 6_rows_of_4bit_masks)
fn glyph_4x6(ch: char) -> (u32, [u8; 6]) {
    match ch {
        '0' => (4, [0b0110, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110]),
        '1' => (3, [0b010, 0b110, 0b010, 0b010, 0b010, 0b111]),
        '2' => (4, [0b0110, 0b1001, 0b0010, 0b0100, 0b1000, 0b1111]),
        '3' => (4, [0b1110, 0b0001, 0b0110, 0b0001, 0b0001, 0b1110]),
        '4' => (4, [0b1001, 0b1001, 0b1111, 0b0001, 0b0001, 0b0001]),
        '5' => (4, [0b1111, 0b1000, 0b1110, 0b0001, 0b0001, 0b1110]),
        '6' => (4, [0b0111, 0b1000, 0b1110, 0b1001, 0b1001, 0b0110]),
        '7' => (4, [0b1111, 0b0001, 0b0010, 0b0100, 0b0100, 0b0100]),
        '8' => (4, [0b0110, 0b1001, 0b0110, 0b1001, 0b1001, 0b0110]),
        '9' => (4, [0b0110, 0b1001, 0b0111, 0b0001, 0b0001, 0b1110]),
        'A' => (4, [0b0110, 0b1001, 0b1111, 0b1001, 0b1001, 0b1001]),
        'C' => (4, [0b0111, 0b1000, 0b1000, 0b1000, 0b1000, 0b0111]),
        'E' => (4, [0b1111, 0b1000, 0b1110, 0b1000, 0b1000, 0b1111]),
        'F' => (4, [0b1111, 0b1000, 0b1110, 0b1000, 0b1000, 0b1000]),
        'I' => (3, [0b111, 0b010, 0b010, 0b010, 0b010, 0b111]),
        'L' => (4, [0b1000, 0b1000, 0b1000, 0b1000, 0b1000, 0b1111]),
        'M' => (5, [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001]),
        'N' => (4, [0b1001, 0b1101, 0b1011, 0b1001, 0b1001, 0b1001]),
        'O' => (4, [0b0110, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110]),
        'P' => (4, [0b1110, 0b1001, 0b1110, 0b1000, 0b1000, 0b1000]),
        'R' => (4, [0b1110, 0b1001, 0b1110, 0b1010, 0b1001, 0b1001]),
        'S' => (4, [0b0111, 0b1000, 0b0110, 0b0001, 0b0001, 0b1110]),
        'T' => (4, [0b1111, 0b0100, 0b0100, 0b0100, 0b0100, 0b0100]),
        'U' => (4, [0b1001, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110]),
        'W' => (5, [0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001]),
        '°' => (3, [0b111, 0b101, 0b111, 0b000, 0b000, 0b000]),
        '.' => (2, [0b00, 0b00, 0b00, 0b00, 0b00, 0b11]),
        '-' => (3, [0b000, 0b000, 0b111, 0b000, 0b000, 0b000]),
        ':' => (2, [0b00, 0b11, 0b00, 0b00, 0b11, 0b00]),
        ' ' => (2, [0; 6]),
        _ => (3, [0b000, 0b010, 0b000, 0b010, 0b000, 0b000]),
    }
}

fn measure_text(text: &str, scale: u32) -> u32 {
    let mut width = 0;
    let mut first = true;
    for ch in text.chars() {
        let (gw, _) = glyph_4x6(ch);
        if !first {
            width += scale;
        }
        width += gw * scale;
        first = false;
    }
    width
}

fn draw_text(
    canvas: &mut Canvas,
    start_x: i32,
    start_y: i32,
    text: &str,
    scale: u32,
    rgba: [u8; 4],
) {
    let mut cursor_x = start_x;
    for ch in text.chars() {
        let (gw, rows) = glyph_4x6(ch);
        for (ry, &mask) in rows.iter().enumerate() {
            for gx in 0..gw {
                let bit = (mask >> (gw - 1 - gx)) & 1;
                if bit == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            canvas.blend_pixel(
                                cursor_x + (gx * scale + sx) as i32,
                                start_y + (ry as u32 * scale + sy) as i32,
                                rgba,
                                1.0,
                            );
                        }
                    }
                }
            }
        }
        cursor_x += ((gw + 1) * scale) as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_telemetry::contract::{
        BatteryReading, FanActuationStatus, FanMode, FanReading, TemperatureReading,
        TemperatureReadings,
    };

    fn sample_snapshot(cpu_celsius: f64, fan_rpm: i32) -> HardwareTelemetrySnapshot {
        HardwareTelemetrySnapshot {
            temperatures: Availability::Available {
                value: TemperatureReadings {
                    cpu_celsius: Some(cpu_celsius),
                    gpu_celsius: Some(cpu_celsius - 4.0),
                    sensors: vec![
                        TemperatureReading {
                            key: "Tp01".into(),
                            label: "P-Core 1".into(),
                            celsius: cpu_celsius,
                        },
                        TemperatureReading {
                            key: "Tp02".into(),
                            label: "P-Core 2".into(),
                            celsius: cpu_celsius - 3.0,
                        },
                    ],
                },
            },
            fans: Availability::Available {
                value: vec![FanReading {
                    id: 0,
                    label: "Fan 1".into(),
                    speed_rpm: fan_rpm,
                    min_speed_rpm: Some(1200),
                    max_speed_rpm: Some(6000),
                    target_speed_rpm: Some(fan_rpm),
                    mode: Some(FanMode::SystemAuto),
                }],
            },
            battery: Availability::Available {
                value: BatteryReading {
                    charge_percent: Some(82),
                    temperature_celsius: Some(31.0),
                    is_charging: Some(false),
                    cycle_count: Some(90),
                    power_watts: Some(14.2),
                },
            },
            top_processes: Availability::NotPresent,
            fan_actuation_status: FanActuationStatus::Ready,
            captured_at_unix_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn renders_valid_rgba_for_every_menu_bar_style() {
        let styles = [
            MenuBarStyle::CompactText,
            MenuBarStyle::CombinedTempFan,
            MenuBarStyle::CombinedTempPower,
            MenuBarStyle::StackedReadout,
            MenuBarStyle::TachometerGauge,
            MenuBarStyle::SparklineMini,
            MenuBarStyle::PerCoreBars,
            MenuBarStyle::BarChart,
            MenuBarStyle::Pie,
            MenuBarStyle::Ring,
            MenuBarStyle::Meter,
            MenuBarStyle::UpDownGraph,
            MenuBarStyle::BatteryIcon,
        ];
        let snap = sample_snapshot(54.0, 2400);
        let history = [48.0, 50.0, 52.0, 54.0];

        for style in styles {
            let prefs = ApplicationPreferences {
                menu_bar_style: style,
                ..Default::default()
            };
            let rendered = render_menu_bar(&snap, &prefs, &history);
            let (rgba, w, h) = rendered.icon_rgba.expect("style must render icon");
            assert_eq!(h, ICON_HEIGHT);
            assert!(w >= 36);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn traffic_light_is_template_monochrome_when_cool_and_colored_when_warm_or_hot() {
        let prefs = ApplicationPreferences {
            traffic_light_menu_bar: true,
            ..Default::default()
        };

        let cool = render_menu_bar(&sample_snapshot(48.0, 0), &prefs, &[]);
        assert!(cool.icon_is_template);

        let warm = render_menu_bar(&sample_snapshot(68.0, 2800), &prefs, &[]);
        assert!(!warm.icon_is_template);

        let hot = render_menu_bar(&sample_snapshot(86.0, 5400), &prefs, &[]);
        assert!(!hot.icon_is_template);
    }

    #[test]
    fn combined_styles_and_fahrenheit_format_expected_titles() {
        let snap = sample_snapshot(50.0, 0);
        let fan_prefs = ApplicationPreferences {
            temperature_unit: TemperatureUnit::Fahrenheit,
            menu_bar_style: MenuBarStyle::CombinedTempFan,
            ..Default::default()
        };
        assert_eq!(
            render_menu_bar(&snap, &fan_prefs, &[]).title,
            "122°F · Silent"
        );

        let power_prefs = ApplicationPreferences {
            temperature_unit: TemperatureUnit::Celsius,
            menu_bar_style: MenuBarStyle::CombinedTempPower,
            ..Default::default()
        };
        assert_eq!(
            render_menu_bar(&snap, &power_prefs, &[]).title,
            "50°C · 14.2W"
        );

        let stacked_prefs = ApplicationPreferences {
            menu_bar_style: MenuBarStyle::StackedReadout,
            ..Default::default()
        };
        assert_eq!(render_menu_bar(&snap, &stacked_prefs, &[]).title, "");

        let island_prefs = ApplicationPreferences {
            layout_density: LayoutDensity::DynamicIsland,
            ..Default::default()
        };
        let rendered = render_menu_bar(&snap, &island_prefs, &[]);
        assert_eq!(rendered.title, "");
        let (_, w, h) = rendered
            .icon_rgba
            .expect("dynamic island renders capsule icon");
        assert_eq!((w, h), (40, ICON_HEIGHT));
    }
}
