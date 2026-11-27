use statpulse_lib::fan_actuation::console_user::active_console_uid;
use statpulse_lib::fan_actuation::lease::ActuationLease;
use statpulse_lib::fan_actuation::migration::remove_legacy_helper;
use statpulse_lib::fan_actuation::protocol::{FanCommand, FanHardware, FanResponse};
use statpulse_lib::fan_actuation::smc_adapter::SmcFanHardware;
use statpulse_lib::fan_actuation::socket::{try_accept_once, wait_for_connection, SOCKET_PATH};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

fn main() {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("fan actuation daemon must run as root");
        std::process::exit(1);
    }

    if let Err(error) = run() {
        eprintln!("fan actuation daemon failed: {error}");
        std::process::exit(1);
    }
}

fn restore_all(
    lease: &mut ActuationLease,
    hardware: &mut impl FanHardware,
    restore_failed: &mut bool,
) -> bool {
    match lease.execute(FanCommand::RestoreAll, hardware, Instant::now) {
        FanResponse::Error { message } => {
            if !*restore_failed {
                eprintln!("fan actuation system auto recovery failed: {message}");
            }
            *restore_failed = true;
            false
        }
        _ => {
            if *restore_failed {
                eprintln!("fan actuation system auto recovery restored");
            }
            *restore_failed = false;
            true
        }
    }
}

#[allow(function_casts_as_integer)]
fn run() -> Result<(), String> {
    unsafe {
        libc::signal(libc::SIGTERM, handle_signal as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_signal as libc::sighandler_t);
    }

    remove_legacy_helper()?;

    let mut hardware = SmcFanHardware;
    let mut lease = ActuationLease::new(Duration::from_secs(5), Instant::now());
    let mut restore_failed = false;
    restore_all(&mut lease, &mut hardware, &mut restore_failed);

    let socket_dir = Path::new(SOCKET_PATH)
        .parent()
        .ok_or_else(|| format!("invalid socket path: {SOCKET_PATH}"))?;
    fs::create_dir_all(socket_dir).map_err(|error| error.to_string())?;
    fs::set_permissions(socket_dir, fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;

    let socket_path = Path::new(SOCKET_PATH);
    if socket_path.exists() {
        fs::remove_file(socket_path).map_err(|error| error.to_string())?;
    }

    let listener = UnixListener::bind(socket_path).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    // Console users can change without a daemon restart; peer UID authorization gates commands.
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o666))
        .map_err(|error| error.to_string())?;

    let mut console: Option<u32> = None;
    while !SHUTDOWN.load(Ordering::Relaxed) {
        let observed = active_console_uid().ok();
        if observed != console {
            // A console-user change invalidates both authorization and any
            // manual state written for the previous user; restore once per
            // transition instead of on every poll with no user.
            console = observed;
            restore_all(&mut lease, &mut hardware, &mut restore_failed);
        } else if lease.should_restore(Instant::now()) {
            restore_all(&mut lease, &mut hardware, &mut restore_failed);
        }

        let Some(authorized_uid) = observed else {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        };

        match try_accept_once(&listener, authorized_uid, &mut hardware, &mut lease) {
            Ok(true) => (),
            Ok(false) => {
                let _ = wait_for_connection(&listener, Duration::from_millis(100));
            }
            Err(error) => {
                restore_all(&mut lease, &mut hardware, &mut restore_failed);
                eprintln!("fan actuation request failed: {error}");
            }
        }
    }

    if !restore_all(&mut lease, &mut hardware, &mut restore_failed) {
        std::thread::sleep(Duration::from_millis(250));
        restore_all(&mut lease, &mut hardware, &mut restore_failed);
    }
    let restored = !restore_failed;
    let _ = fs::remove_file(socket_path);
    if !restored {
        return Err("final fan restore to system auto is unconfirmed".into());
    }
    Ok(())
}
