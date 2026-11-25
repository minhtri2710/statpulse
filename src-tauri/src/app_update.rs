use rustix::fs::{self, AtFlags, Mode, OFlags, RenameFlags};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const RELEASE_API_URL: &str = "https://api.github.com/repos/minhtri2710/statpulse/releases/latest";
const RELEASE_DOWNLOAD_BASE: &str = "https://github.com/minhtri2710/statpulse/releases/download";
const EXPECTED_IDENTIFIER: &str = env!("STATPULSE_BUNDLE_ID");
const CURRENT_VERSION: &str = env!("STATPULSE_VERSION");
const MAX_RELEASE_BYTES: u64 = 2_097_152;
const MAX_DMG_BYTES: u64 = 536_870_912;
const DOWNLOAD_TIMEOUT_SECS: &str = "600";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

const DIR_TRUST: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC);

#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub digest: Option<String>,
    pub state: String,
}

// The downloaded image lives on an anonymous tempfile: nothing queued is left
// behind by "Later", an exit, or a crash.
struct PreparedUpdate {
    version: String,
    digest: String,
    dmg: std::fs::File,
}

struct PreparedSlot(Mutex<Option<PreparedUpdate>>);

impl PreparedSlot {
    // The fetch closure is the only producer; an already-prepared version
    // returns without running it.
    fn prepare(
        &self,
        version: &str,
        fetch: impl FnOnce() -> Result<PreparedUpdate, String>,
    ) -> Result<(), String> {
        let mut slot = self.0.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|prepared| prepared.version == version)
        {
            return Ok(());
        }
        *slot = Some(fetch()?);
        Ok(())
    }

    fn discard(&self, version: &str) {
        let mut slot = self.0.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|prepared| prepared.version == version)
        {
            *slot = None;
        }
    }
}

static PREPARED: PreparedSlot = PreparedSlot(Mutex::new(None));

struct AppContext {
    app_path: PathBuf,
    app_name: std::ffi::OsString,
    parent: PathBuf,
    exe_name: String,
    dev: u64,
    ino: u64,
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

/// Canonical stable semver for a version input: parses and rejects any
/// pre-release/build suffix or non-canonical spelling. Check and prepare
/// share this gate; the newer-than-current comparison stays separate.
fn stable_version(input: &str) -> Result<semver::Version, String> {
    let parsed = semver::Version::parse(input)
        .map_err(|_| format!("version {input} is not canonical semver"))?;
    if parsed.to_string() != input || !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err(format!("version {input} is not a stable release"));
    }
    Ok(parsed)
}

fn target_version(version: &str) -> Result<semver::Version, String> {
    let parsed = stable_version(version)?;
    let current = semver::Version::parse(CURRENT_VERSION)
        .map_err(|_| "current version is not semver".to_string())?;
    if parsed <= current {
        return Err(format!(
            "update version {version} is not newer than {CURRENT_VERSION}"
        ));
    }
    Ok(parsed)
}

fn resolve_release(release: &Release, version: &str) -> Result<(String, String), String> {
    target_version(version)?;
    if release.draft || release.prerelease {
        return Err("latest release is not a published stable release".into());
    }
    if release.tag_name != format!("v{version}") {
        return Err(format!(
            "latest release tag {} does not match v{version}",
            release.tag_name
        ));
    }
    let expected_name = format!("StatPulse_{version}_universal.dmg");
    let expected_url = format!("{RELEASE_DOWNLOAD_BASE}/v{version}/{expected_name}");
    let matches: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|asset| asset.name == expected_name)
        .collect();
    let [asset] = matches.as_slice() else {
        return Err(format!(
            "release v{version} does not contain exactly one {expected_name} asset"
        ));
    };
    if asset.state != "uploaded" {
        return Err(format!("asset {expected_name} is not fully uploaded"));
    }
    if asset.browser_download_url != expected_url {
        return Err(format!(
            "asset {expected_name} has an unexpected download URL"
        ));
    }
    let Some(digest) = asset.digest.as_deref() else {
        return Err(format!("asset {expected_name} has no digest"));
    };
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return Err(format!("asset {expected_name} digest is not sha256"));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("asset {expected_name} digest is malformed"));
    }
    Ok((hex.to_string(), expected_url))
}

// One verified streaming copier: bounds the byte count before any write could
// exceed the ceiling, hashes in the same pass, and requires an exact lowercase
// SHA-256 match.
fn copy_verified(
    mut input: impl Read,
    mut out: impl Write,
    expected: &str,
    max: u64,
) -> Result<(), String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| format!("failed to read the update package: {error}"))?;
        if count == 0 {
            break;
        }
        let next = total.saturating_add(count as u64);
        if next > max {
            return Err("update package exceeds the maximum size".into());
        }
        hasher.update(&buffer[..count]);
        out.write_all(&buffer[..count])
            .map_err(|error| format!("failed to write the update package: {error}"))?;
        total = next;
    }
    if hex_lower(&hasher.finalize()) != expected {
        return Err("downloaded update failed digest verification".into());
    }
    Ok(())
}

fn read_bounded(mut input: impl Read, max: u64) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| format!("failed to read release metadata: {error}"))?;
        if count == 0 {
            break;
        }
        if body.len() as u64 + count as u64 > max {
            return Err("release metadata exceeded the size limit".into());
        }
        body.extend_from_slice(&buffer[..count]);
    }
    Ok(body)
}

fn fetch_release() -> Result<Release, String> {
    // curl's --max-filesize cannot bound chunked responses, so the stream is
    // capped independently of the flag.
    let max_release_bytes = MAX_RELEASE_BYTES.to_string();
    let mut child = Command::new("/usr/bin/curl")
        .arg("-q")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            &max_release_bytes,
            RELEASE_API_URL,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to query the latest release: {error}"))?;
    let stdout = child.stdout.take().expect("curl stdout is piped");
    let body = match read_bounded(stdout, MAX_RELEASE_BYTES) {
        Ok(body) => body,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    if !child
        .wait()
        .map_err(|error| format!("failed to query the latest release: {error}"))?
        .success()
    {
        return Err("failed to query the latest release".into());
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("latest release response is malformed: {error}"))
}

fn download(url: &str, expected_digest: &str) -> Result<std::fs::File, String> {
    let mut dmg = tempfile::tempfile()
        .map_err(|error| format!("failed to allocate update download: {error}"))?;
    let max_dmg_bytes = MAX_DMG_BYTES.to_string();
    let mut child = Command::new("/usr/bin/curl")
        .arg("-q")
        .args([
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            DOWNLOAD_TIMEOUT_SECS,
            "--max-filesize",
            &max_dmg_bytes,
            url,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to download update: {error}"))?;
    let stdout = child.stdout.take().expect("curl stdout is piped");
    if let Err(error) = copy_verified(stdout, &mut dmg, expected_digest, MAX_DMG_BYTES) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    if !child
        .wait()
        .map_err(|error| format!("failed to download update: {error}"))?
        .success()
    {
        return Err("failed to download the update package".into());
    }
    Ok(dmg)
}

fn bundle_metadata(app_dir: &Path) -> Result<(String, String, String), String> {
    let contents = app_dir.join("Contents");
    let macos = contents.join("MacOS");
    let info = contents.join("Info.plist");
    for (path, directory) in [
        (app_dir, true),
        (&contents, true),
        (&macos, true),
        (&info, false),
    ] {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{} must not be a symlink", path.display()));
        }
        if directory && !metadata.is_dir() {
            return Err(format!("{} is not a directory", path.display()));
        }
        if !directory && !metadata.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
    }
    let value = plist::Value::from_file(&info)
        .map_err(|error| format!("failed to read bundle metadata: {error}"))?;
    let dictionary = value
        .as_dictionary()
        .ok_or_else(|| "bundle Info.plist is not a dictionary".to_string())?;
    let get = |key: &str| {
        dictionary
            .get(key)
            .and_then(|value| value.as_string())
            .map(|value| value.to_string())
            .ok_or_else(|| format!("bundle Info.plist is missing {key}"))
    };
    Ok((
        get("CFBundleIdentifier")?,
        get("CFBundleShortVersionString")?,
        get("CFBundleExecutable")?,
    ))
}

fn validate_bundle_metadata(
    app_dir: &Path,
    expected_version: &str,
    expected_exe: &str,
) -> Result<(), String> {
    let (identifier, version, executable) = bundle_metadata(app_dir)?;
    if identifier != EXPECTED_IDENTIFIER {
        return Err(format!(
            "bundle identifier {identifier} is not {EXPECTED_IDENTIFIER}"
        ));
    }
    if version != expected_version {
        return Err(format!(
            "bundle version {version} does not match {expected_version}"
        ));
    }
    if executable != expected_exe || executable.contains('/') || executable.contains('\\') {
        return Err("bundle executable does not match the running product".into());
    }
    let exe_path = app_dir.join("Contents/MacOS").join(&executable);
    let metadata = std::fs::symlink_metadata(&exe_path)
        .map_err(|error| format!("failed to inspect bundle executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("bundle executable is not a regular file".into());
    }
    Ok(())
}

fn current_app() -> Result<AppContext, String> {
    let exe = std::fs::canonicalize(std::env::current_exe().map_err(|error| error.to_string())?)
        .map_err(|error| format!("failed to resolve the running executable: {error}"))?;
    app_context(&exe)
}

fn app_context(exe: &Path) -> Result<AppContext, String> {
    let metadata = std::fs::symlink_metadata(exe).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("running executable is not a regular file".into());
    }
    let exe_name = exe
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "running executable name is invalid".to_string())?
        .to_string();
    let macos = exe
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "MacOS"))
        .ok_or_else(|| "running executable is not inside Contents/MacOS".to_string())?;
    let contents = macos
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "Contents"))
        .ok_or_else(|| "running executable is not inside an app bundle".to_string())?;
    let app_path = contents
        .parent()
        .filter(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        .ok_or_else(|| "running executable is not inside an app bundle".to_string())?
        .to_path_buf();
    validate_bundle_metadata(&app_path, CURRENT_VERSION, &exe_name)?;
    let app_name = app_path
        .file_name()
        .ok_or_else(|| "application bundle name is invalid".to_string())?
        .to_os_string();
    let parent = app_path
        .parent()
        .ok_or_else(|| "application bundle has no parent".to_string())?
        .to_path_buf();
    let stat = std::fs::symlink_metadata(&app_path).map_err(|error| error.to_string())?;
    Ok(AppContext {
        app_path,
        app_name,
        parent,
        exe_name,
        dev: stat.dev(),
        ino: stat.ino(),
    })
}

// The mount point is a plain path, never a TempDir: recursive cleanup on a
// still-attached volume must never run, and a detach failure keeps the dir so
// the operator can `hdiutil detach` it manually.
struct Mount {
    dir: PathBuf,
}

fn mount_dmg(dmg: &tempfile::NamedTempFile) -> Result<Mount, String> {
    let dir = tempfile::Builder::new()
        .prefix(".statpulse-mount-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|error| format!("failed to allocate update mount point: {error}"))?
        .keep();
    let status = Command::new("/usr/bin/hdiutil")
        .args([
            "attach",
            "-readonly",
            "-nobrowse",
            "-noautoopen",
            "-mountpoint",
        ])
        .arg(&dir)
        .arg(dmg.path())
        .status();
    match status {
        Ok(status) if status.success() => Ok(Mount { dir }),
        Ok(_) => {
            let _ = std::fs::remove_dir(&dir);
            Err("failed to mount the update package".into())
        }
        Err(error) => {
            let _ = std::fs::remove_dir(&dir);
            Err(format!("failed to mount update package: {error}"))
        }
    }
}

fn hdiutil_detach(dir: &Path) -> Result<(), String> {
    let status = Command::new("/usr/bin/hdiutil")
        .args(["detach", "-quiet"])
        .arg(dir)
        .status()
        .map_err(|error| format!("failed to detach update package: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("failed to detach the update package".into())
    }
}

fn detach_dmg(
    mount: Mount,
    detach: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    match detach(&mount.dir) {
        Ok(()) => std::fs::remove_dir(&mount.dir).map_err(|error| {
            format!(
                "failed to remove update mount point {}: {error}",
                mount.dir.display()
            )
        }),
        Err(error) => Err(format!(
            "{error}; detach it manually with `hdiutil detach {}`",
            mount.dir.display()
        )),
    }
}

fn mounted_app(mount: &Mount, version: &str, exe_name: &str) -> Result<PathBuf, String> {
    let source = mount.dir.join("StatPulse.app");
    let metadata = std::fs::symlink_metadata(&source)
        .map_err(|error| format!("update package has no StatPulse.app: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("update package StatPulse.app is not a directory".into());
    }
    validate_bundle_metadata(&source, version, exe_name)?;
    Ok(source)
}

fn stage_app(source: &Path, app: &AppContext, version: &str) -> Result<tempfile::TempDir, String> {
    let stage = tempfile::Builder::new()
        .prefix(".statpulse-update-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(&app.parent)
        .map_err(|error| format!("failed to allocate update staging directory: {error}"))?;
    let destination = stage.path().join("StatPulse.app");
    let status = Command::new("/usr/bin/ditto")
        .arg(source)
        .arg(&destination)
        .status()
        .map_err(|error| format!("failed to stage update: {error}"))?;
    if !status.success() {
        return Err("failed to stage the update package".into());
    }
    validate_bundle_metadata(&destination, version, &app.exe_name)?;
    let status = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&destination)
        .status()
        .map_err(|error| format!("failed to verify update signature: {error}"))?;
    if !status.success() {
        return Err("downloaded application failed code signature verification".into());
    }
    Ok(stage)
}

fn open_dir(path: &Path) -> Result<OwnedFd, String> {
    fs::openat(fs::CWD, path, DIR_TRUST, Mode::empty())
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

pub(crate) fn prepare(version: &str) -> Result<(), String> {
    target_version(version)?;
    PREPARED.prepare(version, || {
        let release = fetch_release()?;
        let (digest, url) = resolve_release(&release, version)?;
        let dmg = download(&url, &digest)?;
        Ok(PreparedUpdate {
            version: version.to_string(),
            digest,
            dmg,
        })
    })
}

// "Later" frees the verified bytes while the app stays open; only the version
// the UI actually saw is dropped so a stale dismissal can never discard a
// newer prepared update.
pub(crate) fn discard(version: &str) -> Result<(), String> {
    PREPARED.discard(version);
    Ok(())
}

#[derive(Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct UpdateOffer {
    pub version: String,
    pub name: String,
    pub body: String,
    pub published_at: Option<String>,
    pub release_url: String,
}

/// Bounded query of the official latest release: the same asset/digest rules
/// as prepare, so an offer can never point at bytes install would reject.
/// `Ok(None)` means the installed build is current.
pub(crate) fn check_for_update() -> Result<Option<UpdateOffer>, String> {
    offer_for(fetch_release()?)
}

/// Pure offer seam over an already-fetched release: rejects non-v tags,
/// non-canonical semver, and drafts/prereleases even when not newer; an
/// older or equal tag returns `Ok(None)`, a newer one must still carry the
/// official matching asset/digest.
fn offer_for(release: Release) -> Result<Option<UpdateOffer>, String> {
    let Some(version) = release.tag_name.strip_prefix('v') else {
        return Err(format!(
            "latest release tag {} is not a v-prefixed tag",
            release.tag_name
        ));
    };
    let parsed = stable_version(version)
        .map_err(|error| format!("latest release tag {}: {error}", release.tag_name))?;
    if release.draft || release.prerelease {
        return Err("latest release is not a published stable release".into());
    }
    let current = semver::Version::parse(CURRENT_VERSION)
        .map_err(|_| "current version is not semver".to_string())?;
    if parsed <= current {
        return Ok(None);
    }
    // Same canonical-asset/digest gate as prepare; the digest itself is not
    // needed until the download starts.
    resolve_release(&release, version)?;
    let tag = format!("v{version}");
    Ok(Some(UpdateOffer {
        version: version.to_string(),
        name: release.name.unwrap_or_else(|| tag.clone()),
        body: release
            .body
            .unwrap_or_else(|| "No release notes provided.".to_string()),
        published_at: release.published_at,
        release_url: format!("https://github.com/minhtri2710/statpulse/releases/tag/{tag}"),
    }))
}

// The exchange plus the handshake channel happen under the client handoff;
// the ready wait does not, so an in-flight UI command can never deadlock the
// gate.
struct UpdateActivation {
    parent: OwnedFd,
    app_name: std::ffi::OsString,
    stage_root: OwnedFd,
    stage: tempfile::TempDir,
    child: Child,
    ready: UnixStream,
}

fn spawn_handshake_child(
    command: &mut Command,
    timeout: Duration,
) -> Result<(Child, UnixStream), String> {
    let (reader, writer) = UnixStream::pair()
        .map_err(|error| format!("failed to create update handshake channel: {error}"))?;
    reader
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("failed to configure update handshake: {error}"))?;
    let fd: OwnedFd = writer.into();
    command.env("STATPULSE_UPDATE_HANDSHAKE", "1");
    command.stdin(Stdio::from(fd));
    command
        .spawn()
        .map(|child| (child, reader))
        .map_err(|error| format!("failed to launch updated application: {error}"))
}

// A successful upgrade is proven by one exact ready byte over the private
// socket while the child is still alive; EOF, a timeout, a wrong byte, or an
// early exit all mean the new UI never started.
fn wait_ready(reader: &mut UnixStream, child: &mut Child) -> Result<(), String> {
    let mut byte = [0_u8; 1];
    match reader.read_exact(&mut byte) {
        Ok(()) if byte == *b"R" => {}
        Ok(()) => return Err("updated application sent an invalid startup signal".into()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Err("updated application did not confirm startup in time".into());
        }
        Err(error) => {
            return Err(format!("failed to read the update startup signal: {error}"));
        }
    }
    if child
        .try_wait()
        .map_err(|error| format!("failed to inspect the updated application: {error}"))?
        .is_some()
    {
        return Err("updated application exited before confirming startup".into());
    }
    Ok(())
}

// The owned child must be confirmed stopped before rollback swaps the bundle
// back: a live child could still be running the replaced executable.
fn reap_child(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| format!("failed to inspect the updated application: {error}"))?
        .is_none()
    {
        child
            .kill()
            .map_err(|error| format!("failed to stop the updated application: {error}"))?;
    }
    child
        .wait()
        .map_err(|error| format!("failed to reap the updated application: {error}"))?;
    Ok(())
}

// Child side of the handshake: send one exact byte on stdin only when stdin
// really is the inherited Unix socket pair end — never on arbitrary fds.
#[cfg(target_os = "macos")]
pub(crate) fn signal_update_ready() {
    unsafe {
        let mut socket_type: libc::c_int = 0;
        let mut type_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        if libc::getsockopt(
            libc::STDIN_FILENO,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            &mut socket_type as *mut _ as *mut libc::c_void,
            &mut type_len,
        ) != 0
            || socket_type != libc::SOCK_STREAM
        {
            return;
        }
        let mut address: libc::sockaddr_un = std::mem::zeroed();
        let mut address_len = std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t;
        if libc::getsockname(
            libc::STDIN_FILENO,
            &mut address as *mut _ as *mut libc::sockaddr,
            &mut address_len,
        ) != 0
            || i32::from(address.sun_family) != libc::AF_UNIX
        {
            return;
        }
        let byte = *b"R";
        let _ = libc::send(
            libc::STDIN_FILENO,
            byte.as_ptr() as *const libc::c_void,
            1,
            0,
        );
    }
}

fn rollback_activation(
    parent: &impl AsFd,
    app_name: &std::ffi::OsStr,
    stage_root: &impl AsFd,
    stage: tempfile::TempDir,
    launch_error: String,
) -> String {
    match fs::renameat_with(
        stage_root,
        "StatPulse.app",
        parent,
        app_name,
        RenameFlags::EXCHANGE,
    ) {
        Ok(()) => format!(
            "updated application failed to launch; previous version restored: {launch_error}"
        ),
        Err(rollback) => {
            let kept = stage.keep();
            format!(
                "updated application failed to launch ({launch_error}) and rollback failed ({rollback}); previous version preserved at {}",
                kept.display()
            )
        }
    }
}

fn activate_update(app: &AppContext, stage: tempfile::TempDir) -> Result<UpdateActivation, String> {
    let parent = open_dir(&app.parent)?;
    let stage_root = open_dir(stage.path())?;
    let current = fs::statat(&parent, &app.app_name, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|error| format!("application bundle changed during update: {error}"))?;
    if current.st_dev as u64 != app.dev || current.st_ino != app.ino {
        return Err("application bundle changed during update".into());
    }
    fs::renameat_with(
        &parent,
        &app.app_name,
        &stage_root,
        "StatPulse.app",
        RenameFlags::EXCHANGE,
    )
    .map_err(|error| format!("failed to exchange application bundle: {error}"))?;
    let exe_path = app.app_path.join("Contents/MacOS").join(&app.exe_name);
    match spawn_handshake_child(&mut Command::new(&exe_path), HANDSHAKE_TIMEOUT) {
        Ok((child, ready)) => Ok(UpdateActivation {
            parent,
            app_name: app.app_name.clone(),
            stage_root,
            stage,
            child,
            ready,
        }),
        Err(error) => Err(rollback_activation(
            &parent,
            &app.app_name,
            &stage_root,
            stage,
            error,
        )),
    }
}

pub(crate) fn install(version: &str) -> Result<(), String> {
    let mut slot = PREPARED.0.lock().unwrap();
    let Some(prepared) = slot.as_ref() else {
        return Err("the update has not finished downloading yet".into());
    };
    if prepared.version != version {
        return Err("prepared update does not match the requested version".into());
    }
    let mut source = prepared
        .dmg
        .try_clone()
        .map_err(|error| format!("failed to reopen the prepared update: {error}"))?;
    source
        .rewind()
        .map_err(|error| format!("failed to rewind the prepared update: {error}"))?;
    // hdiutil needs a real filesystem path, so the anonymous image is
    // re-verified into a fresh named scratch file for this install only.
    let mut verified = tempfile::NamedTempFile::new()
        .map_err(|error| format!("failed to allocate verified update copy: {error}"))?;
    copy_verified(&mut source, &mut verified, &prepared.digest, MAX_DMG_BYTES)?;
    let app = current_app()?;
    let mount = mount_dmg(&verified)?;
    let staging = (|| {
        let source = mounted_app(&mount, &prepared.version, &app.exe_name)?;
        stage_app(&source, &app, &prepared.version)
    })();
    let detached = detach_dmg(mount, hdiutil_detach);
    drop(verified);
    let stage = match (staging, detached) {
        (Ok(stage), Ok(())) => stage,
        (staging, detached) => {
            let mut errors = Vec::new();
            if let Err(error) = staging {
                errors.push(error);
            }
            if let Err(error) = detached {
                errors.push(error);
            }
            return Err(errors.join("; "));
        }
    };
    let activation = crate::fan_actuation::client::handoff(|| activate_update(&app, stage))?;
    let UpdateActivation {
        parent,
        app_name,
        stage_root,
        stage,
        mut child,
        mut ready,
    } = activation;
    if let Err(error) = wait_ready(&mut ready, &mut child) {
        if let Err(reap_error) = reap_child(&mut child) {
            // The old process stays suspended: rolling back under a child
            // that cannot be confirmed stopped is not safe, so the previous
            // version is preserved in place rather than deleted.
            let kept = stage.keep();
            return Err(format!(
                "updated application failed to start ({error}) and could not be stopped ({reap_error}); previous version preserved at {}",
                kept.display()
            ));
        }
        let rollback_error = rollback_activation(&parent, &app_name, &stage_root, stage, error);
        crate::fan_actuation::client::release_handoff();
        return Err(rollback_error);
    }
    *slot = None;
    Ok(())
}

// Two-phase startup acknowledgement for the update handshake: the ready
// byte is sent exactly once, only after native setup completed AND the UI
// mounted, in either arrival order. Tauri entry points stay as thin
// adapters over these calls.

// Set when this process was launched as an update payload; the marker is
// captured before any spawned child can inherit it.
static UPDATE_HANDSHAKE_PENDING: AtomicBool = AtomicBool::new(false);

const STARTUP_NATIVE_READY: u8 = 1;
const STARTUP_UI_READY: u8 = 2;
const STARTUP_BOTH_READY: u8 = STARTUP_NATIVE_READY | STARTUP_UI_READY;

static STARTUP_BARRIER: AtomicU8 = AtomicU8::new(0);

fn mark_startup_ready(state: &AtomicU8, bit: u8) -> bool {
    let previous = state.fetch_or(bit, Ordering::SeqCst);
    previous != STARTUP_BOTH_READY && (previous | bit) == STARTUP_BOTH_READY
}

fn startup_ready(bit: u8) {
    if mark_startup_ready(&STARTUP_BARRIER, bit)
        && UPDATE_HANDSHAKE_PENDING.swap(false, Ordering::SeqCst)
    {
        signal_update_ready();
    }
}

/// Captured (and scrubbed) on the main thread before the runtime spawns
/// anything that could inherit the marker.
pub(crate) fn capture_launch_marker() {
    let update_handshake =
        std::env::var_os("STATPULSE_UPDATE_HANDSHAKE").is_some_and(|value| value == "1");
    std::env::remove_var("STATPULSE_UPDATE_HANDSHAKE");
    UPDATE_HANDSHAKE_PENDING.store(update_handshake, Ordering::SeqCst);
}

/// Called once all fallible native setup work is complete.
pub(crate) fn native_ready() {
    startup_ready(STARTUP_NATIVE_READY);
}

/// Invoked once by the UI after React has committed; proves the updated app
/// reached a running UI to the parent waiting on the handshake socket.
pub(crate) fn ui_ready() {
    startup_ready(STARTUP_UI_READY);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn signal_update_ready() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_barrier_releases_once_for_either_order() {
        let state = AtomicU8::new(0);
        assert!(!mark_startup_ready(&state, STARTUP_NATIVE_READY));
        // A duplicate arrival can never fire the acknowledgement twice.
        assert!(!mark_startup_ready(&state, STARTUP_NATIVE_READY));
        assert!(mark_startup_ready(&state, STARTUP_UI_READY));
        assert!(!mark_startup_ready(&state, STARTUP_UI_READY));
        assert!(!mark_startup_ready(&state, STARTUP_NATIVE_READY));

        let state = AtomicU8::new(0);
        assert!(!mark_startup_ready(&state, STARTUP_UI_READY));
        assert!(mark_startup_ready(&state, STARTUP_NATIVE_READY));
    }

    use std::io::Write;

    fn release_json(body: &str) -> Release {
        serde_json::from_str(body).unwrap()
    }

    fn release_with_asset(
        tag: &str,
        draft: bool,
        prerelease: bool,
        name: &str,
        url: &str,
        digest: Option<&str>,
        state: &str,
    ) -> Release {
        let digest_field = digest
            .map(|d| format!("\"{d}\""))
            .unwrap_or_else(|| "null".into());
        release_json(&format!(
            r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{prerelease},"assets":[{{"name":"{name}","browser_download_url":"{url}","digest":{digest_field},"state":"{state}"}}]}}"#
        ))
    }

    fn next_version() -> String {
        let current = semver::Version::parse(CURRENT_VERSION).unwrap();
        format!("{}.{}.{}", current.major, current.minor, current.patch + 1)
    }

    fn digest_for(bytes: &[u8]) -> String {
        hex_lower(&Sha256::digest(bytes))
    }

    #[test]
    fn resolve_release_accepts_exact_newer_uploaded_asset() {
        let version = next_version();
        let digest = "a".repeat(64);
        let url = format!("{RELEASE_DOWNLOAD_BASE}/v{version}/StatPulse_{version}_universal.dmg");
        let release = release_with_asset(
            &format!("v{version}"),
            false,
            false,
            &format!("StatPulse_{version}_universal.dmg"),
            &url,
            Some(&format!("sha256:{digest}")),
            "uploaded",
        );

        let (resolved_digest, resolved_url) = resolve_release(&release, &version).unwrap();
        assert_eq!(resolved_digest, digest);
        assert_eq!(resolved_url, url);

        // Uppercase hex is not canonical; it is malformed, not normalized.
        let release = release_with_asset(
            &format!("v{version}"),
            false,
            false,
            &format!("StatPulse_{version}_universal.dmg"),
            &url,
            Some(&format!("sha256:{}", digest.to_uppercase())),
            "uploaded",
        );
        assert!(resolve_release(&release, &version).is_err());
    }

    #[test]
    fn resolve_release_rejects_wrong_filename_repo_and_state() {
        let version = next_version();
        let digest = "b".repeat(64);
        let url = format!("{RELEASE_DOWNLOAD_BASE}/v{version}/StatPulse_{version}_universal.dmg");
        for (name, asset_url) in [
            (format!("StatPulse_{version}_universal.dmg.beta"), url.clone()),
            (
                format!("StatPulse_{version}_universal.dmg"),
                format!("https://github.com/minhtri2710/not-statpulse/releases/download/v{version}/StatPulse_{version}_universal.dmg"),
            ),
        ] {
            let release = release_with_asset(
                &format!("v{version}"),
                false,
                false,
                &name,
                &asset_url,
                Some(&format!("sha256:{digest}")),
                "uploaded",
            );
            assert!(resolve_release(&release, &version).is_err());
        }
        let release = release_with_asset(
            &format!("v{version}"),
            false,
            false,
            &format!("StatPulse_{version}_universal.dmg"),
            &url,
            Some(&format!("sha256:{digest}")),
            "staged",
        );
        assert!(resolve_release(&release, &version).is_err());
    }

    #[test]
    fn offer_for_distinguishes_old_invalid_newer_and_wrong_asset() {
        let digest = "d".repeat(64);
        let version = next_version();
        let name = format!("StatPulse_{version}_universal.dmg");
        let url = format!("{RELEASE_DOWNLOAD_BASE}/v{version}/{name}");

        // An older or equal published tag offers nothing.
        let old = release_with_asset(
            &format!("v{CURRENT_VERSION}"),
            false,
            false,
            &format!("StatPulse_{CURRENT_VERSION}_universal.dmg"),
            &format!(
                "{RELEASE_DOWNLOAD_BASE}/v{CURRENT_VERSION}/StatPulse_{CURRENT_VERSION}_universal.dmg"
            ),
            Some(&format!("sha256:{digest}")),
            "uploaded",
        );
        assert!(offer_for(old).unwrap().is_none());

        // Drafts/prereleases and malformed or old non-canonical tags are
        // errors, never a silent "no update".
        for release in [
            release_with_asset(
                &format!("v{version}"),
                true,
                false,
                &name,
                &url,
                Some(&format!("sha256:{digest}")),
                "uploaded",
            ),
            release_with_asset(
                &format!("v{version}"),
                false,
                true,
                &name,
                &url,
                Some(&format!("sha256:{digest}")),
                "uploaded",
            ),
            release_with_asset(
                &format!("v{CURRENT_VERSION}-beta.1"),
                false,
                false,
                &name,
                &url,
                Some(&format!("sha256:{digest}")),
                "uploaded",
            ),
        ] {
            assert!(offer_for(release).is_err());
        }

        // A newer published release with the official digest offers an update.
        let newer = release_with_asset(
            &format!("v{version}"),
            false,
            false,
            &name,
            &url,
            Some(&format!("sha256:{digest}")),
            "uploaded",
        );
        let offer = offer_for(newer).unwrap().expect("newer release offers");
        assert_eq!(offer.version, version);
        assert_eq!(
            offer.release_url,
            format!("https://github.com/minhtri2710/statpulse/releases/tag/v{version}")
        );

        // The wrong asset never produces an offer.
        let wrong = release_with_asset(
            &format!("v{version}"),
            false,
            false,
            &format!("StatPulse_{version}_universal.dmg.bak"),
            &url,
            Some(&format!("sha256:{digest}")),
            "uploaded",
        );
        assert!(offer_for(wrong).is_err());
    }

    #[test]
    fn resolve_release_rejects_old_draft_and_missing_or_malformed_digest() {
        let version = next_version();
        let url = format!("{RELEASE_DOWNLOAD_BASE}/v{version}/StatPulse_{version}_universal.dmg");
        let name = format!("StatPulse_{version}_universal.dmg");
        let digest = "c".repeat(64);

        let old = release_with_asset(
            &format!("v{CURRENT_VERSION}"),
            false,
            false,
            &format!("StatPulse_{CURRENT_VERSION}_universal.dmg"),
            &format!("{RELEASE_DOWNLOAD_BASE}/v{CURRENT_VERSION}/StatPulse_{CURRENT_VERSION}_universal.dmg"),
            Some(&format!("sha256:{digest}")),
            "uploaded",
        );
        assert!(resolve_release(&old, CURRENT_VERSION).is_err());
        assert!(resolve_release(&old, "1.0.0").is_err());

        for (draft, prerelease, digest_value) in [
            (true, false, Some(format!("sha256:{digest}"))),
            (false, true, Some(format!("sha256:{digest}"))),
            (false, false, None),
            (false, false, Some("sha256:xyz".into())),
            (false, false, Some(format!("md5:{digest}"))),
        ] {
            let release = release_with_asset(
                &format!("v{version}"),
                draft,
                prerelease,
                &name,
                &url,
                digest_value.as_deref(),
                "uploaded",
            );
            assert!(resolve_release(&release, &version).is_err());
        }
        for noncanonical in ["1.2", "v1.2.3", "01.2.3", "1.2.3-rc.1"] {
            let release = release_with_asset(
                &format!("v{version}"),
                false,
                false,
                &name,
                &url,
                Some(&format!("sha256:{digest}")),
                "uploaded",
            );
            assert!(resolve_release(&release, noncanonical).is_err());
        }
    }

    #[test]
    fn copy_verified_rejects_altered_download_and_replays_anonymous_fd() {
        let payload = b"original payload".to_vec();
        let expected = digest_for(&payload);
        let mut out = Vec::new();
        copy_verified(payload.as_slice(), &mut out, &expected, MAX_DMG_BYTES).unwrap();
        assert!(copy_verified(
            payload.as_slice(),
            Vec::new(),
            &digest_for(b"tampered payload"),
            MAX_DMG_BYTES
        )
        .is_err());

        // Both installs from one anonymous fd must re-read the original bytes.
        let mut dmg = tempfile::tempfile().unwrap();
        dmg.write_all(&payload).unwrap();
        for _ in 0..2 {
            let mut source = dmg.try_clone().unwrap();
            source.rewind().unwrap();
            let mut copy = Vec::new();
            copy_verified(&mut source, &mut copy, &expected, MAX_DMG_BYTES).unwrap();
            assert_eq!(copy, payload);
        }
    }

    #[test]
    fn copy_verified_never_writes_beyond_the_ceiling() {
        let payload = vec![0xAB_u8; 70_000];
        let expected = digest_for(&payload);
        let mut out = Vec::new();
        assert!(copy_verified(payload.as_slice(), &mut out, &expected, 65_536).is_err());
        assert_eq!(out.len(), 65_536);
    }

    // Local slots only: no global PREPARED state, no release fetch.
    #[test]
    fn prepared_slot_coalesces_and_discards_by_version() {
        use std::cell::Cell;
        let slot = PreparedSlot(Mutex::new(None));
        let fetches = Cell::new(0);
        let make = |version: &str| {
            fetches.set(fetches.get() + 1);
            Ok(PreparedUpdate {
                version: version.to_string(),
                digest: "0".repeat(64),
                dmg: tempfile::tempfile().unwrap(),
            })
        };
        let version = "1.4.0";
        slot.prepare(version, || make(version)).unwrap();
        // Already prepared: the producer must not run again.
        slot.prepare(version, || panic!("unexpected refetch"))
            .unwrap();
        assert_eq!(fetches.get(), 1);
        // A stale dismissal of another version retains the prepared update.
        slot.discard("9.9.9");
        slot.prepare(version, || panic!("unexpected refetch"))
            .unwrap();
        // The matching version frees the slot; the next prepare refetches.
        slot.discard(version);
        slot.prepare(version, || make(version)).unwrap();
        assert_eq!(fetches.get(), 2);
        // A failed fetch leaves the slot empty.
        slot.discard(version);
        assert!(slot
            .prepare(version, || -> Result<PreparedUpdate, String> {
                Err("fetch failed".into())
            })
            .is_err());
        assert!(slot.0.lock().unwrap().is_none());
    }

    #[test]
    fn bundle_metadata_rejects_mismatched_identity_and_unsafe_executable() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("StatPulse.app/Contents/MacOS");
        std::fs::create_dir_all(&app).unwrap();
        let plist_path = root.path().join("StatPulse.app/Contents/Info.plist");
        let exe_path = app.join("statpulse");
        std::fs::write(&exe_path, b"bin").unwrap();
        let write_plist = |id: &str, version: &str, exe: &str| {
            let mut dict = plist::Dictionary::new();
            dict.insert("CFBundleIdentifier".into(), id.into());
            dict.insert("CFBundleShortVersionString".into(), version.into());
            dict.insert("CFBundleExecutable".into(), exe.into());
            plist::Value::Dictionary(dict)
                .to_file_xml(&plist_path)
                .unwrap();
        };
        let version = next_version();

        write_plist("com.statpulse.app", &version, "statpulse");
        validate_bundle_metadata(&root.path().join("StatPulse.app"), &version, "statpulse")
            .unwrap();

        write_plist("com.evil.app", &version, "statpulse");
        assert!(validate_bundle_metadata(
            &root.path().join("StatPulse.app"),
            &version,
            "statpulse",
        )
        .is_err());

        std::fs::write(app.join("other-cli"), b"bin").unwrap();
        write_plist(EXPECTED_IDENTIFIER, &version, "other-cli");
        assert!(validate_bundle_metadata(
            &root.path().join("StatPulse.app"),
            &version,
            "statpulse",
        )
        .is_err());
        write_plist(EXPECTED_IDENTIFIER, "0.0.1", "statpulse");
        assert!(validate_bundle_metadata(
            &root.path().join("StatPulse.app"),
            &version,
            "statpulse",
        )
        .is_err());

        write_plist(EXPECTED_IDENTIFIER, &version, "../escape");
        assert!(validate_bundle_metadata(
            &root.path().join("StatPulse.app"),
            &version,
            "statpulse",
        )
        .is_err());
    }

    fn fixture_stage(parent: &Path, marker: &[u8]) -> tempfile::TempDir {
        let stage = tempfile::Builder::new()
            .prefix(".statpulse-update-")
            .tempdir_in(parent)
            .unwrap();
        let staged = stage.path().join("StatPulse.app/Contents");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("marker"), marker).unwrap();
        stage
    }

    #[test]
    fn early_child_exit_rolls_back_and_ready_byte_upgrades() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("Applications");
        std::fs::create_dir_all(&parent).unwrap();
        let current = parent.join("StatPulse.app/Contents");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("marker"), b"old").unwrap();
        let marker = parent.join("StatPulse.app/Contents/marker");

        // Case 1: the child exits before signalling, so the exchange is undone.
        let stage = fixture_stage(&parent, b"new");
        let parent_fd = open_dir(&parent).unwrap();
        let stage_fd = open_dir(stage.path()).unwrap();
        fs::renameat_with(
            &parent_fd,
            std::ffi::OsStr::new("StatPulse.app"),
            &stage_fd,
            "StatPulse.app",
            RenameFlags::EXCHANGE,
        )
        .unwrap();
        let (mut child, mut ready) = spawn_handshake_child(
            Command::new("/bin/sh").arg("-c").arg("exit 0"),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(wait_ready(&mut ready, &mut child).is_err());
        reap_child(&mut child).unwrap();
        let error = rollback_activation(
            &parent_fd,
            std::ffi::OsStr::new("StatPulse.app"),
            &stage_fd,
            stage,
            "early exit".into(),
        );
        assert!(error.contains("restored"));
        assert_eq!(std::fs::read(&marker).unwrap(), b"old");

        // Case 2: a fixture child signals the exact ready byte and stays
        // alive; the staged bundle stays installed.
        let stage = fixture_stage(&parent, b"newer");
        let stage_fd = open_dir(stage.path()).unwrap();
        fs::renameat_with(
            &parent_fd,
            std::ffi::OsStr::new("StatPulse.app"),
            &stage_fd,
            "StatPulse.app",
            RenameFlags::EXCHANGE,
        )
        .unwrap();
        let (mut child, mut ready) = spawn_handshake_child(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("printf R >&0; exec /bin/sleep 30"),
            Duration::from_secs(5),
        )
        .unwrap();
        wait_ready(&mut ready, &mut child).unwrap();
        reap_child(&mut child).unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), b"newer");
        drop(stage);
    }

    #[test]
    fn handshake_rejects_wrong_or_missing_ready_byte() {
        let (mut child, mut ready) = spawn_handshake_child(
            Command::new("/bin/sh")
                .arg("-c")
                .arg("printf X >&0; exec /bin/sleep 30"),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(wait_ready(&mut ready, &mut child).is_err());
        reap_child(&mut child).unwrap();

        let (mut child, mut ready) = spawn_handshake_child(
            Command::new("/bin/sh").arg("-c").arg("exec /bin/sleep 30"),
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(wait_ready(&mut ready, &mut child).is_err());
        reap_child(&mut child).unwrap();
    }

    fn write_fixture_plist(app_dir: &Path, id: &str, version: &str, exe: &str) {
        let mut dict = plist::Dictionary::new();
        dict.insert("CFBundleIdentifier".into(), id.into());
        dict.insert("CFBundleShortVersionString".into(), version.into());
        dict.insert("CFBundleExecutable".into(), exe.into());
        plist::Value::Dictionary(dict)
            .to_file_xml(app_dir.join("Contents/Info.plist"))
            .unwrap();
    }

    #[test]
    fn bundle_metadata_rejects_macos_symlink_escaping_bundle() {
        let root = tempfile::tempdir().unwrap();
        let app_dir = root.path().join("StatPulse.app");
        let outside = root.path().join("payload");
        std::fs::create_dir_all(app_dir.join("Contents")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let version = next_version();
        write_fixture_plist(&app_dir, EXPECTED_IDENTIFIER, &version, "statpulse");
        std::fs::write(outside.join("statpulse"), b"bin").unwrap();
        std::os::unix::fs::symlink(&outside, app_dir.join("Contents/MacOS")).unwrap();

        assert!(validate_bundle_metadata(&app_dir, &version, "statpulse").is_err());
    }

    #[test]
    fn unbundled_executable_path_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("statpulse-dev");
        std::fs::write(&exe, b"bin").unwrap();
        assert!(app_context(&exe).is_err());

        let nested = root.path().join("StatPulse.app/Contents/MacOS");
        std::fs::create_dir_all(&nested).unwrap();
        let bundled_exe = nested.join("statpulse");
        std::fs::write(&bundled_exe, b"bin").unwrap();
        // Bundle layout is present but metadata is missing: still rejected.
        assert!(app_context(&bundled_exe).is_err());
    }

    #[test]
    fn detach_failure_keeps_mount_directory() {
        let dir = tempfile::tempdir().unwrap().keep();
        let error =
            detach_dmg(Mount { dir: dir.clone() }, |_| Err("detach refused".into())).unwrap_err();
        assert!(dir.exists());
        assert!(error.contains(&*dir.to_string_lossy()));
        std::fs::remove_dir(&dir).unwrap();
    }
}
