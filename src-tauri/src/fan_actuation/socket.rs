use super::lease::ActuationLease;
use super::protocol::{FanCommand, FanHardware, FanResponse};
use std::io::{BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};

// Only helpers that confirm firmware restoration bind this socket; daemons
// from earlier builds answering other endpoints are never contacted.
pub const SOCKET_PATH: &str = "/var/run/statpulse/confirmed-actuation.sock";

const MAX_REQUEST_BYTES: usize = 4096;
const IO_TIMEOUT: Duration = Duration::from_secs(12);
const SERVER_IO_TIMEOUT: Duration = Duration::from_secs(1);

pub fn send_command(path: &Path, command: &FanCommand) -> Result<FanResponse, String> {
    let mut stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;

    let mut payload = serde_json::to_vec(command).map_err(|error| error.to_string())?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .map_err(|error| error.to_string())?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| error.to_string())?;

    let response = read_message(&stream)?;
    serde_json::from_slice(&response).map_err(|error| error.to_string())
}

pub fn serve_connection(
    stream: UnixStream,
    authorized_uid: u32,
    hardware: &mut impl FanHardware,
    lease: &mut ActuationLease,
) -> Result<(), String> {
    serve_connection_with_timeout(stream, authorized_uid, hardware, lease, SERVER_IO_TIMEOUT)
}

fn serve_connection_with_timeout(
    mut stream: UnixStream,
    authorized_uid: u32,
    hardware: &mut impl FanHardware,
    lease: &mut ActuationLease,
    io_timeout: Duration,
) -> Result<(), String> {
    stream
        .set_write_timeout(Some(io_timeout))
        .map_err(|error| error.to_string())?;

    let peer_uid = peer_uid(&stream).map_err(|error| error.to_string())?;
    if peer_uid != authorized_uid {
        let _ = write_response(
            &mut stream,
            &FanResponse::Error {
                message: "fan actuation is limited to the active console user".into(),
            },
        );
        return Ok(());
    }

    let command = match read_message_with_timeout(&stream, io_timeout).and_then(|payload| {
        serde_json::from_slice::<FanCommand>(&payload).map_err(|e| e.to_string())
    }) {
        Ok(command) => command,
        Err(message) => {
            let _ = lease.execute(FanCommand::RestoreAll, hardware, Instant::now);
            write_response(&mut stream, &FanResponse::Error { message })?;
            return Ok(());
        }
    };

    let response = lease.execute(command, hardware, Instant::now);
    if let Err(error) = write_response(&mut stream, &response) {
        let _ = lease.execute(FanCommand::RestoreAll, hardware, Instant::now);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
pub fn accept_once(
    listener: &UnixListener,
    authorized_uid: u32,
    hardware: &mut impl FanHardware,
    lease: &mut ActuationLease,
) -> Result<(), String> {
    let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
    serve_connection(stream, authorized_uid, hardware, lease)
}

pub fn try_accept_once(
    listener: &UnixListener,
    authorized_uid: u32,
    hardware: &mut impl FanHardware,
    lease: &mut ActuationLease,
) -> Result<bool, String> {
    match listener.accept() {
        Ok((stream, _)) => {
            serve_connection(stream, authorized_uid, hardware, lease)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Wait up to `timeout` for a pending connection on `listener` via `poll(2)`.
pub fn wait_for_connection(listener: &UnixListener, timeout: Duration) -> Result<bool, String> {
    let mut pollfd = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let rc = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(err.to_string());
    }
    Ok(rc > 0 && (pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0)
}

fn read_message(stream: &UnixStream) -> Result<Vec<u8>, String> {
    let mut message = Vec::new();
    BufReader::new(stream)
        .take(MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut message)
        .map_err(|error| error.to_string())?;

    if message.len() > MAX_REQUEST_BYTES {
        return Err("fan actuation request is too large".into());
    }
    if message.last() != Some(&b'\n') {
        return Err("fan actuation request must end with a newline".into());
    }
    message.pop();
    Ok(message)
}

fn read_message_with_timeout(stream: &UnixStream, timeout: Duration) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let mut message = Vec::new();
    let mut chunk = [0; 512];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("fan actuation request timed out".into());
        }
        stream
            .set_read_timeout(Some(remaining.max(Duration::from_millis(1))))
            .map_err(|error| error.to_string())?;
        let count = (&*stream)
            .read(&mut chunk)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        message.extend_from_slice(&chunk[..count]);
        if message.len() > MAX_REQUEST_BYTES {
            return Err("fan actuation request is too large".into());
        }
    }
    if message.last() != Some(&b'\n') {
        return Err("fan actuation request must end with a newline".into());
    }
    message.pop();
    Ok(message)
}

fn write_response(stream: &mut UnixStream, response: &FanResponse) -> Result<(), String> {
    let mut payload = serde_json::to_vec(response).map_err(|error| error.to_string())?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .map_err(|error| error.to_string())
}

fn peer_uid(stream: &UnixStream) -> std::io::Result<u32> {
    let mut uid = 0;
    let mut gid = 0;
    // Apple documents getpeereid(3) as a reliable credential check for connected
    // SOCK_STREAM Unix-domain peers.
    // Source: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/getpeereid.3.html
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if result == 0 {
        Ok(uid)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::{accept_once, send_command, serve_connection_with_timeout};
    use crate::fan_actuation::lease::ActuationLease;
    use crate::fan_actuation::protocol::{FanCommand, FanEnvelope, FanHardware, FanResponse};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct FixtureHardware {
        fans: BTreeMap<usize, (i32, i32)>,
        targets: BTreeMap<usize, i32>,
        restored: Vec<usize>,
    }

    impl FixtureHardware {
        fn with_fan(mut self, id: usize, min_rpm: i32, max_rpm: i32) -> Self {
            self.fans.insert(id, (min_rpm, max_rpm));
            self
        }
    }

    impl FanHardware for FixtureHardware {
        fn envelopes(&self) -> Result<Vec<FanEnvelope>, String> {
            Ok(self
                .fans
                .iter()
                .map(|(id, (min_rpm, max_rpm))| FanEnvelope {
                    id: *id,
                    min_rpm: *min_rpm,
                    max_rpm: *max_rpm,
                })
                .collect())
        }

        fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String> {
            self.targets.insert(fan_id, rpm);
            Ok(())
        }

        fn system_auto(&mut self, fan_id: usize) -> Result<(), String> {
            self.restored.push(fan_id);
            Ok(())
        }
    }

    #[test]
    fn accepts_commands_from_authorized_peer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authorized.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        let server = thread::spawn(move || {
            let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
            let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
            accept_once(&listener, uid, &mut hardware, &mut lease).unwrap();
            hardware
        });

        let response = send_command(
            &path,
            &FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
        )
        .unwrap();

        let hardware = server.join().unwrap();
        assert_eq!(response, FanResponse::Ok);
        assert_eq!(hardware.targets.get(&0), Some(&3200));
    }

    #[test]
    fn rejects_commands_from_other_users() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unauthorized.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        let mut request = serde_json::to_vec(&FanCommand::SetTarget {
            fan_id: 0,
            rpm: 3200,
        })
        .unwrap();
        request.push(b'\n');
        client.write_all(&request).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
        let other_uid = unsafe { libc::geteuid() } ^ 1;
        accept_once(&listener, other_uid, &mut hardware, &mut lease).unwrap();
        let mut reply = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(client), &mut reply).unwrap();
        let response: FanResponse = serde_json::from_str(&reply).unwrap();
        assert!(matches!(response, FanResponse::Error { .. }));
        assert!(hardware.targets.is_empty());
        assert!(hardware.restored.is_empty());
    }

    #[test]
    fn idle_peer_releases_daemon_before_lease_expires() {
        let (server, client) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::geteuid() };
        let started = Instant::now();
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
        let _result = serve_connection_with_timeout(
            server,
            uid,
            &mut hardware,
            &mut lease,
            Duration::from_millis(60),
        );
        drop(client);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(hardware.restored, vec![0]);
    }

    #[test]
    fn slow_peer_cannot_extend_request_deadline() {
        let (server, mut client) = UnixStream::pair().unwrap();
        let uid = unsafe { libc::geteuid() };
        let (completion, finished) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
            let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
            let result = serve_connection_with_timeout(
                server,
                uid,
                &mut hardware,
                &mut lease,
                Duration::from_millis(60),
            );
            completion.send((result, hardware)).unwrap();
        });

        for _ in 0..6 {
            if client.write_all(b" ").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
        let (_result, hardware) = finished.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(client);
        worker.join().unwrap();
        assert_eq!(hardware.restored, vec![0]);
    }

    #[test]
    fn malformed_authorized_request_restores_system_auto() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("malformed.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        let server = thread::spawn(move || {
            let mut hardware = FixtureHardware::default()
                .with_fan(0, 1200, 6000)
                .with_fan(1, 1200, 6000);
            let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
            accept_once(&listener, uid, &mut hardware, &mut lease).unwrap();
            hardware
        });

        let mut stream = UnixStream::connect(&path).unwrap();
        stream.write_all(b"{not-json}\n").unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let hardware = server.join().unwrap();
        assert!(response.contains("error"));
        assert_eq!(hardware.restored, vec![0, 1]);
    }

    #[test]
    fn wait_for_connection_wakes_immediately_on_client_connect() {
        use super::wait_for_connection;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("poll-wake.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();

        assert!(!wait_for_connection(&listener, Duration::from_millis(5)).unwrap());

        let client_path = path.clone();
        let connector = thread::spawn(move || {
            thread::sleep(Duration::from_millis(15));
            UnixStream::connect(&client_path).unwrap()
        });

        let started = Instant::now();
        let ready = wait_for_connection(&listener, Duration::from_secs(2)).unwrap();
        let elapsed = started.elapsed();
        let _client = connector.join().unwrap();

        assert!(ready);
        assert!(
            elapsed < Duration::from_millis(500),
            "poll took too long to wake: {elapsed:?}"
        );
    }
}
