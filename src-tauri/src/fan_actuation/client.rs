use super::installer::{self, InstallStatus};
use super::protocol::{FanCommand, FanResponse};
use super::socket::{send_command, SOCKET_PATH};
use crate::hardware_telemetry::contract::FanActuationStatus;
use std::path::Path;
use std::sync::{Mutex, RwLock};

// Helper readiness has one shape crate-wide: artifact presence is resolved
// into the shared `FanActuationStatus` contract here and nowhere else.

static STATUS_CACHE: std::sync::OnceLock<Mutex<Option<(std::time::Instant, FanActuationStatus)>>> =
    std::sync::OnceLock::new();

const STATUS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

fn publish_status(fresh: FanActuationStatus) -> FanActuationStatus {
    if let Ok(mut guard) = STATUS_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some((std::time::Instant::now(), fresh.clone()));
    }
    fresh
}

/// Fresh probe that also refreshes the shared cache.
pub fn status() -> FanActuationStatus {
    publish_status(status_at(installer::status(), Path::new(SOCKET_PATH)))
}

/// Cached read for hot paths (telemetry capture): serves the last probe for
/// five seconds before paying for a socket round-trip.
pub fn cached_status() -> FanActuationStatus {
    let cache = STATUS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((ts, ref cached)) = *guard {
            if ts.elapsed() < STATUS_CACHE_TTL {
                return cached.clone();
            }
        }
    }
    status()
}

fn status_at(installed: InstallStatus, socket: &Path) -> FanActuationStatus {
    match installed {
        InstallStatus::NotInstalled => FanActuationStatus::NotRegistered,
        InstallStatus::Unavailable => FanActuationStatus::Unavailable,
        InstallStatus::Installed => match send_command(socket, &FanCommand::Status) {
            Ok(FanResponse::Ready { .. }) => FanActuationStatus::Ready,
            _ => FanActuationStatus::Unavailable,
        },
    }
}

// An in-flight application update suspends every mutating command: this
// process must never write fan state between the pre-exchange restore and its
// own exit, or undo what the child just configured.
struct ActuationGate(RwLock<bool>);

static GATE: ActuationGate = ActuationGate(RwLock::new(false));

impl ActuationGate {
    fn with_writer<T>(&self, send: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let state = self.0.read().unwrap();
        if *state {
            return Err("fan actuation is suspended during an application update".into());
        }
        send()
    }

    // The write lock drains in-flight sends before the restore runs, so no
    // command can interleave with the handoff; the gate only raises after a
    // confirmed restore.
    fn suspend(&self, restore: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let mut state = self.0.write().unwrap();
        if *state {
            return Err("a fan actuation handoff is already in progress".into());
        }
        restore()?;
        *state = true;
        Ok(())
    }

    fn release(&self) {
        *self.0.write().unwrap() = false;
    }
}

pub(crate) fn handoff<T>(action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    GATE.suspend(|| {
        match installer::status() {
            // No controllable helper exists to keep or undo state; a legacy
            // daemon answering an old endpoint is never contacted.
            InstallStatus::NotInstalled => Ok(()),
            InstallStatus::Unavailable => {
                Err("repair the Fan actuation helper before updating".into())
            }
            InstallStatus::Installed => {
                match send_command(Path::new(SOCKET_PATH), &FanCommand::RestoreAll) {
                    Ok(FanResponse::Ok) => Ok(()),
                    Ok(FanResponse::Error { message }) => Err(message),
                    Ok(_) => {
                        Err("fan actuation restore before the update was not confirmed".into())
                    }
                    // A confirmed-current helper that does not answer is
                    // indistinguishable from a broken install.
                    Err(_) => Err("repair the Fan actuation helper before updating".into()),
                }
            }
        }
    })?;
    // The write lock is released before the action runs, so an in-flight UI
    // command can never deadlock the gate.
    let result = action();
    if result.is_err() {
        GATE.release();
    }
    result
}

// A failed post-exchange step (e.g. the child never confirmed its UI) reopens
// the gate after the rollback restored the previous bundle.
pub(crate) fn release_handoff() {
    GATE.release();
}

pub fn command(command: FanCommand) -> Result<FanResponse, String> {
    let response = GATE.with_writer(|| send_command(Path::new(SOCKET_PATH), &command))?;
    match response {
        FanResponse::Error { message } => Err(message),
        response => Ok(response),
    }
}

pub fn set_target(fan_id: usize, rpm: i32) -> Result<(), String> {
    expect_ok(command(FanCommand::SetTarget { fan_id, rpm })?)
}

pub fn system_auto(fan_id: usize) -> Result<(), String> {
    expect_ok(command(FanCommand::SystemAuto { fan_id })?)
}

pub fn restore_all() -> Result<(), String> {
    expect_ok(command(FanCommand::RestoreAll)?)
}

pub fn heartbeat() -> Result<(), String> {
    expect_ok(command(FanCommand::Heartbeat)?)
}

fn expect_ok(response: FanResponse) -> Result<(), String> {
    if response == FanResponse::Ok {
        Ok(())
    } else {
        Err("unexpected response from Fan actuation service".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{status_at, ActuationGate};
    use crate::fan_actuation::installer::InstallStatus;
    use crate::fan_actuation::protocol::{FanCommand, FanResponse};
    use crate::hardware_telemetry::contract::FanActuationStatus;
    use std::io::{BufRead, BufReader, ErrorKind, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::RwLock;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn installed_service_without_the_endpoint_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("actuation.sock");
        let unrelated = UnixListener::bind(dir.path().join("unrelated.sock")).unwrap();
        unrelated.set_nonblocking(true).unwrap();

        assert_eq!(
            status_at(InstallStatus::Installed, &socket),
            FanActuationStatus::Unavailable
        );
        assert_eq!(
            unrelated.accept().unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
    }

    #[test]
    fn actuation_gate_suspends_writers_only_after_confirmed_restore() {
        let gate = ActuationGate(RwLock::new(false));
        let sends = std::cell::Cell::new(0);
        gate.with_writer(|| {
            sends.set(sends.get() + 1);
            Ok::<(), String>(())
        })
        .unwrap();

        // A failed restore never suspends the writers.
        assert!(gate.suspend(|| Err("restore refused".into())).is_err());
        gate.with_writer(|| {
            sends.set(sends.get() + 1);
            Ok::<(), String>(())
        })
        .unwrap();
        assert_eq!(sends.get(), 2);

        let restores = std::cell::Cell::new(0);
        gate.suspend(|| {
            restores.set(restores.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(restores.get(), 1);
        for _ in 0..2 {
            match gate.with_writer(|| Ok::<(), String>(())) {
                Err(error) => assert!(error.contains("suspended")),
                Ok(()) => panic!("mutating command bypassed the update gate"),
            }
        }
        assert_eq!(sends.get(), 2);
        // A second suspend while suspended is refused.
        assert!(gate.suspend(|| Ok(())).is_err());

        gate.release();
        gate.with_writer(|| {
            sends.set(sends.get() + 1);
            Ok::<(), String>(())
        })
        .unwrap();
        assert_eq!(sends.get(), 3);
    }

    #[test]
    fn installed_service_answering_ready_is_ready() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("actuation.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            assert_eq!(
                serde_json::from_str::<FanCommand>(&request).unwrap(),
                FanCommand::Status
            );
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut payload = serde_json::to_vec(&FanResponse::Ready { fans: vec![] }).unwrap();
            payload.push(b'\n');
            stream.write_all(&payload).unwrap();
        });

        assert_eq!(
            status_at(InstallStatus::Installed, &socket),
            FanActuationStatus::Ready
        );
        peer.join().unwrap();
    }
}
