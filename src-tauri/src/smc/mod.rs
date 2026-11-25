use crate::hardware_telemetry::contract::{BatteryReading, FanMode, FanReading};
use crate::proc_info::round1;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_uchar};
use std::sync::{Mutex, OnceLock};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SMCVal {
    pub key: [c_char; 5],
    pub data_size: u32,
    pub data_type: [c_char; 5],
    pub bytes: [c_uchar; 32],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct BatteryInfoC {
    pub percentage: c_int,
    pub is_charging: c_int,
    pub cycle_count: c_int,
    pub temperature: f64,
    pub power_watts: f64,
    pub has_battery: c_int,
    pub has_percentage: c_int,
    pub has_charging: c_int,
    pub has_cycle_count: c_int,
    pub has_temperature: c_int,
    pub has_power_watts: c_int,
}

extern "C" {
    pub fn SMCOpen(conn: *mut u32) -> c_int;
    pub fn SMCReadKey(key: *const c_char, val: *mut SMCVal, conn: u32) -> c_int;
    pub fn getFloatFromVal(val: SMCVal) -> f32;
    pub fn getFanCount(conn: u32) -> c_int;
    pub fn getFanMinSpeed(fan_num: c_int, conn: u32) -> f32;
    pub fn getFanMaxSpeed(fan_num: c_int, conn: u32) -> f32;
    pub fn setFanSpeed(fan_num: c_int, speed: c_int, conn: u32) -> c_int;
    pub fn setFanAuto(fan_num: c_int, conn: u32) -> c_int;
    pub fn setAllFansAuto(conn: u32) -> c_int;
    pub fn fetch_battery_info(info: *mut BatteryInfoC) -> c_int;
}

static SMC_CONNECTION: Mutex<u32> = Mutex::new(0);

pub fn ensure_smc_open() -> bool {
    let mut conn = SMC_CONNECTION.lock().unwrap();
    if *conn != 0 {
        return true;
    }
    let mut new_conn: u32 = 0;
    let res = unsafe { SMCOpen(&mut new_conn) };
    if res == 0 && new_conn != 0 {
        *conn = new_conn;
        true
    } else {
        false
    }
}

pub fn with_smc_connection<T>(
    operation: impl FnOnce(u32) -> Result<T, String>,
) -> Result<T, String> {
    if !ensure_smc_open() {
        return Err("SMC access is unavailable".into());
    }
    let conn = *SMC_CONNECTION.lock().unwrap();
    operation(conn)
}

pub fn read_smc_key(key_str: &str) -> Option<f32> {
    if !ensure_smc_open() {
        return None;
    }
    let conn = *SMC_CONNECTION.lock().unwrap();
    let c_key = CString::new(key_str).ok()?;
    let mut val: SMCVal = unsafe { std::mem::zeroed() };

    let res = unsafe { SMCReadKey(c_key.as_ptr(), &mut val, conn) };
    if res == 0 {
        let fval = unsafe { getFloatFromVal(val) };
        if (0.0..65535.0).contains(&fval) {
            return Some(fval);
        }
    }
    None
}

const CPU_INTEL_KEYS: &[&str] = &[
    "TC0P", "TCXC", "TC0E", "TC0F", "TC0D", "TC1C", "TC2C", "TC3C", "TC4C",
];
const GPU_INTEL_KEYS: &[&str] = &["TGDD", "TG0P", "TG0D", "TG0E", "TG0F"];

const CPU_APPLE_SILICON_KEYS: &[&str] = &[
    "Te04", "Te05", "Te09", "Te0H", "Te0L", "Te0P", "Te0S", "Tf04", "Tf09", "Tf0A", "Tf0B", "Tf0D",
    "Tf0E", "Tf14", "Tf18", "Tf19", "Tf1A", "Tf24", "Tf28", "Tf29", "Tf2A", "Tf44", "Tf49", "Tf4A",
    "Tf4B", "Tf4D", "Tf4E", "Tp01", "Tp05", "Tp09", "Tp0D", "Tp0H", "Tp0L", "Tp0P", "Tp0T", "Tp0X",
    "Tp0b", "Tp0f", "Tp0j", "Tp0n", "Tp0r", "Tp0v", "Tp0z", "Tp19", "Tp1d", "Tp1f", "Tp1h", "Tp1n",
    "Tp1p", "Tp1t", "Tp1v", "Tp2a", "Tp2f", "Tp2h",
];
const GPU_APPLE_SILICON_KEYS: &[&str] = &[
    "Tg04", "Tg05", "Tg0D", "Tg0G", "Tg0H", "Tg0K", "Tg0L", "Tg0T", "Tg0U", "Tg0V", "Tg0X", "Tg0d",
    "Tg0f", "Tg0g", "Tg0j", "Tg0k", "Tg1U", "Tg1Y", "Tg1c", "Tg1f", "Tg1g", "Tg1j", "Tg1k",
];
const CHASSIS_SMC_KEYS: &[(&str, &str)] = &[
    ("TH0a", "SSD NAND A"),
    ("TH0b", "SSD NAND B"),
    ("TH0F", "SSD NAND C"),
    ("TH0x", "SSD Controller"),
    ("Ts0P", "Palm Rest L"),
    ("Ts1P", "Palm Rest R"),
    ("Ta0P", "Airflow Intake"),
    ("TaLP", "Airflow Left"),
    ("TaSP", "Airflow System"),
    ("TaRF", "Airflow Rear"),
    ("TB0T", "Battery Cell 1"),
    ("TB1T", "Battery Cell 2"),
    ("Tm0P", "Logic Board"),
    ("TW0P", "Wi-Fi Module"),
    ("TW0p", "Wi-Fi Proximity"),
];
const POWER_SMC_KEYS: &[&str] = &["PSTR", "PDTR", "PCPT", "PCPC"];

static SUPPORTED_CPU_KEYS: OnceLock<Vec<&'static str>> = OnceLock::new();
static SUPPORTED_GPU_KEYS: OnceLock<Vec<&'static str>> = OnceLock::new();
static SUPPORTED_CHASSIS_KEYS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();

fn supported_cpu_keys() -> &'static [&'static str] {
    SUPPORTED_CPU_KEYS.get_or_init(|| {
        if !ensure_smc_open() {
            return Vec::new();
        }
        CPU_APPLE_SILICON_KEYS
            .iter()
            .chain(CPU_INTEL_KEYS.iter())
            .copied()
            .filter(|k| read_smc_key(k).is_some_and(|t| t > 15.0 && t < 125.0))
            .collect()
    })
}

fn supported_gpu_keys() -> &'static [&'static str] {
    SUPPORTED_GPU_KEYS.get_or_init(|| {
        if !ensure_smc_open() {
            return Vec::new();
        }
        GPU_APPLE_SILICON_KEYS
            .iter()
            .chain(GPU_INTEL_KEYS.iter())
            .copied()
            .filter(|k| read_smc_key(k).is_some_and(|t| t > 15.0 && t < 125.0))
            .collect()
    })
}

fn supported_chassis_keys() -> &'static [(&'static str, &'static str)] {
    SUPPORTED_CHASSIS_KEYS.get_or_init(|| {
        if !ensure_smc_open() {
            return Vec::new();
        }
        CHASSIS_SMC_KEYS
            .iter()
            .copied()
            .filter(|&(k, _)| read_smc_key(k).is_some_and(|t| t > 10.0 && t < 125.0))
            .collect()
    })
}

pub fn is_cpu_sensor_key(key: &str) -> bool {
    key.starts_with("Te")
        || key.starts_with("Tf")
        || key.starts_with("Tp")
        || key.starts_with("TC")
}

pub fn average_top_cpu_celsius(temps: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut valid: Vec<f64> = temps.into_iter().collect();
    if valid.is_empty() {
        return None;
    }
    valid.sort_by(|a, b| b.total_cmp(a));
    let top_count = std::cmp::min(4, valid.len());
    let sum: f64 = valid.iter().take(top_count).sum();
    Some(round1(sum / top_count as f64))
}

pub fn get_system_power_watts() -> Option<f64> {
    for key in POWER_SMC_KEYS {
        if let Some(w) = read_smc_key(key) {
            if w > 0.2 && w < 500.0 {
                return Some(round1(f64::from(w)));
            }
        }
    }
    None
}

pub fn get_dc_in_power_watts() -> Option<f64> {
    read_smc_key("PDTR")
        .filter(|&w| w > 0.2 && w < 500.0)
        .map(|w| round1(f64::from(w)))
}

pub fn get_smc_cpu_power_watts() -> Option<f64> {
    for key in ["PCPC", "PC0C", "PHPC"] {
        if let Some(w) = read_smc_key(key) {
            if w > 0.05 && w < 300.0 {
                return Some(f64::from(w));
            }
        }
    }
    None
}

pub fn get_smc_dram_power_watts() -> Option<f64> {
    read_smc_key("PMVR")
        .filter(|&w| w > 0.05 && w < 150.0)
        .map(f64::from)
}

pub fn get_cpu_temperature() -> Option<f64> {
    let discovered = supported_cpu_keys();
    let keys: Box<dyn Iterator<Item = &str>> = if discovered.is_empty() {
        Box::new(
            CPU_APPLE_SILICON_KEYS
                .iter()
                .chain(CPU_INTEL_KEYS.iter())
                .copied(),
        )
    } else {
        Box::new(discovered.iter().copied())
    };

    average_top_cpu_celsius(
        keys.filter_map(read_smc_key)
            .filter(|&t| t > 15.0 && t < 125.0)
            .map(f64::from),
    )
}

pub fn get_gpu_temperature() -> Option<f64> {
    let discovered = supported_gpu_keys();
    let keys: Box<dyn Iterator<Item = &str>> = if discovered.is_empty() {
        Box::new(
            GPU_APPLE_SILICON_KEYS
                .iter()
                .chain(GPU_INTEL_KEYS.iter())
                .copied(),
        )
    } else {
        Box::new(discovered.iter().copied())
    };

    keys.filter_map(read_smc_key)
        .filter(|temperature| *temperature > 15.0 && *temperature < 125.0)
        .map(f64::from)
        .max_by(|left, right| left.total_cmp(right))
        .map(round1)
}

pub fn get_all_sensors() -> Vec<(String, String, f64)> {
    let mut sensors = Vec::new();
    let mut core_idx = 1;
    let discovered_cpu = supported_cpu_keys();
    let cpu_keys: Box<dyn Iterator<Item = &str>> = if discovered_cpu.is_empty() {
        Box::new(
            CPU_APPLE_SILICON_KEYS
                .iter()
                .chain(CPU_INTEL_KEYS.iter())
                .copied(),
        )
    } else {
        Box::new(discovered_cpu.iter().copied())
    };

    for key in cpu_keys {
        if let Some(t) = read_smc_key(key) {
            if t > 15.0 && t < 125.0 {
                let label = if key.starts_with("Te") {
                    format!("E-Core {}", core_idx)
                } else if key.starts_with("Tp") {
                    format!("P-Core {}", core_idx)
                } else {
                    format!("CPU Core {}", core_idx)
                };
                sensors.push((key.to_string(), label, round1(f64::from(t))));
                core_idx += 1;
            }
        }
    }

    let discovered_chassis = supported_chassis_keys();
    let chassis_keys = if discovered_chassis.is_empty() {
        CHASSIS_SMC_KEYS
    } else {
        discovered_chassis
    };
    for &(key, label) in chassis_keys {
        if let Some(t) = read_smc_key(key) {
            if t > 10.0 && t < 125.0 {
                sensors.push((key.to_string(), label.to_string(), round1(f64::from(t))));
            }
        }
    }
    sensors
}

// Reads the declared fans keyed by the checked hardware count. A fan that was
// counted but no longer reports a speed is an error, not a partial reading:
// the policy must never heartbeat a missing controlled fan.
fn collect_fan_readings(
    count: i32,
    read: impl Fn(&str) -> Option<f32>,
) -> Result<Vec<FanReading>, String> {
    if count < 0 {
        return Err("hardware fan count is unavailable".into());
    }
    let mut fans = Vec::with_capacity(count as usize);
    for i in 0..count {
        let speed_rpm = read(&format!("F{i}Ac"))
            .map(|v| v.round() as i32)
            .ok_or_else(|| format!("declared fan {i} reports no speed"))?;
        let min_speed_rpm = read(&format!("F{i}Mn")).map(|v| v.round() as i32);
        let max_speed_rpm = read(&format!("F{i}Mx")).map(|v| v.round() as i32);
        let target_speed_rpm = read(&format!("F{i}Tg")).map(|v| v.round() as i32);
        let mode = read(&format!("F{i}Md"))
            .or_else(|| read(&format!("F{i}md")))
            .map(|v| {
                if (v as i32) == 1 {
                    FanMode::Manual
                } else {
                    FanMode::SystemAuto
                }
            });

        let label = match (count, i) {
            (1, 0) => "Fan 1".into(),
            (_, 0) => "Fan 1 (Left)".into(),
            (_, 1) => "Fan 2 (Right)".into(),
            _ => format!("Fan {}", i + 1),
        };

        fans.push(FanReading {
            id: i as usize,
            label,
            speed_rpm,
            min_speed_rpm,
            max_speed_rpm,
            target_speed_rpm,
            mode,
        });
    }
    Ok(fans)
}

pub fn get_fan_readings() -> Result<Vec<FanReading>, String> {
    let count = with_smc_connection(|conn| Ok(unsafe { getFanCount(conn) }))?;
    collect_fan_readings(count, read_smc_key)
}

pub fn get_battery_reading() -> Result<Option<BatteryReading>, String> {
    let mut c_info: BatteryInfoC = unsafe { std::mem::zeroed() };
    let status = unsafe { fetch_battery_info(&mut c_info) };

    match status {
        1 => {
            let battery_watts = (c_info.has_power_watts != 0).then_some(c_info.power_watts);
            let power_watts = match battery_watts {
                Some(w) if w >= 0.2 => Some(w),
                _ => get_system_power_watts().or(battery_watts),
            };
            Ok(Some(BatteryReading {
                charge_percent: (c_info.has_percentage != 0).then_some(c_info.percentage),
                temperature_celsius: (c_info.has_temperature != 0)
                    .then_some(round1(c_info.temperature)),
                is_charging: (c_info.has_charging != 0).then_some(c_info.is_charging != 0),
                cycle_count: (c_info.has_cycle_count != 0).then_some(c_info.cycle_count),
                power_watts,
            }))
        }
        0 => Ok(None),
        _ => Err("IOKit battery access is unavailable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::collect_fan_readings;

    #[test]
    fn negative_fan_count_is_unavailable() {
        assert!(collect_fan_readings(-1, |_| None).is_err());
    }

    #[test]
    fn zero_fan_count_probes_no_fan_keys() {
        let fans =
            collect_fan_readings(0, |_| panic!("fan key probed without a declared fan")).unwrap();
        assert!(fans.is_empty());
    }

    #[test]
    fn missing_declared_speed_fails_instead_of_returning_partial() {
        let result = collect_fan_readings(2, |key| if key == "F0Ac" { Some(2100.0) } else { None });
        assert!(result.is_err());
    }

    #[test]
    fn all_declared_speeds_produce_readings() {
        let fans = collect_fan_readings(2, |key| match key {
            "F0Ac" | "F1Ac" => Some(2100.0),
            _ => None,
        })
        .unwrap();
        assert_eq!(fans.len(), 2);
        assert_eq!(fans[0].speed_rpm, 2100);
    }
}
