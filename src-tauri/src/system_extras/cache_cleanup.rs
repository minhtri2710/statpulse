//! Cleanup of rebuildable cache directories.
//!
//! Traversal is descriptor-relative: each path component is opened with
//! `openat` under its already-open parent with no-follow flags, pinned by
//! fstat, and checked for effective-user ownership, group/other write bits,
//! and extended ACLs. Only two target shapes exist — the explicit
//! [`SHARED_CACHES`] allowlist, or `Work/<project>/<artifact>` whose leaf
//! holds a trusted build-output marker. Cleaning re-pins the leaf and
//! verifies the device/inode identity captured at scan time before removing
//! anything. Shared leaves are emptied; project leaves are removed.

use rustix::fs::{self, AtFlags, Dev, Dir, FileType, Mode, OFlags, Stat};
use rustix::io::Errno;
use std::collections::HashSet;
use std::ffi::{CStr, OsStr};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// `(id, label, home-relative path)` shared caches eligible for cleanup.
/// Consumed by both discovery and [`open_target`] so enumeration and the
/// deletion gate share one source of truth.
pub const SHARED_CACHES: &[(&str, &str, &str)] = &[
    (
        "xcode_derived",
        "Xcode DerivedData",
        "Library/Developer/Xcode/DerivedData",
    ),
    ("npm_cache", "npm Cache", ".npm/_cacache"),
    (
        "homebrew_cache",
        "Homebrew Cache",
        "Library/Caches/Homebrew",
    ),
    (
        "cargo_cache",
        "Cargo Registry Cache",
        ".cargo/registry/cache",
    ),
];

const PROJECT_PARENT: &str = "Work";

/// `(artifact dir name, build-produced marker files proving it is output)`.
/// Any single trusted marker is sufficient evidence.
const ARTIFACT_MARKERS: &[(&str, &[&str])] = &[
    ("target", &[".rustc_info.json"]),
    (
        "node_modules",
        &[
            ".package-lock.json",
            ".modules.yaml",
            ".yarn-integrity",
            ".yarn-state.yml",
        ],
    ),
    (
        ".next",
        &["BUILD_ID", "build-manifest.json", "routes-manifest.json"],
    ),
    (".venv", &["pyvenv.cfg"]),
];

const MAX_PROJECTS: usize = 32;

const DIR_PIN: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const FILE_PIN: OFlags = OFlags::RDONLY
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// Device/inode/size pinned from the same descriptor used for validation.
pub struct CachePin {
    pub dev: u64,
    pub ino: u64,
    pub kib: u64,
}

struct PinnedTarget {
    ancestors: Vec<(std::ffi::OsString, OwnedFd)>,
    leaf: OwnedFd,
    device: Dev,
    project: bool,
}

enum TargetKind {
    Shared,
    Project(&'static [&'static str]),
}

fn classify(rel: &Path) -> Option<TargetKind> {
    if SHARED_CACHES
        .iter()
        .any(|(_, _, shared)| Path::new(shared) == rel)
    {
        return Some(TargetKind::Shared);
    }
    let mut parts = rel.components();
    let (Component::Normal(parent), Component::Normal(project), Component::Normal(artifact)) =
        (parts.next()?, parts.next()?, parts.next()?)
    else {
        return None;
    };
    if parts.next().is_some()
        || parent != OsStr::new(PROJECT_PARENT)
        || project.is_empty()
        || project == OsStr::new(".")
        || project == OsStr::new("..")
    {
        return None;
    }
    let markers = ARTIFACT_MARKERS
        .iter()
        .find(|(name, _)| OsStr::new(name) == artifact)?
        .1;
    Some(TargetKind::Project(markers))
}

/// Enumerate `Work/<project>/<artifact>` candidates as
/// `(logical id, label, home-relative path)` triples. Entries are listed
/// without following symlinks; marker and trust validation happen later on
/// pinned descriptors.
pub fn project_cache_candidates(home: &Path) -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let work_dir = home.join(PROJECT_PARENT);
    let Ok(projects) = std::fs::read_dir(&work_dir) else {
        return out;
    };
    for proj in projects.flatten().take(MAX_PROJECTS) {
        let proj_path = proj.path();
        let Ok(file_type) = proj.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let Some(proj_name) = proj_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        for (artifact, _) in ARTIFACT_MARKERS {
            let candidate = proj_path.join(artifact);
            let Ok(meta) = std::fs::symlink_metadata(&candidate) else {
                continue;
            };
            if meta.file_type().is_symlink() || !meta.is_dir() {
                continue;
            }
            out.push((
                format!("proj::{proj_name}::{artifact}"),
                format!("{proj_name}/{artifact}"),
                Path::new(PROJECT_PARENT).join(proj_name).join(artifact),
            ));
        }
    }
    out
}

#[cfg(target_os = "macos")]
extern "C" {
    fn statpulse_cache_acl_safe(fd: RawFd) -> i32;
}

fn check_acl(fd: RawFd) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        match unsafe { statpulse_cache_acl_safe(fd) } {
            1 => Ok(()),
            0 => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "extended ACL grants write access",
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = fd;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "extended ACL safety cannot be established on this platform",
        ))
    }
}

fn check_owner_mode_acl(fd: RawFd, st: &Stat) -> io::Result<()> {
    let euid = unsafe { libc::geteuid() };
    if st.st_uid != euid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "entry is not owned by the effective user",
        ));
    }
    if (st.st_mode as u32) & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "entry is writable by group or others",
        ));
    }
    check_acl(fd)
}

fn check_trusted_dir(file: &OwnedFd, st: &Stat) -> io::Result<()> {
    if !FileType::from_raw_mode(st.st_mode as _).is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a directory",
        ));
    }
    check_owner_mode_acl(file.as_raw_fd(), st)
}

/// Resolve the effective user's home directory from passwd data rather than
/// trusting arbitrary environment input.
pub fn account_home() -> io::Result<PathBuf> {
    unsafe {
        let uid = libc::geteuid();
        if uid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing privileged account",
            ));
        }
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut size = 1024usize;
        loop {
            let mut buf = vec![0u8; size];
            let mut result: *mut libc::passwd = std::ptr::null_mut();
            let rc = libc::getpwuid_r(
                uid,
                &mut pwd,
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.len(),
                &mut result,
            );
            if rc == libc::ERANGE {
                size *= 2;
                if size > 65_536 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "passwd entry exceeds buffer bound",
                    ));
                }
                continue;
            }
            if rc != 0 {
                return Err(io::Error::from_raw_os_error(rc));
            }
            if result.is_null() || pwd.pw_dir.is_null() || pwd.pw_uid == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "no usable passwd home for the effective user",
                ));
            }
            let dir = PathBuf::from(OsStr::from_bytes(CStr::from_ptr(pwd.pw_dir).to_bytes()));
            if dir.as_os_str().is_empty() || !dir.is_absolute() || dir.parent().is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "refusing empty, relative, or root passwd home",
                ));
            }
            return Ok(dir);
        }
    }
}

fn validate_home(home: &Path) -> io::Result<()> {
    if !home.is_absolute()
        || home.parent().is_none()
        || home
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "home must be an absolute non-root path without special components",
        ));
    }
    Ok(())
}

/// Open `rel` under `home`: the rel path must be an explicit shared-cache
/// path or `Work/<project>/<artifact>`, the effective user must not be root,
/// and every pinned component (home included) must be a real directory owned
/// by the effective user, not group/other writable, ACL-safe, and on one
/// filesystem. Project leaves must hold a trusted regular-file marker.
fn open_target(home: &Path, rel: &Path) -> io::Result<PinnedTarget> {
    let kind = classify(rel).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path is not an allowed rebuildable cache",
        )
    })?;
    validate_home(home)?;
    if unsafe { libc::geteuid() } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing cleanup while running as root",
        ));
    }
    let home_dir = fs::openat(fs::CWD, home, DIR_PIN, Mode::empty())?;
    let home_st = fs::fstat(&home_dir)?;
    check_trusted_dir(&home_dir, &home_st)?;
    let device = home_st.st_dev;

    let mut ancestors: Vec<(std::ffi::OsString, OwnedFd)> = Vec::new();
    let mut dir = home_dir;
    for comp in rel.components() {
        let Component::Normal(name) = comp else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported path component",
            ));
        };
        let child = fs::openat(&dir, name, DIR_PIN, Mode::empty())?;
        let st = fs::fstat(&child)?;
        check_trusted_dir(&child, &st)?;
        if st.st_dev != device {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to cross a filesystem boundary",
            ));
        }
        ancestors.push((name.to_os_string(), std::mem::replace(&mut dir, child)));
    }

    let project = match kind {
        TargetKind::Shared => false,
        TargetKind::Project(markers) => {
            let mut found = false;
            for marker in markers {
                match fs::openat(&dir, *marker, FILE_PIN, Mode::empty()) {
                    Ok(marker_fd) => {
                        let st = fs::fstat(&marker_fd)?;
                        if !FileType::from_raw_mode(st.st_mode as _).is_file()
                            || st.st_dev != device
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::PermissionDenied,
                                "marker is not a same-device regular file",
                            ));
                        }
                        check_owner_mode_acl(marker_fd.as_raw_fd(), &st)?;
                        found = true;
                        break;
                    }
                    Err(Errno::NOENT) => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            if !found {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "leaf has no build-output marker",
                ));
            }
            true
        }
    };

    Ok(PinnedTarget {
        ancestors,
        leaf: dir,
        device,
        project,
    })
}

/// Allocated 512-byte blocks under `dir` on `device`, deduplicating
/// hardlinked files within this one root — matching `du -skx`.
fn allocated_blocks(dir: &OwnedFd, device: Dev, seen: &mut HashSet<u64>) -> io::Result<u64> {
    let mut total = fs::fstat(dir)?.st_blocks.max(0) as u64;
    for item in Dir::read_from(dir)? {
        let item = item?;
        let name = item.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let stat = match fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(s) => s,
            Err(Errno::NOENT) => continue,
            Err(e) => return Err(e.into()),
        };
        if FileType::from_raw_mode(stat.st_mode as _).is_dir() {
            if stat.st_dev != device {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "refusing to cross a filesystem boundary",
                ));
            }
            let child = match fs::openat(dir, name, DIR_PIN, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::NOENT) => continue,
                Err(e) => return Err(e.into()),
            };
            let s = fs::fstat(&child)?;
            if s.st_dev != stat.st_dev || s.st_ino != stat.st_ino {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "directory entry changed while scanning",
                ));
            }
            check_trusted_dir(&child, &s)?;
            total = total.saturating_add(allocated_blocks(&child, device, seen)?);
        } else if stat.st_dev == device && (stat.st_nlink <= 1 || seen.insert(stat.st_ino)) {
            total = total.saturating_add(stat.st_blocks.max(0) as u64);
        }
    }
    Ok(total)
}

/// Pin `rel` under `home`, measure `du -skx`-equivalent KiB, and report the
/// leaf device/inode/size. `Ok(None)` covers a missing leaf, a leaf with no
/// marker, or a leaf below `min_kib`; trust violations fail with an error.
pub fn inspect(home: &Path, rel: &Path, min_kib: u64) -> io::Result<Option<CachePin>> {
    let pin = match open_target(home, rel) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut seen = HashSet::new();
    let blocks = allocated_blocks(&pin.leaf, pin.device, &mut seen)?;
    let kib = blocks.div_ceil(2);
    if kib < min_kib {
        return Ok(None);
    }
    let st = fs::fstat(&pin.leaf)?;
    Ok(Some(CachePin {
        dev: st.st_dev as u64,
        ino: st.st_ino,
        kib,
    }))
}

/// Verify `held` is still the `name` entry inside `parent`, then unlink it as
/// a directory entry.
fn remove_dir_entry(parent: &OwnedFd, name: &CStr, held: &OwnedFd) -> io::Result<()> {
    let held_st = fs::fstat(held)?;
    let entry_st = match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(s) => s,
        Err(Errno::NOENT) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "directory entry vanished before removal",
            ))
        }
        Err(e) => return Err(e.into()),
    };
    if held_st.st_dev != entry_st.st_dev || held_st.st_ino != entry_st.st_ino {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "directory entry changed before removal",
        ));
    }
    fs::unlinkat(parent, name, AtFlags::REMOVEDIR).map_err(Into::into)
}

fn remove_contents(dir: &OwnedFd, device: Dev) -> io::Result<()> {
    for item in Dir::read_from(dir)? {
        let item = item?;
        let name = item.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let stat = match fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(s) => s,
            Err(Errno::NOENT) => continue,
            Err(e) => return Err(e.into()),
        };
        if FileType::from_raw_mode(stat.st_mode as _).is_dir() {
            let child = match fs::openat(dir, name, DIR_PIN, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::NOENT) => continue,
                Err(e) => return Err(e.into()),
            };
            let s = fs::fstat(&child)?;
            if s.st_dev != stat.st_dev || s.st_ino != stat.st_ino || s.st_dev != device {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "directory entry changed or crosses a filesystem boundary",
                ));
            }
            check_trusted_dir(&child, &s)?;
            remove_contents(&child, device)?;
            match remove_dir_entry(dir, name, &child) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        } else {
            // Files, symlinks (unlinked as links), sockets, fifos: remove the
            // entry itself; unlinkat never follows links.
            match fs::unlinkat(dir, name, AtFlags::empty()) {
                Ok(()) => {}
                Err(Errno::NOENT) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Ok(())
}

/// Re-pin `rel` under `home`, verify the leaf is still the scanned inode, and
/// delete its contents. Shared leaves stay in place; project leaves are
/// removed through the same pinned-entry check.
pub fn clean(home: &Path, rel: &Path, expected_dev: u64, expected_ino: u64) -> io::Result<()> {
    let pin = open_target(home, rel)?;
    let st = fs::fstat(&pin.leaf)?;
    if st.st_dev as u64 != expected_dev || st.st_ino != expected_ino {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cache leaf was replaced since it was scanned",
        ));
    }
    remove_contents(&pin.leaf, pin.device)?;
    if pin.project {
        let Some((leaf_name, parent)) = pin.ancestors.last() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "project leaf has no parent",
            ));
        };
        let name = std::ffi::CString::new(leaf_name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains NUL"))?;
        remove_dir_entry(parent, &name, &pin.leaf)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use tempfile::TempDir;

    const KIB: u64 = 1024;

    fn make_file(path: &Path, kib: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; (kib * KIB) as usize]).unwrap();
    }

    fn home() -> TempDir {
        TempDir::new().unwrap()
    }

    fn shared_home() -> (TempDir, PathBuf) {
        let tmp = home();
        let rel = Path::new(SHARED_CACHES[1].2);
        let leaf = tmp.path().join(rel);
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&leaf.join("cached.bin"), 8);
        (tmp, rel.to_path_buf())
    }

    fn project_home() -> (TempDir, PathBuf) {
        let tmp = home();
        let rel = Path::new("Work/proj/target");
        let leaf = tmp.path().join(rel);
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&leaf.join(".rustc_info.json"), 1);
        make_file(&leaf.join("build/out.bin"), 8);
        (tmp, rel.to_path_buf())
    }

    fn chmod(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn shared_cache_scans_and_cleans_contents_only() {
        let (tmp, rel) = shared_home();
        let leaf = tmp.path().join(&rel);
        let pin = inspect(tmp.path(), &rel, 1).unwrap().unwrap();
        assert!(pin.kib >= 8);
        clean(tmp.path(), &rel, pin.dev, pin.ino).unwrap();
        assert!(leaf.is_dir());
        assert_eq!(std::fs::read_dir(&leaf).unwrap().count(), 0);
    }

    #[test]
    fn project_cache_requires_marker_and_allowed_shape() {
        let tmp = home();
        let leaf = tmp.path().join("Work/proj/target");
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&leaf.join("build/out.bin"), 8);
        assert!(
            inspect(tmp.path(), Path::new("Work/proj/target"), 0)
                .unwrap()
                .is_none(),
            "a target directory without a build marker is not a cache",
        );
        make_file(&leaf.join(".rustc_info.json"), 1);
        let pin = inspect(tmp.path(), Path::new("Work/proj/target"), 0)
            .unwrap()
            .unwrap();
        assert!(pin.kib >= 8);

        for rel in [
            ".npm",
            ".ssh",
            "Work/proj/evil",
            "Work/proj",
            "../etc",
            "Work/../.npm/_cacache",
            ".npm/_cacache/extra",
        ] {
            let result = inspect(tmp.path(), Path::new(rel), 0);
            assert!(
                result.is_err() || result.unwrap().is_none(),
                "{rel} must not resolve as a cache target",
            );
        }
    }

    #[test]
    fn untrusted_ownership_bits_refuse_scan_and_clean() {
        let (tmp, rel) = shared_home();
        let leaf = tmp.path().join(&rel);

        chmod(&leaf, 0o775);
        assert!(inspect(tmp.path(), &rel, 0).is_err(), "group-writable leaf");
        chmod(&leaf, 0o755);

        let parent = leaf.parent().unwrap();
        chmod(parent, 0o777);
        assert!(
            inspect(tmp.path(), &rel, 0).is_err(),
            "world-writable parent"
        );
        chmod(parent, 0o755);

        chmod(tmp.path(), 0o775);
        assert!(inspect(tmp.path(), &rel, 0).is_err(), "group-writable home");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn extended_acl_allow_write_refuses_and_deny_read_passes() {
        let user = Command::new("id").arg("-un").output().unwrap();
        let user = String::from_utf8(user.stdout).unwrap();
        let user = user.trim();

        let (tmp, rel) = project_home();
        let leaf = tmp.path().join(&rel);
        let acl = |spec: String| {
            let status = Command::new("chmod")
                .args(["+a", &spec])
                .arg(&leaf)
                .status()
                .unwrap();
            assert!(status.success());
        };

        acl(format!("{user} allow write"));
        assert!(inspect(tmp.path(), &rel, 0).is_err(), "allow-write ACL");
        Command::new("chmod")
            .args(["-a#", "0"])
            .arg(&leaf)
            .status()
            .unwrap();

        acl(format!("{user} allow read"));
        assert!(
            inspect(tmp.path(), &rel, 0).unwrap().is_some(),
            "allow-read ACL is not a write grant"
        );
        Command::new("chmod")
            .args(["-a#", "0"])
            .arg(&leaf)
            .status()
            .unwrap();

        acl(format!("{user} deny delete"));
        assert!(
            inspect(tmp.path(), &rel, 0).unwrap().is_some(),
            "deny ACL entries are ignored"
        );
        let status = Command::new("chmod")
            .args(["-a#", "0"])
            .arg(&leaf)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn extended_acl_on_marker_refuses() {
        let user = Command::new("id").arg("-un").output().unwrap();
        let user = String::from_utf8(user.stdout).unwrap();
        let user = user.trim().to_string();
        let (tmp, rel) = project_home();
        let marker = tmp.path().join(&rel).join(".rustc_info.json");
        let status = Command::new("chmod")
            .args(["+a", &format!("{user} allow write")])
            .arg(&marker)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            inspect(tmp.path(), &rel, 0).is_err(),
            "writable marker must not grant authority"
        );
    }

    #[test]
    fn replaced_leaf_is_refused_and_left_untouched() {
        let (tmp, rel) = shared_home();
        let leaf = tmp.path().join(&rel);
        let pin = inspect(tmp.path(), &rel, 1).unwrap().unwrap();

        let held = tmp.path().join("held-leaf");
        std::fs::rename(&leaf, &held).unwrap();
        std::fs::create_dir_all(&leaf).unwrap();
        let replacement = leaf.join("replacement.txt");
        make_file(&replacement, 1);

        assert!(clean(tmp.path(), &rel, pin.dev, pin.ino).is_err());
        assert!(
            replacement.exists(),
            "replacement content must survive a stale selection",
        );
    }

    #[test]
    fn discovery_lists_only_explicit_candidates() {
        let (tmp, _) = project_home();
        let other = tmp.path().join("Work/proj/build");
        std::fs::create_dir_all(&other).unwrap();
        make_file(&other.join("x.bin"), 1);
        let found = project_cache_candidates(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "proj::proj::target");
        assert_eq!(found[0].2, Path::new("Work/proj/target"));
    }

    fn du_kib(path: &Path) -> u64 {
        let output = Command::new("/usr/bin/du")
            .args(["-skx"])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        std::str::from_utf8(&output.stdout)
            .unwrap()
            .split('\t')
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    #[test]
    fn inspect_matches_du_for_known_shared_targets() {
        let tmp = home();
        let npm = tmp.path().join(".npm/_cacache");
        let cargo = tmp.path().join(".cargo/registry/cache");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::create_dir_all(&cargo).unwrap();

        // Allocated content, nested dirs, hardlinks inside one root, a
        // hardlink shared across both roots, a sparse file, and a symlink
        // pointing outside the home entirely.
        make_file(&npm.join("index/blob.bin"), 8);
        make_file(&npm.join("empty-dir/.keep"), 0);
        std::fs::create_dir_all(npm.join("truly-empty")).unwrap();
        make_file(&cargo.join("registry/pkg.crate"), 8);
        std::fs::hard_link(npm.join("index/blob.bin"), npm.join("index/blob-alias.bin")).unwrap();
        std::fs::hard_link(
            npm.join("index/blob.bin"),
            cargo.join("registry/shared.crate"),
        )
        .unwrap();
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut sparse = std::fs::File::create(npm.join("sparse.bin")).unwrap();
            sparse.seek(SeekFrom::Start(4 * 1024 * 1024 - 1)).unwrap();
            sparse.write_all(&[0]).unwrap();
        }
        let outside = tmp.path().join("outside.txt");
        make_file(&outside, 4);
        std::os::unix::fs::symlink(&outside, npm.join("link-out")).unwrap();
        std::os::unix::fs::symlink(&outside, cargo.join("link-out")).unwrap();

        for rel in [".npm/_cacache", ".cargo/registry/cache"] {
            let rel = Path::new(rel);
            let pin = inspect(tmp.path(), rel, 0).unwrap().unwrap();
            assert_eq!(
                pin.kib,
                du_kib(&tmp.path().join(rel)),
                "{rel:?}: inspect must match du -skx",
            );
        }
    }

    #[test]
    fn project_leaf_is_removed_but_sentinel_survives() {
        let tmp = home();
        let leaf = tmp.path().join("Work/proj/node_modules");
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&leaf.join(".package-lock.json"), 1);
        make_file(&leaf.join("dep/dist.js"), 4);
        let sentinel = tmp.path().join("Work/proj/keep.txt");
        make_file(&sentinel, 1);
        std::os::unix::fs::symlink(&sentinel, leaf.join("dep/keep-link")).unwrap();

        let rel = Path::new("Work/proj/node_modules");
        let pin = inspect(tmp.path(), rel, 0).unwrap().unwrap();
        clean(tmp.path(), rel, pin.dev, pin.ino).unwrap();
        assert!(!leaf.exists(), "project artifact leaf must be removed");
        assert!(
            sentinel.exists(),
            "sentinel outside the target must survive"
        );
    }

    #[test]
    fn shared_leaf_is_emptied_not_removed() {
        let (tmp, rel) = shared_home();
        let leaf = tmp.path().join(&rel);
        let pin = inspect(tmp.path(), &rel, 1).unwrap().unwrap();
        clean(tmp.path(), &rel, pin.dev, pin.ino).unwrap();
        assert!(leaf.is_dir());
        assert_eq!(std::fs::read_dir(&leaf).unwrap().count(), 0);
    }

    #[test]
    fn symlink_components_are_refused() {
        let tmp = home();

        // Home that is itself a symlink.
        let real_home = tmp.path().join("real-home");
        let link_home = tmp.path().join("link-home");
        std::fs::create_dir_all(real_home.join(".npm/_cacache")).unwrap();
        std::os::unix::fs::symlink(&real_home, &link_home).unwrap();
        assert!(inspect(&link_home, Path::new(".npm/_cacache"), 0).is_err());

        // Symlinked intermediate ancestor for a shared cache.
        let shared = tmp.path().join("real-cacache");
        std::fs::create_dir_all(&shared).unwrap();
        std::os::unix::fs::symlink(&shared, tmp.path().join(".npm")).unwrap();
        assert!(inspect(tmp.path(), Path::new(".npm/_cacache"), 0).is_err());
        std::fs::remove_file(tmp.path().join(".npm")).unwrap();

        // Symlinked Work directory.
        let real_work = tmp.path().join("real-work");
        std::fs::create_dir_all(&real_work).unwrap();
        std::os::unix::fs::symlink(&real_work, tmp.path().join("Work")).unwrap();
        assert!(inspect(tmp.path(), Path::new("Work/p/target"), 0).is_err());
        std::fs::remove_file(tmp.path().join("Work")).unwrap();

        // Symlinked project leaf and symlinked marker.
        let proj = tmp.path().join("Work/proj");
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        make_file(&elsewhere.join("data.bin"), 1);
        std::fs::create_dir_all(&proj).unwrap();
        std::os::unix::fs::symlink(&elsewhere, proj.join("target")).unwrap();
        assert!(inspect(tmp.path(), Path::new("Work/proj/target"), 0).is_err());
        std::fs::remove_file(proj.join("target")).unwrap();
        let leaf = proj.join("target");
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&elsewhere.join(".rustc_info.json"), 1);
        std::os::unix::fs::symlink(
            elsewhere.join(".rustc_info.json"),
            leaf.join(".rustc_info.json"),
        )
        .unwrap();
        assert!(
            inspect(tmp.path(), Path::new("Work/proj/target"), 0).is_err(),
            "a symlinked marker must not be accepted",
        );
    }

    #[test]
    fn parent_manifests_are_not_authority_and_markers_cover_each_artifact() {
        let tmp = home();

        // A manifest in the project root alone proves nothing.
        let leaf = tmp.path().join("Work/app/target");
        std::fs::create_dir_all(&leaf).unwrap();
        make_file(&leaf.join("out.bin"), 4);
        make_file(&tmp.path().join("Work/app/Cargo.toml"), 1);
        assert!(inspect(tmp.path(), Path::new("Work/app/target"), 0)
            .unwrap()
            .is_none());

        // Every advertised marker variant registers the leaf.
        for (artifact, marker) in [
            ("target", ".rustc_info.json"),
            ("node_modules", ".package-lock.json"),
            ("node_modules", ".modules.yaml"),
            ("node_modules", ".yarn-integrity"),
            ("node_modules", ".yarn-state.yml"),
            (".next", "BUILD_ID"),
            (".next", "build-manifest.json"),
            (".next", "routes-manifest.json"),
            (".venv", "pyvenv.cfg"),
        ] {
            let leaf = tmp.path().join("Work/m").join(artifact);
            std::fs::create_dir_all(&leaf).unwrap();
            make_file(&leaf.join(marker), 1);
            make_file(&leaf.join("payload.bin"), 4);
            let rel = Path::new("Work/m").join(artifact);
            assert!(
                inspect(tmp.path(), &rel, 0).unwrap().is_some(),
                "{artifact}/{marker} must register as a project cache",
            );
            std::fs::remove_dir_all(&leaf).unwrap();
        }
    }

    #[test]
    fn root_home_is_refused_without_acting() {
        assert!(inspect(Path::new("/"), Path::new(".npm/_cacache"), 0).is_err());
        assert!(clean(Path::new("/"), Path::new(".npm/_cacache"), 0, 0).is_err());
    }

    #[test]
    fn account_home_matches_passwd_entry() {
        let dir = account_home().unwrap();
        assert!(dir.is_absolute());
        assert_ne!(dir, Path::new("/"));
    }
}
