pub mod adapters;
pub mod contract;
pub mod snapshot;

use adapters::{iokit::IokitAdapter, process::ProcessAdapter, smc::SmcAdapter};
use contract::{Availability, FanActuationStatus, HardwareTelemetrySnapshot, ProcessReading};
use snapshot::{HardwareTelemetry, ProcessTelemetryAdapter};

impl ProcessTelemetryAdapter for ProcessAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>> {
        ProcessAdapter::top_processes()
    }
}

struct CachedOnlyProcessAdapter;

impl ProcessTelemetryAdapter for CachedOnlyProcessAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>> {
        ProcessAdapter::cached_top_processes()
    }
}

pub fn capture(
    fan_actuation_status: FanActuationStatus,
    captured_at_unix_ms: u64,
) -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcAdapter, IokitAdapter, ProcessAdapter)
        .capture(fan_actuation_status, captured_at_unix_ms)
}

/// Capture stamped with the crate wall clock at call time.
pub fn capture_now(fan_actuation_status: FanActuationStatus) -> HardwareTelemetrySnapshot {
    capture(fan_actuation_status, crate::clock::now_unix_ms())
}

/// Background tray capture: reuses cached top processes if fresh, otherwise
/// skips spawning `/bin/ps` while the UI is hidden.
pub fn capture_tray_now(fan_actuation_status: FanActuationStatus) -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcAdapter, IokitAdapter, CachedOnlyProcessAdapter)
        .capture(fan_actuation_status, crate::clock::now_unix_ms())
}
