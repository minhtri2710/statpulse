pub mod adapters;
pub mod contract;
pub mod snapshot;

use adapters::{
    iokit::IokitAdapter,
    process::ProcessAdapter,
    smc::{SmcAdapter, SmcMetricsAdapter},
};
use contract::{
    Availability, BatteryReading, FanActuationStatus, HardwareTelemetrySnapshot, ProcessReading,
};
use snapshot::{HardwareTelemetry, IokitTelemetryAdapter, ProcessTelemetryAdapter};

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

struct NoopIokitAdapter;

impl IokitTelemetryAdapter for NoopIokitAdapter {
    fn battery(&self) -> Availability<BatteryReading> {
        Availability::Unavailable {
            reason: "skipped for fast metrics capture".into(),
        }
    }
}

struct NoopProcessAdapter;

impl ProcessTelemetryAdapter for NoopProcessAdapter {
    fn top_processes(&self) -> Availability<Vec<ProcessReading>> {
        Availability::Unavailable {
            reason: "skipped for fast metrics capture".into(),
        }
    }
}

pub fn capture(
    fan_actuation_status: FanActuationStatus,
    captured_at_unix_ms: u64,
) -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcAdapter, IokitAdapter, ProcessAdapter)
        .capture(fan_actuation_status, captured_at_unix_ms)
}

pub fn capture_now(fan_actuation_status: FanActuationStatus) -> HardwareTelemetrySnapshot {
    capture(fan_actuation_status, crate::clock::now_unix_ms())
}

/// Background tray capture: reuses cached top processes if fresh, otherwise
/// skips spawning `/bin/ps` while the UI is hidden.
pub fn capture_tray_now(fan_actuation_status: FanActuationStatus) -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcAdapter, IokitAdapter, CachedOnlyProcessAdapter)
        .capture(fan_actuation_status, crate::clock::now_unix_ms())
}

/// Fast metrics/CSV capture: reads only CPU/GPU temperatures and fan RPMs without
/// chassis SMC sensors, fan metadata, IOKit battery, process snapshots, or daemon RPC.
pub fn capture_metrics_now() -> HardwareTelemetrySnapshot {
    HardwareTelemetry::new(SmcMetricsAdapter, NoopIokitAdapter, NoopProcessAdapter)
        .capture(FanActuationStatus::Unavailable, crate::clock::now_unix_ms())
}
