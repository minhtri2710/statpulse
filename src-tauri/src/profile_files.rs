use crate::application_preferences::contract::{ApplicationPreferences, ConfigurationBundle};
use crate::application_preferences::preferences::{self, AutostartAdapter, PreferencesStore};
use crate::thermal_policy::contract::{ThermalPolicyMode, ThermalPolicySettings};
use crate::thermal_policy::state::SettingsStore;
use rustix::fs::{self, FlockOperation};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const PREFERENCES_FILE: &str = "application-preferences.json";
pub(crate) const PREFERENCES_KEY: &str = "preferences";
pub(crate) const POLICY_FILE: &str = "thermal-policy.json";
pub(crate) const POLICY_KEY: &str = "settings";
const LOCK_FILE: &str = ".profile.lock";
const JOURNAL_FILE: &str = ".profile-import.pending";

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Before {
    #[serde(deserialize_with = "required_option")]
    preferences: Option<Vec<u8>>,
    #[serde(deserialize_with = "required_option")]
    policy: Option<Vec<u8>>,
    #[serde(deserialize_with = "required_option")]
    launch_at_login: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct ProfileFiles {
    root: PathBuf,
    // Set when a disk failure prevented re-establishing the recovery journal;
    // mutations stay blocked until a fresh process re-evaluates state.
    poisoned: Arc<AtomicBool>,
}

impl ProfileFiles {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            poisoned: Arc::new(AtomicBool::new(false)),
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.root.join(JOURNAL_FILE)
    }

    fn locked<T>(
        &self,
        operation: impl FnOnce(&std::fs::File) -> Result<T, String>,
    ) -> Result<T, String> {
        std::fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.root.join(LOCK_FILE))
            .map_err(|error| format!("profile lock is unavailable: {error}"))?;
        fs::flock(&lock, FlockOperation::LockExclusive)
            .map_err(|error| format!("profile lock is unavailable: {error}"))?;
        operation(&lock)
    }

    // true only when the journal exists as a regular file; inspection errors
    // and non-regular entries fail closed instead of reading as absent.
    fn journal_present(&self) -> Result<bool, String> {
        match std::fs::symlink_metadata(self.journal_path()) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
            Ok(_) => Err("profile import journal is not a regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!(
                "profile import journal cannot be inspected: {error}"
            )),
        }
    }

    fn assert_idle(&self) -> Result<(), String> {
        if self.poisoned.load(Ordering::SeqCst) {
            return Err(
                "profile import journal could not be re-established; relaunch to recover".into(),
            );
        }
        if self.journal_present()? {
            return Err(
                "a configuration import is unrecovered; relaunch to finish recovery".into(),
            );
        }
        Ok(())
    }

    fn read_document(
        &self,
        name: &str,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
        match std::fs::read(self.root.join(name)) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("{name} is malformed: {error}"))?;
                let Some(document) = value.as_object() else {
                    return Err(format!("{name} is malformed: not a JSON object"));
                };
                Ok(Some(document.clone()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("failed to read {name}: {error}")),
        }
    }

    fn load_key<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        key: &str,
    ) -> Result<Option<T>, String> {
        self.locked(|_| {
            self.assert_idle()?;
            self.read_document(name)?
                .and_then(|mut document| document.remove(key))
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| format!("{name} key {key} is malformed: {error}"))
        })
    }

    fn encode_document<T: Serialize>(
        &self,
        name: &str,
        key: &str,
        value: &T,
    ) -> Result<Vec<u8>, String> {
        let mut document = self.read_document(name)?.unwrap_or_default();
        document.insert(
            key.to_string(),
            serde_json::to_value(value).map_err(|error| error.to_string())?,
        );
        serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())
    }

    fn write_key<T: Serialize>(&self, name: &str, key: &str, value: &T) -> Result<(), String> {
        self.locked(|_| {
            self.assert_idle()?;
            let bytes = self.encode_document(name, key, value)?;
            write_durable(&self.root.join(name), &bytes)?;
            sync_dir(&self.root)
        })
    }

    fn read_image(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        match std::fs::read(self.root.join(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("failed to read {name}: {error}")),
        }
    }

    fn restore_image(&self, name: &str, before: &Option<Vec<u8>>) -> Result<(), String> {
        let path = self.root.join(name);
        match before {
            Some(bytes) => {
                let current = self.read_image(name)?;
                if current.as_ref() == Some(bytes) {
                    return Ok(());
                }
                write_durable(&path, bytes)
            }
            None => match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("failed to remove {name}: {error}")),
            },
        }
    }

    fn restore_login(
        &self,
        before: Option<bool>,
        autostart: &mut impl AutostartAdapter,
    ) -> Result<(), String> {
        let Some(expected) = before else {
            return Ok(());
        };
        // A crash can land right after the journal was written but before the
        // OS change; if the actual state is already correct, do not write it.
        let actual = autostart.is_enabled()?;
        if actual == expected {
            return Ok(());
        }
        autostart.set_enabled(expected)?;
        let confirmed = autostart.is_enabled()?;
        if confirmed != expected {
            return Err(format!(
                "launch at login rollback rejected by the OS: remains {confirmed}"
            ));
        }
        Ok(())
    }

    fn rollback(&self, before: &Before, autostart: &mut impl AutostartAdapter) -> Vec<String> {
        let mut errors = Vec::new();
        for (name, image) in [
            (PREFERENCES_FILE, &before.preferences),
            (POLICY_FILE, &before.policy),
        ] {
            if let Err(error) = self.restore_image(name, image) {
                errors.push(error);
            }
        }
        if let Err(error) = self.restore_login(before.launch_at_login, autostart) {
            errors.push(error);
        }
        if let Err(error) = sync_dir(&self.root) {
            errors.push(error);
        }
        errors
    }

    fn write_journal(&self, before: &Before) -> Result<(), String> {
        let bytes = serde_json::to_vec(before).map_err(|error| error.to_string())?;
        write_durable(&self.journal_path(), &bytes)?;
        sync_dir(&self.root)
    }

    fn clear_journal(&self) -> Result<(), String> {
        // The transaction knows it wrote the marker: its absence is an error,
        // not a successful clear.
        std::fs::remove_file(self.journal_path())
            .map_err(|error| format!("failed to clear the profile import journal: {error}"))?;
        sync_dir(&self.root)
            .map_err(|error| format!("failed to sync the profile import journal removal: {error}"))
    }

    fn read_journal(&self) -> Result<Before, String> {
        if !self.journal_present()? {
            return Err("profile import journal is absent".into());
        }
        let bytes = std::fs::read(self.journal_path())
            .map_err(|error| format!("profile import journal is unreadable: {error}"))?;
        let before: Before = serde_json::from_slice(&bytes)
            .map_err(|_| "profile import journal is unresolvable".to_string())?;
        for image in [&before.preferences, &before.policy].into_iter().flatten() {
            match serde_json::from_slice::<serde_json::Value>(image) {
                Ok(value) if value.is_object() => {}
                _ => return Err("profile import journal is unresolvable".into()),
            }
        }
        Ok(before)
    }

    // Roll back to the journal's before image, keeping a durable journal on
    // disk throughout so a crash mid-rollback can still be recovered.
    fn rollback_and_clear(
        &self,
        before: &Before,
        autostart: &mut impl AutostartAdapter,
    ) -> Result<(), String> {
        // Only re-arm when there isn't already a durable regular journal on
        // disk; the marker written by the transaction is still authoritative.
        let guard = self.journal_present().and_then(|present| {
            if present {
                Ok(())
            } else {
                self.write_journal(before)
            }
        });
        if let Err(error) = guard {
            self.poisoned.store(true, Ordering::SeqCst);
            return Err(format!(
                "profile rollback is indeterminate: the recovery journal could not be re-established: {error}"
            ));
        }
        let errors = self.rollback(before, autostart);
        if !errors.is_empty() {
            return Err(format!("profile rollback failed: {}", errors.join("; ")));
        }
        match self.clear_journal() {
            Ok(()) => Ok(()),
            Err(clear_error) => {
                // The remove may have landed without a durable directory sync;
                // re-arm the journal so recovery does not depend on luck.
                match self.write_journal(before) {
                    Ok(()) => Err(clear_error),
                    Err(rearm) => {
                        self.poisoned.store(true, Ordering::SeqCst);
                        Err(format!(
                            "{clear_error}; the recovery journal could not be re-established: {rearm}"
                        ))
                    }
                }
            }
        }
    }

    pub(crate) fn import(
        &self,
        next: &ConfigurationBundle,
        autostart: &mut impl AutostartAdapter,
        restore_auto: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        if next.version != 1 {
            return Err(format!(
                "Unsupported configuration bundle version: {}",
                next.version
            ));
        }
        preferences::validate(&next.preferences)?;
        crate::thermal_policy::settings::validate(&next.thermal_policy)?;
        self.locked(|_| {
            self.assert_idle()?;
            let before = Before {
                preferences: self.read_image(PREFERENCES_FILE)?,
                policy: self.read_image(POLICY_FILE)?,
                launch_at_login: {
                    let actual = autostart.is_enabled()?;
                    (actual != next.preferences.launch_at_login).then_some(actual)
                },
            };
            let preferences_bytes =
                self.encode_document(PREFERENCES_FILE, PREFERENCES_KEY, &next.preferences)?;
            let policy_bytes =
                self.encode_document(POLICY_FILE, POLICY_KEY, &next.thermal_policy)?;
            self.write_journal(&before)?;

            let commit = (|| -> Result<(), String> {
                write_durable(&self.root.join(PREFERENCES_FILE), &preferences_bytes)?;
                write_durable(&self.root.join(POLICY_FILE), &policy_bytes)?;
                if before.launch_at_login.is_some() {
                    autostart.set_enabled(next.preferences.launch_at_login)?;
                    let actual = autostart.is_enabled()?;
                    if actual != next.preferences.launch_at_login {
                        return Err(format!(
                            "launch at login rejected by the OS: remains {actual}"
                        ));
                    }
                }
                if next.thermal_policy.mode == ThermalPolicyMode::SystemAuto {
                    restore_auto()?;
                }
                // The committed pair must be durable before the journal is
                // cleared; the clear itself performs the final directory sync.
                sync_dir(&self.root)?;
                self.clear_journal()
            })();
            match commit {
                Ok(()) => Ok(()),
                Err(commit_error) => match self.rollback_and_clear(&before, autostart) {
                    Ok(()) => Err(commit_error),
                    Err(rollback_error) => Err(format!("{commit_error}; {rollback_error}")),
                },
            }
        })
    }

    pub(crate) fn recover(&self, autostart: &mut impl AutostartAdapter) -> Result<(), String> {
        self.locked(|_| {
            if !self.journal_present()? {
                return Ok(());
            }
            let before = self.read_journal()?;
            self.rollback_and_clear(&before, autostart)
        })
    }
}

fn write_durable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to stage {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("failed to sync {}: {error}", path.display()))?;
    file.persist(path)
        .map_err(|error| format!("failed to commit {}: {error}", path.display()))?;
    Ok(())
}

fn sync_dir(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| format!("failed to sync {}: {error}", path.display()))
}

impl PreferencesStore for ProfileFiles {
    fn load(&self) -> Result<Option<ApplicationPreferences>, String> {
        self.load_key(PREFERENCES_FILE, PREFERENCES_KEY)
    }

    fn with_writable<T>(&self, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        self.locked(|_| {
            self.assert_idle()?;
            operation()
        })
    }

    fn save(&mut self, preferences: &ApplicationPreferences) -> Result<(), String> {
        self.write_key(PREFERENCES_FILE, PREFERENCES_KEY, preferences)
    }
}

impl SettingsStore for ProfileFiles {
    fn load(&self) -> Result<Option<ThermalPolicySettings>, String> {
        self.load_key(POLICY_FILE, POLICY_KEY)
    }

    fn save(&mut self, settings: &ThermalPolicySettings) -> Result<(), String> {
        self.write_key(POLICY_FILE, POLICY_KEY, settings)
    }
}

#[cfg(test)]
mod tests {
    use super::{ProfileFiles, POLICY_FILE, PREFERENCES_FILE};
    use crate::application_preferences::contract::{ApplicationPreferences, ConfigurationBundle};
    use crate::application_preferences::preferences::{AutostartAdapter, PreferencesStore};
    use crate::thermal_policy::contract::ThermalPolicySettings;
    use crate::thermal_policy::state::SettingsStore;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "statpulse-profile-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[derive(Clone, Default)]
    struct FakeAutostart {
        enabled: Rc<RefCell<bool>>,
        fail_set: Rc<RefCell<Option<bool>>>,
        fail_read: Rc<RefCell<bool>>,
    }

    impl AutostartAdapter for FakeAutostart {
        fn is_enabled(&self) -> Result<bool, String> {
            if *self.fail_read.borrow() {
                return Err("login state unreadable".into());
            }
            Ok(*self.enabled.borrow())
        }

        fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
            if let Some(reject) = self.fail_set.borrow_mut().take() {
                return Err(format!("OS rejected launch at login {reject}"));
            }
            *self.enabled.borrow_mut() = enabled;
            Ok(())
        }
    }

    fn bundle(
        preferences: ApplicationPreferences,
        policy: ThermalPolicySettings,
    ) -> ConfigurationBundle {
        ConfigurationBundle {
            version: 1,
            preferences,
            thermal_policy: policy,
        }
    }

    fn no_restore() -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn store_roundtrips_and_preserves_unrelated_keys() {
        let root = fixture_root("roundtrip");
        let files = ProfileFiles::new(root.clone());
        let mut existing = ApplicationPreferences::default();
        existing.telemetry_interval_ms = 1500;
        let mut document = serde_json::Map::new();
        document.insert("other".into(), 42.into());
        document.insert(
            "preferences".into(),
            serde_json::to_value(&existing).unwrap(),
        );
        std::fs::write(
            root.join(PREFERENCES_FILE),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();

        let loaded: Option<ApplicationPreferences> = PreferencesStore::load(&files).unwrap();
        assert_eq!(loaded.unwrap().telemetry_interval_ms, 1500);

        let mut files = files.clone();
        let mut next = ApplicationPreferences::default();
        next.telemetry_interval_ms = 2500;
        PreferencesStore::save(&mut files, &next).unwrap();

        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(PREFERENCES_FILE)).unwrap()).unwrap();
        assert_eq!(document["other"], 42);
        assert_eq!(
            PreferencesStore::load(&files)
                .unwrap()
                .unwrap()
                .telemetry_interval_ms,
            2500
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn malformed_existing_document_refuses_save() {
        let root = fixture_root("malformed");
        std::fs::write(root.join(PREFERENCES_FILE), b"{not json").unwrap();
        let mut files = ProfileFiles::new(root.clone());

        assert!(PreferencesStore::save(&mut files, &ApplicationPreferences::default()).is_err());
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            b"{not json"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn import_commits_both_files_and_login() {
        let root = fixture_root("import");
        std::fs::write(
            root.join(PREFERENCES_FILE),
            br#"{"unrelated":"keep","preferences":{}}"#,
        )
        .unwrap();
        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;
        let policy = ThermalPolicySettings::default();
        let restored = Rc::new(RefCell::new(0_usize));
        let restored_count = restored.clone();

        files
            .import(
                &bundle(preferences.clone(), policy),
                &mut autostart,
                move || {
                    *restored_count.borrow_mut() += 1;
                    Ok(())
                },
            )
            .unwrap();

        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(PREFERENCES_FILE)).unwrap()).unwrap();
        assert_eq!(document["unrelated"], "keep");
        assert!(*autostart.enabled.borrow());
        assert_eq!(*restored.borrow(), 1);
        assert!(root.join(POLICY_FILE).exists());
        assert!(!files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn failed_second_phase_restores_byte_exact_files_and_login() {
        let root = fixture_root("rollback");
        let before_prefs = br#"{"unrelated":"keep","preferences":{"telemetry_interval_ms":1000}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();
        autostart.fail_set.replace(Some(true));
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;

        let error = files
            .import(
                &bundle(preferences, ThermalPolicySettings::default()),
                &mut autostart,
                no_restore,
            )
            .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert!(!root.join(POLICY_FILE).exists());
        assert!(!*autostart.enabled.borrow());
        assert!(!files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn startup_recovery_restores_interrupted_pair() {
        let root = fixture_root("recover");
        let before_prefs = br#"{"preferences":{"telemetry_interval_ms":1000}}"#;
        let before_policy = br#"{"settings":{"mode":"quiet","rules":[]}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        std::fs::write(root.join(POLICY_FILE), before_policy).unwrap();

        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;
        let restore_calls = Rc::new(RefCell::new(0_usize));
        let calls = restore_calls.clone();
        files
            .import(
                &bundle(preferences, ThermalPolicySettings::default()),
                &mut autostart,
                move || {
                    *calls.borrow_mut() += 1;
                    Err("actuation rejected".into())
                },
            )
            .unwrap_err();

        // The failed actuation already rolled the committed pair back and
        // cleared the marker; confirm that before simulating the crash.
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert_eq!(
            std::fs::read(root.join(POLICY_FILE)).unwrap(),
            before_policy
        );
        assert!(!*autostart.enabled.borrow());
        assert!(!files.journal_path().exists());

        // Simulate a crash during rollback: overwrite one file, delete the other,
        // and recreate the journal manually.
        std::fs::write(root.join(PREFERENCES_FILE), b"overwritten").unwrap();
        std::fs::remove_file(root.join(POLICY_FILE)).unwrap();
        std::fs::write(
            files.journal_path(),
            serde_json::to_vec(&serde_json::json!({
                "preferences": before_prefs.to_vec(),
                "policy": before_policy.to_vec(),
                "launch_at_login": false,
            }))
            .unwrap(),
        )
        .unwrap();

        let mut autostart = FakeAutostart {
            enabled: Rc::new(RefCell::new(true)),
            ..Default::default()
        };
        files.recover(&mut autostart).unwrap();

        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert_eq!(
            std::fs::read(root.join(POLICY_FILE)).unwrap(),
            before_policy
        );
        assert!(!*autostart.enabled.borrow());
        assert!(!files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unrecoverable_rollback_fails_writes_until_recovered() {
        let root = fixture_root("pending");
        let before_prefs = br#"{"preferences":{"telemetry_interval_ms":1000}}"#;
        let files = ProfileFiles::new(root.clone());
        std::fs::write(
            files.journal_path(),
            serde_json::to_vec(&serde_json::json!({
                "preferences": before_prefs.to_vec(),
                "policy": null,
                "launch_at_login": false,
            }))
            .unwrap(),
        )
        .unwrap();

        let mut blocked = FakeAutostart {
            enabled: Rc::new(RefCell::new(true)),
            fail_set: Rc::new(RefCell::new(Some(false))),
            ..Default::default()
        };
        assert!(files.recover(&mut blocked).is_err());
        assert!(files.journal_path().exists());
        assert!(
            PreferencesStore::save(&mut files.clone(), &ApplicationPreferences::default()).is_err()
        );
        assert!(
            SettingsStore::save(&mut files.clone(), &ThermalPolicySettings::default()).is_err()
        );
        assert!(PreferencesStore::load(&files).is_err());
        assert!(SettingsStore::load(&files).is_err());
        assert!(files
            .with_writable(|| -> Result<(), String> { Ok(()) })
            .is_err());

        let mut permitted = FakeAutostart {
            enabled: Rc::new(RefCell::new(true)),
            ..Default::default()
        };
        files.recover(&mut permitted).unwrap();
        assert!(!files.journal_path().exists());
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert!(!*permitted.enabled.borrow());
        assert!(
            PreferencesStore::save(&mut files.clone(), &ApplicationPreferences::default()).is_ok()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lost_commit_marker_rolls_back_instead_of_reporting_success() {
        let root = fixture_root("lost-marker");
        let before_prefs = br#"{"unrelated":"keep","preferences":{"telemetry_interval_ms":1000}}"#;
        let before_policy = br#"{"settings":{"mode":"quiet","rules":[]}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        std::fs::write(root.join(POLICY_FILE), before_policy).unwrap();
        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;
        let mut policy = ThermalPolicySettings::default();
        policy.mode = crate::thermal_policy::contract::ThermalPolicyMode::SystemAuto;

        let journal = files.journal_path();
        let error = files
            .import(&bundle(preferences, policy), &mut autostart, move || {
                // The marker vanished after both writes and the login
                // change: commit must not report success.
                std::fs::remove_file(&journal).unwrap();
                Ok(())
            })
            .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert_eq!(
            std::fs::read(root.join(POLICY_FILE)).unwrap(),
            before_policy
        );
        assert!(!*autostart.enabled.borrow());
        assert!(!files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unreadable_login_state_keeps_journal_instead_of_clearing() {
        let root = fixture_root("unreadable-login");
        let before_prefs = br#"{"unrelated":"keep","preferences":{"telemetry_interval_ms":1000}}"#;
        let before_policy = br#"{"settings":{"mode":"quiet","rules":[]}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        std::fs::write(root.join(POLICY_FILE), before_policy).unwrap();
        let files = ProfileFiles::new(root.clone());
        let fail_read = Rc::new(RefCell::new(false));
        let mut autostart = FakeAutostart {
            fail_read: fail_read.clone(),
            ..Default::default()
        };
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;

        let error = files
            .import(
                &bundle(preferences, ThermalPolicySettings::default()),
                &mut autostart,
                move || {
                    // From here the OS can no longer report the actual state:
                    // rollback must keep the journal rather than clear it.
                    *fail_read.borrow_mut() = true;
                    Err("actuation rejected".into())
                },
            )
            .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert_eq!(
            std::fs::read(root.join(POLICY_FILE)).unwrap(),
            before_policy
        );
        assert!(files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn corrupt_journal_blocks_recovery_without_touching_files_or_login() {
        let root = fixture_root("corrupt-journal");
        let before_prefs = br#"{"preferences":{"telemetry_interval_ms":1000}}"#;
        let before_policy = br#"{"settings":{"mode":"quiet","rules":[]}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        std::fs::write(root.join(POLICY_FILE), before_policy).unwrap();
        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();

        for journal in [
            serde_json::json!({}),
            serde_json::json!({"preferences": null, "policy": null}),
            serde_json::json!({"preferences": null, "policy": null, "launch_at_login": null, "extra": 1}),
            serde_json::json!({"preferences": [1, 2], "policy": null, "launch_at_login": null}),
        ] {
            std::fs::write(files.journal_path(), serde_json::to_vec(&journal).unwrap()).unwrap();
            assert!(files.recover(&mut autostart).is_err());
            assert!(files.journal_path().exists());
        }

        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert_eq!(
            std::fs::read(root.join(POLICY_FILE)).unwrap(),
            before_policy
        );
        assert!(!*autostart.enabled.borrow());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn invalid_bundle_has_no_effects() {
        let root = fixture_root("invalid");
        let before_prefs = br#"{"preferences":{"telemetry_interval_ms":1000}}"#;
        std::fs::write(root.join(PREFERENCES_FILE), before_prefs).unwrap();
        let files = ProfileFiles::new(root.clone());
        let mut autostart = FakeAutostart::default();
        let mut preferences = ApplicationPreferences::default();
        preferences.launch_at_login = true;
        let mut policy = ThermalPolicySettings::default();
        policy.mode = crate::thermal_policy::contract::ThermalPolicyMode::SystemAuto;

        let mut invalid_version = bundle(preferences.clone(), policy.clone());
        invalid_version.version = 2;
        assert!(files
            .import(&invalid_version, &mut autostart, no_restore)
            .is_err());

        let mut invalid_rules = bundle(preferences, policy);
        invalid_rules
            .thermal_policy
            .rules
            .push(crate::thermal_policy::contract::ThermalRule {
                id: "cpu".into(),
                name: "CPU".into(),
                target: crate::thermal_policy::contract::ThermalTarget::Cpu,
                low_celsius: 90.0,
                high_celsius: 80.0,
                min_fan_percent: 20,
                max_fan_percent: 100,
                active: true,
            });
        assert!(files
            .import(&invalid_rules, &mut autostart, no_restore)
            .is_err());

        assert_eq!(
            std::fs::read(root.join(PREFERENCES_FILE)).unwrap(),
            before_prefs
        );
        assert!(!*autostart.enabled.borrow());
        assert!(!files.journal_path().exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
