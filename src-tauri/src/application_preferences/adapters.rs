use super::preferences::AutostartAdapter;
use std::path::{Path, PathBuf};
use std::process::Command;

const LOGIN_ITEM_SCRIPT: &str = include_str!("login_item.applescript");

// The login item targets the installed bundle; unbundled development builds
// register the executable itself.
#[derive(Clone)]
pub struct MacLoginItem {
    path: PathBuf,
}

impl MacLoginItem {
    pub fn running() -> Result<Self, String> {
        let exe =
            std::fs::canonicalize(std::env::current_exe().map_err(|error| error.to_string())?)
                .map_err(|error| format!("failed to resolve the running executable: {error}"))?;
        Ok(Self {
            path: login_item_path(&exe),
        })
    }

    fn run(&self, action: &str) -> Result<bool, String> {
        let output = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(LOGIN_ITEM_SCRIPT)
            .arg(action)
            .arg(&self.path)
            .output()
            .map_err(|error| format!("failed to query login items: {error}"))?;
        decode_output(output.status.success(), &output.stdout, &output.stderr)
    }
}

fn login_item_path(exe: &Path) -> PathBuf {
    exe.parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "MacOS"))
        .and_then(|dir| dir.parent())
        .filter(|dir| dir.file_name().is_some_and(|name| name == "Contents"))
        .and_then(|dir| dir.parent())
        .filter(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        .map_or_else(|| exe.to_path_buf(), Path::to_path_buf)
}

// Success must produce exactly "true" or "false"; anything else, or a nonzero
// exit, is a failed query rather than a disabled login item.
fn decode_output(success: bool, stdout: &[u8], stderr: &[u8]) -> Result<bool, String> {
    if !success {
        let detail = String::from_utf8_lossy(stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            "login item query failed".into()
        } else {
            format!("login item query failed: {detail}")
        });
    }
    match String::from_utf8_lossy(stdout).trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("login item query returned malformed output".into()),
    }
}

impl AutostartAdapter for MacLoginItem {
    fn is_enabled(&self) -> Result<bool, String> {
        self.run("status")
    }

    fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let actual = self.run(if enabled { "enable" } else { "disable" })?;
        if actual != enabled {
            return Err(format!("launch at login remains {actual}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_output, login_item_path};
    use std::path::Path;

    #[test]
    fn decode_accepts_exact_boolean_output() {
        assert!(decode_output(true, b"true\n", b"").unwrap());
        assert!(!decode_output(true, b"false", b"").unwrap());
        assert!(decode_output(false, b"false", b"denied").is_err());
        assert!(decode_output(true, b"yes", b"").is_err());
        assert!(decode_output(true, b"", b"").is_err());
    }

    #[test]
    fn bundled_executables_register_the_app_path() {
        let exe = Path::new("/Applications/StatPulse.app/Contents/MacOS/statpulse");
        assert_eq!(
            login_item_path(exe),
            Path::new("/Applications/StatPulse.app")
        );
        let unbundled = Path::new("/tmp/build/statpulse");
        assert_eq!(login_item_path(unbundled), unbundled);
    }
}
