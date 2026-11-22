use crate::hardware_telemetry::contract::{
    Availability, FanMode, FanReading, TemperatureReading, TemperatureReadings,
};
use crate::hardware_telemetry::snapshot::SmcTelemetryAdapter;
use crate::smc;

#[derive(Clone, Copy, Default)]
pub struct SmcAdapter;

impl SmcTelemetryAdapter for SmcAdapter {
    fn temperatures(&self) -> Availability<TemperatureReadings> {
        if !smc::ensure_smc_open() {
            return Availability::Unavailable {
                reason: "SMC access is unavailable".into(),
            };
        }

        let sensors: Vec<TemperatureReading> = smc::get_all_sensors()
            .into_iter()
            .map(|(key, label, celsius)| TemperatureReading {
                key,
                label,
                celsius,
            })
            .collect();

        let mut cpu_temps: Vec<f64> = sensors
            .iter()
            .filter(|s| s.key.starts_with("Te") || s.key.starts_with("Tp") || s.key.starts_with("TC"))
            .map(|s| s.celsius)
            .collect();
        let cpu_celsius = if cpu_temps.is_empty() {
            smc::get_cpu_temperature()
        } else {
            cpu_temps.sort_by(|a, b| b.total_cmp(a));
            let top_count = std::cmp::min(4, cpu_temps.len());
            let sum: f64 = cpu_temps.iter().take(top_count).sum();
            Some((sum / top_count as f64 * 10.0).round() / 10.0)
        };

        Availability::Available {
            value: TemperatureReadings {
                cpu_celsius,
                gpu_celsius: smc::get_gpu_temperature(),
                sensors,
            },
        }
    }

    fn fans(&self) -> Availability<Vec<FanReading>> {
        if !smc::ensure_smc_open() {
            return Availability::Unavailable {
                reason: "SMC access is unavailable".into(),
            };
        }

        let fans = smc::get_fan_readings()
            .into_iter()
            .map(|fan| FanReading {
                id: fan.id,
                label: fan.label,
                speed_rpm: fan.speed_rpm,
                min_speed_rpm: fan.min_speed_rpm,
                max_speed_rpm: fan.max_speed_rpm,
                target_speed_rpm: fan.target_speed_rpm,
                mode: fan.is_manual.map(|is_manual| {
                    if is_manual {
                        FanMode::Manual
                    } else {
                        FanMode::SystemAuto
                    }
                }),
            })
            .collect::<Vec<_>>();

        if fans.is_empty() {
            Availability::NotPresent
        } else {
            Availability::Available { value: fans }
        }
    }
}
