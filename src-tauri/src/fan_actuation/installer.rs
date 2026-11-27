use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const INSTALLED_HELPER_PATH: &str =
    "/Library/PrivilegedHelperTools/com.statpulse.fan-actuation";
pub const INSTALLED_PLIST_PATH: &str = "/Library/LaunchDaemons/com.statpulse.fan-actuation.plist";
pub const LEGACY_INSTALLED_HELPER_PATH: &str =
    "/Library/PrivilegedHelperTools/com.superfan.fan-actuation";
pub const LEGACY_INSTALLED_PLIST_PATH: &str =
    "/Library/LaunchDaemons/com.superfan.fan-actuation.plist";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    NotInstalled,
    Installed,
    Unavailable,
}

pub fn status() -> InstallStatus {
    let primary = installed_status_at(
        Path::new(INSTALLED_HELPER_PATH),
        Path::new(INSTALLED_PLIST_PATH),
    );
    if primary != InstallStatus::NotInstalled {
        return primary;
    }
    installed_status_at(
        Path::new(LEGACY_INSTALLED_HELPER_PATH),
        Path::new(LEGACY_INSTALLED_PLIST_PATH),
    )
}

fn installed_status_at(helper: &Path, plist: &Path) -> InstallStatus {
    installed_status_at_with_owner(helper, plist, 0, 0)
}

fn installed_status_at_with_owner(
    helper: &Path,
    plist: &Path,
    expected_uid: u32,
    expected_gid: u32,
) -> InstallStatus {
    match (
        installed_artifact(helper, 0o111, expected_uid, expected_gid),
        installed_artifact(plist, 0, expected_uid, expected_gid),
    ) {
        (InstalledArtifact::Valid, InstalledArtifact::Valid) => InstallStatus::Installed,
        (InstalledArtifact::Missing, InstalledArtifact::Missing) => InstallStatus::NotInstalled,
        _ => InstallStatus::Unavailable,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstalledArtifact {
    Missing,
    Valid,
    Invalid,
}

fn installed_artifact(
    path: &Path,
    required_mode: u32,
    expected_uid: u32,
    expected_gid: u32,
) -> InstalledArtifact {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == expected_uid
                && metadata.gid() == expected_gid
                && metadata.mode() & required_mode == required_mode =>
        {
            InstalledArtifact::Valid
        }
        Ok(_) => InstalledArtifact::Invalid,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => InstalledArtifact::Missing,
        Err(_) => InstalledArtifact::Invalid,
    }
}

fn file_sha256(path: &Path) -> Option<[u8; 32]> {
    let bytes = fs::read(path).ok()?;
    Some(Sha256::digest(&bytes).into())
}

fn installed_helper_matches_bundle(installed_helper: &Path, resource_directory: &Path) -> bool {
    let Some(installed_digest) = file_sha256(installed_helper) else {
        return false;
    };
    let Some(bundled_digest) = file_sha256(&resource_directory.join("fan-actuation-daemon")) else {
        return false;
    };
    installed_digest == bundled_digest
}

pub fn install(resource_directory: &Path, force: bool) -> Result<InstallStatus, String> {
    if !force
        && status() == InstallStatus::Installed
        && crate::fan_actuation::client::status()
            == crate::hardware_telemetry::contract::FanActuationStatus::Ready
        && installed_helper_matches_bundle(Path::new(INSTALLED_HELPER_PATH), resource_directory)
    {
        return Ok(InstallStatus::Installed);
    }
    let installer = validate_resources(resource_directory)?;
    let script = resource_directory.join("authorize-install.applescript");
    let output = Command::new("/usr/bin/osascript")
        .arg(&script)
        .arg(&installer)
        .output()
        .map_err(|error| format!("could not start administrator installer: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            "administrator installation was cancelled or failed".into()
        } else {
            message
        });
    }
    match status() {
        InstallStatus::Installed => Ok(InstallStatus::Installed),
        _ => Err("Fan actuation helper installation did not produce valid root-owned files".into()),
    }
}

fn validate_resources(resource_directory: &Path) -> Result<PathBuf, String> {
    for name in [
        "fan-actuation-daemon",
        "install-fan-actuation.sh",
        "authorize-install.applescript",
        "com.statpulse.fan-actuation.plist",
    ] {
        let path = resource_directory.join(name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| format!("missing bundled Fan actuation resource: {name}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("unsafe bundled Fan actuation resource: {name}"));
        }
    }
    let installer = resource_directory.join("install-fan-actuation.sh");
    let helper = resource_directory.join("fan-actuation-daemon");
    if fs::metadata(&installer)
        .map_err(|error| error.to_string())?
        .mode()
        & 0o111
        == 0
        || fs::metadata(&helper)
            .map_err(|error| error.to_string())?
            .mode()
            & 0o111
            == 0
    {
        return Err("bundled Fan actuation installer and helper must be executable".into());
    }
    Ok(installer)
}

#[cfg(test)]
mod tests {
    use super::{installed_status_at_with_owner, validate_resources, InstallStatus};
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn resource_manifest_rejects_missing_and_symlinked_helper() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path();
        assert!(validate_resources(dir).is_err());
        for name in [
            "fan-actuation-daemon",
            "install-fan-actuation.sh",
            "authorize-install.applescript",
            "com.statpulse.fan-actuation.plist",
        ] {
            std::fs::write(dir.join(name), b"fixture").unwrap();
        }
        for name in ["fan-actuation-daemon", "install-fan-actuation.sh"] {
            std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        assert!(validate_resources(dir).is_ok());

        let helper = dir.join("fan-actuation-daemon");
        std::fs::remove_file(&helper).unwrap();
        let target = dir.join("target");
        std::fs::write(&target, b"fixture").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&target, &helper).unwrap();
        assert!(validate_resources(dir).is_err());
    }

    #[test]
    fn installed_status_distinguishes_missing_incomplete_and_installed() {
        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path();
        let helper = dir.join("helper");
        let plist = dir.join("service.plist");
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };

        assert_eq!(
            installed_status_at_with_owner(&helper, &plist, uid, gid),
            InstallStatus::NotInstalled
        );

        std::fs::write(&helper, b"helper").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            installed_status_at_with_owner(&helper, &plist, uid, gid),
            InstallStatus::Unavailable
        );

        std::fs::write(&plist, b"plist").unwrap();
        assert_eq!(
            installed_status_at_with_owner(&helper, &plist, uid, gid),
            InstallStatus::Installed
        );

        assert_eq!(
            installed_status_at_with_owner(&helper, &plist, uid ^ 1, gid),
            InstallStatus::Unavailable
        );

        std::fs::remove_file(&plist).unwrap();
        symlink(&helper, &plist).unwrap();
        assert_eq!(
            installed_status_at_with_owner(&helper, &plist, uid, gid),
            InstallStatus::Unavailable
        );
    }

    #[test]
    fn installed_helper_matches_bundle_compares_sha256() {
        use super::installed_helper_matches_bundle;

        let directory = tempfile::tempdir().unwrap();
        let dir = directory.path();
        let installed = dir.join("installed-helper");
        let bundle_dir = dir.join("bundle");
        std::fs::create_dir_all(&bundle_dir).unwrap();
        let bundled = bundle_dir.join("fan-actuation-daemon");

        assert!(!installed_helper_matches_bundle(&installed, &bundle_dir));

        std::fs::write(&installed, b"daemon-v1").unwrap();
        std::fs::write(&bundled, b"daemon-v1").unwrap();
        assert!(installed_helper_matches_bundle(&installed, &bundle_dir));

        std::fs::write(&bundled, b"daemon-v2").unwrap();
        assert!(!installed_helper_matches_bundle(&installed, &bundle_dir));
    }
}
