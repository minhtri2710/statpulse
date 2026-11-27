#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;

#[test]
fn native_restore_checks_firmware_without_iokit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().unwrap();
    let binary = tmp.path().join("smc-restore");
    assert!(Command::new("/usr/bin/cc")
        .current_dir(root)
        .args([
            "-Wno-deprecated-declarations",
            "-DIOConnectCallStructMethod=statpulse_fixture_smc",
            "-Dusleep=statpulse_fixture_usleep",
            "src/smc/smc.c",
            "tests/smc_restore.c",
            "-framework",
            "IOKit",
            "-framework",
            "CoreFoundation",
            "-o",
        ])
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(binary).status().unwrap().success());
}
