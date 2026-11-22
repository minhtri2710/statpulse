use crate::hardware_telemetry::contract::{
    Availability, BatteryReading, FanReading, ProcessReading, TemperatureReadings,
};
use crate::hardware_telemetry::snapshot::{
    IokitTelemetryAdapter, ProcessTelemetryAdapter, SmcTelemetryAdapter,
};

#[derive(Clone)]
pub struct FixtureTelemetryAdapter {
    pub temperatures: Availability<TemperatureReadings>,
    pub fans: Availability<Vec<FanReading>>,
    pub battery: Availability<BatteryReading>,
    pub top_processes: Availability<Vec<ProcessReading>>,
}

impl SmcTelemetryAdapter for FixtureTelemetryAdapter {
    fn temperatures(&self) -> Availability<TemperatureReadings> {
        self.temperatures.clone()
    }

    fn fans(&self) -> Availability<Vec<FanReading>> {
        self.fans.clone()
    }
}

impl IokitTelemetryAdapter for FixtureTelemetryAdapter {
    fn battery(&self) -> Availability<BatteryReading> {
        self.battery.clone()
    }
}

impl ProcessTelemetryAdapter for FixtureTelemetryAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>> {
        self.top_processes.clone()
    }
}
