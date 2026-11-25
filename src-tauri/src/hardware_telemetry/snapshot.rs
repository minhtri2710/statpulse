use super::contract::{
    Availability, BatteryReading, FanActuationStatus, FanReading, HardwareTelemetrySnapshot,
    ProcessReading, TemperatureReadings,
};

pub trait SmcTelemetryAdapter {
    fn temperatures(&self) -> Availability<TemperatureReadings>;
    fn fans(&self) -> Availability<Vec<FanReading>>;
}

pub trait IokitTelemetryAdapter {
    fn battery(&self) -> Availability<BatteryReading>;
}

pub trait ProcessTelemetryAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>>;
}

pub struct HardwareTelemetry<S, I, P> {
    smc: S,
    iokit: I,
    process: P,
}

impl<S, I, P> HardwareTelemetry<S, I, P>
where
    S: SmcTelemetryAdapter,
    I: IokitTelemetryAdapter,
    P: ProcessTelemetryAdapter,
{
    pub fn new(smc: S, iokit: I, process: P) -> Self {
        Self {
            smc,
            iokit,
            process,
        }
    }

    pub fn capture(
        &self,
        fan_actuation_status: FanActuationStatus,
        captured_at_unix_ms: u64,
    ) -> HardwareTelemetrySnapshot {
        HardwareTelemetrySnapshot {
            temperatures: self.smc.temperatures(),
            fans: self.smc.fans(),
            battery: self.iokit.battery(),
            top_processes: self.process.top_processes(),
            fan_actuation_status,
            captured_at_unix_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_telemetry::adapters::fixture::FixtureTelemetryAdapter;
    use crate::hardware_telemetry::contract::{FanMode, TemperatureReadings};

    #[test]
    fn fixture_adapter_produces_a_complete_snapshot() {
        let fixture = FixtureTelemetryAdapter {
            temperatures: Availability::Available {
                value: TemperatureReadings {
                    cpu_celsius: Some(70.0),
                    gpu_celsius: None,
                    sensors: vec![],
                },
            },
            fans: Availability::Available {
                value: vec![FanReading {
                    id: 0,
                    label: "Fan 1".into(),
                    speed_rpm: 2200,
                    min_speed_rpm: Some(1200),
                    max_speed_rpm: Some(6000),
                    target_speed_rpm: None,
                    mode: Some(FanMode::SystemAuto),
                }],
            },
            battery: Availability::Available {
                value: BatteryReading {
                    charge_percent: Some(80),
                    temperature_celsius: None,
                    is_charging: Some(true),
                    cycle_count: None,
                    power_watts: None,
                },
            },
            top_processes: Availability::Available {
                value: vec![ProcessReading {
                    name: "Cursor".into(),
                    cpu_percent: 12.0,
                    memory_mb: 950.0,
                    process_count: 4,
                    primary_pid: 4242,
                    is_paused: false,
                }],
            },
        };

        let snapshot = HardwareTelemetry::new(fixture.clone(), fixture.clone(), fixture)
            .capture(FanActuationStatus::Ready, 1_700_000_000_000);

        assert_eq!(snapshot.captured_at_unix_ms, 1_700_000_000_000);
        assert_eq!(snapshot.fan_actuation_status, FanActuationStatus::Ready);
        assert!(matches!(
            snapshot.temperatures,
            Availability::Available { .. }
        ));
        assert!(matches!(snapshot.fans, Availability::Available { .. }));
        assert!(matches!(snapshot.battery, Availability::Available { .. }));
        assert!(matches!(
            snapshot.top_processes,
            Availability::Available { .. }
        ));
    }

    #[test]
    fn snapshot_preserves_adapter_unavailability_without_fallbacks() {
        let fixture = FixtureTelemetryAdapter {
            temperatures: Availability::Unavailable {
                reason: "SMC access denied".into(),
            },
            fans: Availability::Unavailable {
                reason: "SMC access denied".into(),
            },
            battery: Availability::NotPresent,
            top_processes: Availability::Unavailable {
                reason: "ps failed".into(),
            },
        };

        let snapshot = HardwareTelemetry::new(fixture.clone(), fixture.clone(), fixture)
            .capture(FanActuationStatus::Unavailable, 1_700_000_000_000);

        assert_eq!(
            snapshot.temperatures,
            Availability::Unavailable {
                reason: "SMC access denied".into()
            }
        );
        assert_eq!(snapshot.battery, Availability::NotPresent);
    }
}
