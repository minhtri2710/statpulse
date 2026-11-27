use crate::proc_info::{cmd_stdout, round1};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct BluetoothDeviceItem {
    pub name: String,
    pub device_type: String,
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct UsbDeviceItem {
    pub name: String,
    pub depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ChargerAdapterInfo {
    pub connected: bool,
    pub watts: Option<u32>,
    pub voltage_v: Option<f64>,
    pub current_a: Option<f64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct BatteryHealthVitals {
    #[ts(type = "number | null")]
    pub design_capacity_mah: Option<u32>,
    #[ts(type = "number | null")]
    pub nominal_capacity_mah: Option<u32>,
    #[ts(type = "number | null")]
    pub raw_max_capacity_mah: Option<u32>,
    pub health_percent: Option<u8>,
    pub voltage_v: Option<f64>,
    pub amperage_ma: Option<i32>,
    #[ts(type = "number | null")]
    pub time_remaining_min: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DevicesSnapshot {
    pub bluetooth: Vec<BluetoothDeviceItem>,
    pub usb: Vec<UsbDeviceItem>,
    pub charger: ChargerAdapterInfo,
    #[serde(default)]
    pub battery_health: BatteryHealthVitals,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SiliconPowerBreakdown {
    pub cpu_watts: f64,
    pub gpu_watts: f64,
    pub ane_watts: f64,
    pub dram_watts: f64,
    pub package_watts: f64,
    #[serde(default)]
    pub adapter_watts: f64,
    #[serde(default)]
    pub e_cluster_mhz: u32,
    #[serde(default)]
    pub e_cluster_active_percent: u8,
    #[serde(default)]
    pub p_cluster_mhz: u32,
    #[serde(default)]
    pub p_cluster_active_percent: u8,
    #[serde(default)]
    pub gpu_mhz: u32,
    #[serde(default)]
    pub gpu_active_percent: u8,
    #[serde(default)]
    pub dram_read_gbps: f64,
    #[serde(default)]
    pub dram_write_gbps: f64,
    #[serde(default)]
    pub dram_bw_gbps: f64,
    #[serde(default)]
    pub dram_peak_gbps: f64,
}

const DEVICES_CACHE_TTL: Duration = Duration::from_millis(2500);

static DEVICES_CACHE: Mutex<Option<(Instant, DevicesSnapshot)>> = Mutex::new(None);

/// The mutex is held across the collection, so concurrent callers share one
/// run; rapid repeat calls inside the TTL reuse its result.
pub fn collect_devices_snapshot() -> DevicesSnapshot {
    let mut guard = DEVICES_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((captured_at, snapshot)) = guard.as_ref() {
        if captured_at.elapsed() < DEVICES_CACHE_TTL {
            return snapshot.clone();
        }
    }
    let snapshot = collect_devices_snapshot_uncached();
    *guard = Some((Instant::now(), snapshot.clone()));
    snapshot
}

fn collect_devices_snapshot_uncached() -> DevicesSnapshot {
    let (bt_raw, usb_raw, bat_raw) = std::thread::scope(|s| {
        let bt_h = s.spawn(|| {
            cmd_stdout(
                "/usr/sbin/system_profiler",
                &["SPBluetoothDataType", "-json"],
            )
            .unwrap_or_default()
        });
        let usb_h =
            s.spawn(|| cmd_stdout("/usr/sbin/ioreg", &["-p", "IOUSB", "-w0"]).unwrap_or_default());
        let bat_raw =
            cmd_stdout("/usr/sbin/ioreg", &["-rw0", "-c", "AppleSmartBattery"]).unwrap_or_default();
        (
            bt_h.join().unwrap_or_default(),
            usb_h.join().unwrap_or_default(),
            bat_raw,
        )
    });

    DevicesSnapshot {
        bluetooth: parse_bluetooth_profiler_json(&bt_raw),
        usb: parse_ioreg_usb_tree(&usb_raw),
        charger: parse_ioreg_adapter_details(&bat_raw),
        battery_health: parse_battery_health_vitals(&bat_raw),
    }
}

pub fn parse_ioreg_usb_tree(raw: &str) -> Vec<UsbDeviceItem> {
    let mut devices = Vec::new();
    for line in raw.lines() {
        if !line.contains("<class IOUSBHostDevice") {
            continue;
        }
        let Some(pos) = line.find("+-o ") else {
            continue;
        };
        let depth = (pos / 2) as u8;
        let after_marker = &line[pos + 4..];
        let raw_name = after_marker
            .split('<')
            .next()
            .unwrap_or(after_marker)
            .trim();
        let name = raw_name
            .split('@')
            .next()
            .unwrap_or(raw_name)
            .trim()
            .to_string();
        if !name.is_empty() {
            devices.push(UsbDeviceItem { name, depth });
        }
    }
    devices
}

pub fn parse_ioreg_adapter_details(raw: &str) -> ChargerAdapterInfo {
    for line in raw.lines() {
        if !line.contains("\"AdapterDetails\"") {
            continue;
        }
        let watts = extract_ioreg_u32(line, "\"Watts\"=");
        let voltage_mv = extract_ioreg_u32(line, "\"AdapterVoltage\"=");
        let current_ma = extract_ioreg_u32(line, "\"Current\"=");
        let description = line
            .split_once("\"Description\"=\"")
            .and_then(|(_, rest)| rest.split('"').next())
            .map(|s| s.to_string());
        return ChargerAdapterInfo {
            connected: watts.is_some(),
            watts,
            voltage_v: voltage_mv.map(|mv| round1(mv as f64 / 1000.0)),
            current_a: current_ma.map(|ma| round1(ma as f64 / 1000.0)),
            description,
        };
    }
    ChargerAdapterInfo {
        connected: false,
        watts: None,
        voltage_v: None,
        current_a: None,
        description: None,
    }
}

fn extract_ioreg_u32(line: &str, key: &str) -> Option<u32> {
    let (_, after) = line.split_once(key)?;
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok()
}

pub fn parse_bluetooth_profiler_json(raw: &str) -> Vec<BluetoothDeviceItem> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(ctrl) = val["SPBluetoothDataType"]
        .as_array()
        .and_then(|a| a.first())
    else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for (key, connected) in [("device_connected", true), ("device_not_connected", false)] {
        if let Some(arr) = ctrl[key].as_array() {
            for entry in arr {
                if let Some(obj) = entry.as_object() {
                    for (name, details) in obj {
                        let device_type = details["device_minorType"]
                            .as_str()
                            .unwrap_or("Bluetooth")
                            .to_string();
                        let battery_percent = details["device_batteryLevelMain"]
                            .as_str()
                            .or_else(|| details["device_batteryLevelLeft"].as_str())
                            .and_then(|s| s.trim_end_matches('%').trim().parse::<u8>().ok());
                        let rssi = details["device_rssi"]
                            .as_str()
                            .and_then(|s| s.trim().parse::<i32>().ok());
                        items.push(BluetoothDeviceItem {
                            name: name.clone(),
                            device_type,
                            connected,
                            battery_percent,
                            rssi,
                        });
                    }
                }
            }
        }
    }
    items
}

pub fn parse_pmset_thermal_pressure(raw: &str) -> String {
    for line in raw.lines() {
        if let Some((_, val)) = line.split_once("CPU_Speed_Limit") {
            if let Some(num) = val
                .split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
                .and_then(|s| s.parse::<u32>().ok())
            {
                return if num < 60 {
                    "critical".into()
                } else if num < 85 {
                    "heavy".into()
                } else if num < 100 {
                    "moderate".into()
                } else {
                    "nominal".into()
                };
            }
        }
    }
    "nominal".into()
}

fn cached_thermal_pressure() -> String {
    if let Some(level) = crate::macos_bridge::read_native_thermal_pressure() {
        return level.to_string();
    }
    let therm_raw = cmd_stdout("/usr/bin/pmset", &["-g", "therm"]).unwrap_or_default();
    parse_pmset_thermal_pressure(&therm_raw)
}

fn parse_peak_bw_from_brand(brand: &str) -> f64 {
    let lower = brand.to_ascii_lowercase();
    if !lower.contains("apple") {
        return 0.0;
    }
    if lower.contains("ultra") {
        800.0
    } else if lower.contains("max") {
        400.0
    } else if lower.contains("pro") {
        200.0
    } else if lower.contains("m4") {
        120.0
    } else if lower.contains("m3") || lower.contains("m2") {
        100.0
    } else if lower.contains("m1") {
        68.0
    } else {
        0.0
    }
}

fn detect_apple_silicon_peak_bw_gbps() -> f64 {
    static PEAK_BW: OnceLock<f64> = OnceLock::new();
    *PEAK_BW.get_or_init(|| {
        cmd_stdout("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"])
            .map(|s| parse_peak_bw_from_brand(&s))
            .unwrap_or(0.0)
    })
}

pub(super) fn collect_silicon_power_and_pressure() -> (SiliconPowerBreakdown, String) {
    let pressure = cached_thermal_pressure();

    let ior = crate::macos_bridge::sample_apple_silicon_metrics();
    let dram_read_gbps = round1(ior.map(|m| m.dram_read_gbps).unwrap_or(0.0));
    let dram_write_gbps = round1(ior.map(|m| m.dram_write_gbps).unwrap_or(0.0));
    let dram_bw_gbps = round1(ior.map(|m| m.dram_bw_gbps).unwrap_or(0.0));
    let dram_peak_gbps = detect_apple_silicon_peak_bw_gbps();
    let adapter_watts = crate::smc::get_dc_in_power_watts().unwrap_or(0.0);
    let smc_pkg = crate::smc::get_system_power_watts().unwrap_or(0.0);
    let ior_sum = ior
        .map(|m| m.cpu_watts + m.gpu_watts + m.ane_watts + m.dram_watts)
        .unwrap_or(0.0);
    let package_watts = if smc_pkg > 0.1 { smc_pkg } else { ior_sum };
    if package_watts <= 0.1 {
        return (
            SiliconPowerBreakdown {
                adapter_watts,
                e_cluster_mhz: ior.map(|m| m.e_mhz).unwrap_or(0),
                e_cluster_active_percent: ior.map(|m| m.e_active_pct).unwrap_or(0),
                p_cluster_mhz: ior.map(|m| m.p_mhz).unwrap_or(0),
                p_cluster_active_percent: ior.map(|m| m.p_active_pct).unwrap_or(0),
                gpu_mhz: ior.map(|m| m.gpu_mhz).unwrap_or(0),
                gpu_active_percent: ior.map(|m| m.gpu_active_pct).unwrap_or(0),
                dram_read_gbps,
                dram_write_gbps,
                dram_bw_gbps,
                dram_peak_gbps,
                ..Default::default()
            },
            pressure,
        );
    }

    let gpu_watts = round1(
        ior.map(|m| m.gpu_watts)
            .filter(|&w| w > 0.01)
            .or_else(|| {
                crate::smc::read_smc_key("PCPG")
                    .map(f64::from)
                    .filter(|w| *w > 0.05 && *w <= package_watts)
            })
            .unwrap_or(0.0),
    );

    let max_cpu_budget = (package_watts - gpu_watts).max(0.0);
    let cpu_watts = round1(
        ior.map(|m| m.cpu_watts)
            .filter(|&w| w > 0.01)
            .or_else(|| crate::smc::get_smc_cpu_power_watts().map(|w| w.min(max_cpu_budget)))
            .unwrap_or(0.0),
    );

    let after_cpu_gpu = (package_watts - cpu_watts - gpu_watts).max(0.0);
    let dram_watts = round1(
        ior.map(|m| m.dram_watts)
            .filter(|&w| w > 0.01)
            .or_else(|| crate::smc::get_smc_dram_power_watts().map(|w| w.min(after_cpu_gpu)))
            .unwrap_or(0.0),
    );

    let ane_watts = round1(
        ior.map(|m| m.ane_watts)
            .filter(|&w| w > 0.01)
            .unwrap_or(0.0),
    );

    (
        SiliconPowerBreakdown {
            cpu_watts,
            gpu_watts,
            ane_watts,
            dram_watts,
            package_watts: round1(package_watts),
            adapter_watts,
            e_cluster_mhz: ior.map(|m| m.e_mhz).unwrap_or(0),
            e_cluster_active_percent: ior.map(|m| m.e_active_pct).unwrap_or(0),
            p_cluster_mhz: ior.map(|m| m.p_mhz).unwrap_or(0),
            p_cluster_active_percent: ior.map(|m| m.p_active_pct).unwrap_or(0),
            gpu_mhz: ior.map(|m| m.gpu_mhz).unwrap_or(0),
            gpu_active_percent: ior.map(|m| m.gpu_active_pct).unwrap_or(0),
            dram_read_gbps,
            dram_write_gbps,
            dram_bw_gbps,
            dram_peak_gbps,
        },
        pressure,
    )
}

pub fn parse_battery_health_vitals(raw: &str) -> BatteryHealthVitals {
    let mut design = None;
    let mut nominal = None;
    let mut raw_max = None;
    let mut voltage_mv = None;
    let mut amperage_ma = None;
    let mut time_rem = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        let clean = trimmed.trim_start_matches('|').trim();
        if design.is_none() {
            design = extract_ioreg_u32(trimmed, "\"DesignCapacity\"=");
        }
        if nominal.is_none() {
            nominal = extract_ioreg_u32(trimmed, "\"NominalChargeCapacity\"=");
        }
        if raw_max.is_none() {
            raw_max = extract_ioreg_u32(trimmed, "\"AppleRawMaxCapacity\"=");
        }
        if voltage_mv.is_none() && clean.starts_with("\"Voltage\" =") {
            voltage_mv = clean
                .split('=')
                .nth(1)
                .and_then(|s| s.trim().parse::<u32>().ok());
        }
        if amperage_ma.is_none() && clean.starts_with("\"InstantAmperage\" =") {
            amperage_ma = clean.split('=').nth(1).and_then(|s| {
                let t = s.trim();
                t.parse::<i64>()
                    .ok()
                    .or_else(|| t.parse::<u64>().ok().map(|u| u as i64))
                    .map(|v| v as i32)
            });
        }
        if time_rem.is_none() && clean.starts_with("\"TimeRemaining\" =") {
            time_rem = clean
                .split('=')
                .nth(1)
                .and_then(|s| s.trim().parse::<u32>().ok())
                .filter(|&m| m > 0 && m < 65535);
        }
    }

    let current_cap = raw_max.or(nominal);
    let health_percent = match (current_cap, design) {
        (Some(cur), Some(des)) if des > 0 => Some(
            ((cur as f64 / des as f64) * 100.0)
                .round()
                .clamp(1.0, 100.0) as u8,
        ),
        _ => None,
    };

    BatteryHealthVitals {
        design_capacity_mah: design,
        nominal_capacity_mah: nominal,
        raw_max_capacity_mah: raw_max,
        health_percent,
        voltage_v: voltage_mv.map(|mv| round1(mv as f64 / 1000.0)),
        amperage_ma,
        time_remaining_min: time_rem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_tree_and_adapter_details() {
        let usb = "\
+-o Root  <class IORegistryEntry>
  | +-o USB2.0 Hub@02100000  <class IOUSBHostDevice, id 0x1>
  | | +-o Gaming Keyboard@02132000  <class IOUSBHostDevice, id 0x2>
";
        let usb_items = parse_ioreg_usb_tree(usb);
        assert_eq!(usb_items.len(), 2);
        assert_eq!(usb_items[0].name, "USB2.0 Hub");
        assert_eq!(usb_items[1].name, "Gaming Keyboard");

        let bat = r#"  |   "AdapterDetails" = {"AdapterVoltage"=20000,"Watts"=100,"Current"=5000,"Description"="pd charger"}"#;
        let charger = parse_ioreg_adapter_details(bat);
        assert!(charger.connected);
        assert_eq!(charger.watts, Some(100));
        assert_eq!(charger.voltage_v, Some(20.0));
        assert_eq!(charger.current_a, Some(5.0));
    }

    #[test]
    fn parses_pmset_thermal_pressure_and_battery_vitals() {
        assert_eq!(
            parse_pmset_thermal_pressure("Note: No thermal warning level has been recorded"),
            "nominal"
        );
        assert_eq!(
            parse_pmset_thermal_pressure("CPU_Speed_Limit = 72"),
            "heavy"
        );

        let bat_raw = r#"
  |   "Voltage" = 12604
  |   "BatteryData" = {"DesignCapacity"=8694,"NominalChargeCapacity"=7815,"AppleRawMaxCapacity"=7571}
  |   "InstantAmperage" = -1420
  |   "TimeRemaining" = 185
"#;
        let bh = parse_battery_health_vitals(bat_raw);
        assert_eq!(bh.design_capacity_mah, Some(8694));
        assert_eq!(bh.raw_max_capacity_mah, Some(7571));
        assert_eq!(bh.health_percent, Some(87));
        assert_eq!(bh.voltage_v, Some(12.6));
        assert_eq!(bh.amperage_ma, Some(-1420));
        assert_eq!(bh.time_remaining_min, Some(185));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires live macOS host"]
    fn native_thermal_pressure_and_apple_silicon_metrics_succeed() {
        let pressure =
            crate::macos_bridge::read_native_thermal_pressure().expect("thermalState available");
        assert!(matches!(
            pressure,
            "nominal" | "moderate" | "heavy" | "critical"
        ));
        let _ = crate::macos_bridge::sample_apple_silicon_metrics();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let metrics = crate::macos_bridge::sample_apple_silicon_metrics();
        assert!(metrics.is_some());
    }
}
