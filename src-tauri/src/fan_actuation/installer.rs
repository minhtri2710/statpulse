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
    match (
        installed_artifact(helper, 0o111),
        installed_artifact(plist, 0),
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

fn installed_artifact(path: &Path, required_mode: u32) -> InstalledArtifact {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_file()
                && !metadata.file_type().is_symlink()
                && metadata.uid() == 0
                && metadata.gid() == 0
                && metadata.mode() & required_mode == required_mode =>
        {
            InstalledArtifact::Valid
        }
        Ok(_) => InstalledArtifact::Invalid,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => InstalledArtifact::Missing,
        Err(_) => InstalledArtifact::Invalid,
    }
}

pub fn install(resource_directory: &Path) -> Result<InstallStatus, String> {
    if status() == InstallStatus::Installed
        && crate::fan_actuation::client::status()
            == crate::hardware_telemetry::contract::FanActuationStatus::Ready
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
    use super::{installed_status_at, validate_resources, InstallStatus};
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("statpulse-installer-{nonce}"));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resource_manifest_rejects_missing_and_symlinked_helper() {
        let directory = fixture_dir();
        assert!(validate_resources(&directory).is_err());
        for name in [
            "fan-actuation-daemon",
            "install-fan-actuation.sh",
            "authorize-install.applescript",
            "com.statpulse.fan-actuation.plist",
        ] {
            std::fs::write(directory.join(name), b"fixture").unwrap();
        }
        for name in ["fan-actuation-daemon", "install-fan-actuation.sh"] {
            std::fs::set_permissions(directory.join(name), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        assert!(validate_resources(&directory).is_ok());

        let helper = directory.join("fan-actuation-daemon");
        std::fs::remove_file(&helper).unwrap();
        let target = directory.join("target");
        std::fs::write(&target, b"fixture").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&target, &helper).unwrap();
        assert!(validate_resources(&directory).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn installed_status_distinguishes_missing_from_incomplete_installation() {
        let directory = fixture_dir();
        let helper = directory.join("helper");
        let plist = directory.join("service.plist");
        assert_eq!(
            installed_status_at(&helper, &plist),
            InstallStatus::NotInstalled
        );
        std::fs::write(&helper, b"helper").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            installed_status_at(&helper, &plist),
            InstallStatus::Unavailable
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
