use crate::hardware_telemetry::contract::{
    Availability, FanReading, TemperatureReading, TemperatureReadings,
};
use crate::hardware_telemetry::snapshot::SmcTelemetryAdapter;
use crate::smc;

#[derive(Clone, Copy, Default)]
pub struct SmcAdapter;

#[derive(Clone, Copy, Default)]
pub struct SmcMetricsAdapter;

fn temperatures_from(sensors_raw: Vec<(String, String, f64)>) -> Availability<TemperatureReadings> {
    if !smc::ensure_smc_open() {
        return Availability::Unavailable {
            reason: "SMC access is unavailable".into(),
        };
    }

    let sensors: Vec<TemperatureReading> = sensors_raw
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
    );

    let gpu_celsius = sensors
        .iter()
        .filter(|s| smc::is_gpu_sensor_key(&s.key))
        .map(|s| s.celsius)
        .max_by(|l, r| l.total_cmp(r));

    Availability::Available {
        value: TemperatureReadings {
            cpu_celsius,
            gpu_celsius,
            sensors,
        },
    }
}

fn fans_from(
    probe: impl FnOnce() -> Result<Vec<FanReading>, String>,
) -> Availability<Vec<FanReading>> {
    if !smc::ensure_smc_open() {
        return Availability::Unavailable {
            reason: "SMC access is unavailable".into(),
        };
    }

    let fans = match probe() {
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

impl SmcTelemetryAdapter for SmcAdapter {
    fn temperatures(&self) -> Availability<TemperatureReadings> {
        temperatures_from(smc::get_all_sensors())
    }

    fn fans(&self) -> Availability<Vec<FanReading>> {
        fans_from(smc::get_fan_readings)
    }
}

impl SmcTelemetryAdapter for SmcMetricsAdapter {
    fn temperatures(&self) -> Availability<TemperatureReadings> {
        temperatures_from(smc::get_core_temperatures())
    }

    fn fans(&self) -> Availability<Vec<FanReading>> {
        fans_from(smc::get_fan_speeds_fast)
    }
}
