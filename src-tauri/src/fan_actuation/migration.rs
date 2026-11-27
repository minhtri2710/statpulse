use std::path::Path;

pub const LEGACY_HELPER_PATH: &str = "/usr/local/bin/smc-helper";

pub fn remove_legacy_helper() -> Result<(), String> {
    remove_legacy_helper_at(Path::new(LEGACY_HELPER_PATH))
}

fn remove_legacy_helper_at(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => std::fs::remove_file(path)
            .map_err(|error| format!("could not remove legacy setuid helper: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not inspect legacy setuid helper: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::remove_legacy_helper_at;

    #[test]
    fn removes_only_the_named_legacy_path() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("smc-helper");
        let neighbor = directory.path().join("keep-me");
        std::fs::write(&legacy, b"legacy").unwrap();
        std::fs::write(&neighbor, b"neighbor").unwrap();

        remove_legacy_helper_at(&legacy).unwrap();

        assert!(!legacy.exists());
        assert!(neighbor.exists());
    }

    #[test]
    fn missing_legacy_path_is_already_migrated() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("uninstalled-helper");

        remove_legacy_helper_at(&path).unwrap();
    }
}
