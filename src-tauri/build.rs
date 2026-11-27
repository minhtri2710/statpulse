fn main() {
    // Release validation uses the build's version and canonical bundle identity.
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../package.json")).expect("valid package.json");
    let version = manifest["version"].as_str().expect("package.json version");
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rustc-env=STATPULSE_VERSION={version}");
    let config: serde_json::Value =
        serde_json::from_str(include_str!("tauri.conf.json")).expect("valid tauri.conf.json");
    let identifier = config["identifier"]
        .as_str()
        .expect("Tauri bundle identifier");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rustc-env=STATPULSE_BUNDLE_ID={identifier}");

    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/smc/smc.c")
            .file("src/power_events.m")
            .include("src/smc")
            .warnings(false)
            .compile("smc");
        println!("cargo:rustc-link-lib=framework=IOKit");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Foundation");

        cc::Build::new()
            .file("src/system_extras/cache_permissions.c")
            .warnings(false)
            .compile("cache_permissions");

        cc::Build::new()
            .file("src/macos_bridge.m")
            .flag("-fobjc-arc")
            .warnings(false)
            .compile("macos_bridge");
        println!("cargo:rustc-link-lib=IOReport");
        println!("cargo:rustc-link-lib=framework=CoreWLAN");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=Carbon");
        println!("cargo:rustc-link-lib=framework=UserNotifications");

        println!("cargo:rerun-if-changed=src/smc/smc.c");
        println!("cargo:rerun-if-changed=src/power_events.m");
        println!("cargo:rerun-if-changed=src/macos_bridge.m");
        println!("cargo:rerun-if-changed=src/system_extras/cache_permissions.c");
    }

    tauri_build::build();
}
