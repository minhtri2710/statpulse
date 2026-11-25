use crate::hardware_telemetry::contract::{
    Availability, FanReading, TemperatureReading, TemperatureReadings,
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

        let cpu_celsius = smc::average_top_cpu_celsius(
            sensors
                .iter()
                .filter(|s| smc::is_cpu_sensor_key(&s.key))
                .map(|s| s.celsius),
        )
        .or_else(smc::get_cpu_temperature);

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

        let fans = match smc::get_fan_readings() {
            Ok(fans) => fans,
            Err(reason) => {
                return Availability::Unavailable { reason };
            }
        };

        if fans.is_empty() {
            Availability::NotPresent
        } else {
            Availability::Available { value: fans }
        }
    }
}
