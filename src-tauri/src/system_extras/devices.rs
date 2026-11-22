// Device probes: Bluetooth/USB/charger/battery vitals plus SoC power
// breakdown and thermal pressure.

use super::{cmd_stdout, get_or_refresh_swr, parse_ps_rows, round1, SwrEntry};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
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
}

static THERMAL_PRESSURE_CACHE: OnceLock<Mutex<SwrEntry<Option<String>>>> = OnceLock::new();

pub fn collect_devices_snapshot() -> DevicesSnapshot {
    let bt_raw = cmd_stdout(
        "/usr/sbin/system_profiler",
        &["SPBluetoothDataType", "-json"],
    )
    .unwrap_or_default();
    let usb_raw = cmd_stdout("/usr/sbin/ioreg", &["-p", "IOUSB", "-w0"]).unwrap_or_default();
    let bat_raw =
        cmd_stdout("/usr/sbin/ioreg", &["-rw0", "-c", "AppleSmartBattery"]).unwrap_or_default();

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
    get_or_refresh_swr(&THERMAL_PRESSURE_CACHE, Duration::from_secs(6), || {
        let therm_raw = cmd_stdout("/usr/bin/pmset", &["-g", "therm"]).unwrap_or_default();
        Some(parse_pmset_thermal_pressure(&therm_raw))
    })
    .unwrap_or_else(|| "nominal".into())
}

pub(super) fn collect_silicon_power_and_pressure(ps_raw: &str) -> (SiliconPowerBreakdown, String) {
    let pressure = cached_thermal_pressure();

    let package_watts = crate::smc::get_system_power_watts().unwrap_or(0.0);
    if package_watts <= 0.1 {
        return (SiliconPowerBreakdown::default(), pressure);
    }

    let total_pcpu: f64 = parse_ps_rows(ps_raw).into_iter().map(|r| r.pcpu).sum();
    let pcpu_frac = (total_pcpu / 800.0).clamp(0.15, 0.72);
    let smc_cpu = crate::smc::read_smc_key("PCPC")
        .map(f64::from)
        .filter(|w| *w > 0.05 && *w <= package_watts);
    let smc_gpu = crate::smc::read_smc_key("PCPG")
        .map(f64::from)
        .filter(|w| *w > 0.05 && *w <= package_watts);

    let cpu_watts = round1(smc_cpu.unwrap_or(package_watts * pcpu_frac));
    let remaining = (package_watts - cpu_watts).max(0.0);
    let gpu_watts = round1(smc_gpu.unwrap_or(remaining * 0.48));
    let after_gpu = (remaining - gpu_watts).max(0.0);
    let dram_watts = round1(after_gpu * 0.68);
    let ane_watts = round1((after_gpu - dram_watts).max(0.0));

    (
        SiliconPowerBreakdown {
            cpu_watts,
            gpu_watts,
            ane_watts,
            dram_watts,
            package_watts: round1(package_watts),
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
}
