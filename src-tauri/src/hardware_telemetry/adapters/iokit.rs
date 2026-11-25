use crate::hardware_telemetry::contract::{Availability, BatteryReading};
use crate::hardware_telemetry::snapshot::IokitTelemetryAdapter;
use crate::smc;

#[derive(Clone, Copy, Default)]
pub struct IokitAdapter;

impl IokitTelemetryAdapter for IokitAdapter {
    fn battery(&self) -> Availability<BatteryReading> {
        match smc::get_battery_reading() {
            Ok(Some(value)) => Availability::Available { value },
            Ok(None) => Availability::NotPresent,
            Err(reason) => Availability::Unavailable { reason },
        }
    }
}
