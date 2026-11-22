pub mod adapters;
pub mod contract;
pub mod snapshot;

use adapters::{iokit::IokitAdapter, process::ProcessAdapter, smc::SmcAdapter};
use contract::{
    Availability, FanActuationStatus, HardwareTelemetrySnapshot, ProcessReading,
};
use snapshot::{HardwareTelemetry, ProcessTelemetryAdapter};

impl ProcessTelemetryAdapter for ProcessAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>> {
        ProcessAdapter::top_processes()
    }
}

pub fn capture(
    fan_actuation_status: FanActuationStatus,
    captured_at_unix_ms: u64,
) -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcAdapter, IokitAdapter, ProcessAdapter)
        .capture(fan_actuation_status, captured_at_unix_ms)
}
