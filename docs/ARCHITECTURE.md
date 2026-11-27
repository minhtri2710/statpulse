# StatPulse Developer & Architecture Guide

## System shape

```text
React UI (`src/App.tsx`, `src/features/*`)
  └─ generated TypeScript interfaces (`src/generated/*`, `src/types.ts`)
       │ Tauri commands and telemetry events
       ▼
Rust backend (`src-tauri/src/`)
  ├─ Application preferences & profile journal (`application_preferences/*`, `profile_files.rs`)
  │    ├─ `ProfileFiles` atomic journaled JSON store & config bundle export/import
  │    └─ `MacLoginItem` (`login_item.applescript`) macOS launch-at-login adapter
  ├─ Hardware telemetry snapshot (`hardware_telemetry/*`, `smc/*`, `power_events.m`, `power_management.rs`)
  │    ├─ SMC adapter (`smc.c` / `smc.h`)
  │    ├─ IOKit adapter (`IOPowerSources`, `AppleSmartBattery`, `AppleSmartBatteryPack` fallback)
  │    ├─ Process adapter (`ps` top thermal load with `.app` bundle roll-up)
  │    └─ fixture adapter for tests
  ├─ System extras & developer observability (`system_extras.rs`, `system_extras/*`, `ports.rs`, `awake.rs`, `alerts.rs`, `proc_info.rs`)
  │    ├─ `storage.rs`: APFS Strict Free vs Purgeable disk metrics & SSD health
  │    ├─ `cache_cleanup.rs` + `cache_permissions.c`: descriptor-pinned (`openat` + `O_NOFOLLOW`) cache inspection & cleanup
  │    ├─ `developer_tools.rs`: AI agent sessions, multi-worktree tracking, token burn/cost, Ollama, containers, custom scripts
  │    ├─ `network.rs`: interface throughput, Wi-Fi diagnostics, speed test, app connections, SSH/cloudflared/ngrok/kubectl/Tailscale tunnels
  │    ├─ `devices.rs` & `audio.rs`: Bluetooth, USB, CoreAudio device switching & mic mute
  │    └─ `ports.rs`, `awake.rs`, `alerts.rs`: listening TCP ports, Keep Awake rules, threshold/runaway alert evaluator
  ├─ Native windowing, tray & macOS bridge (`native_window.rs`, `menu_bar.rs`, `tray_menu.rs`, `macos_bridge.{rs,m}`)
  │    ├─ `LSUIElement` / `ActivationPolicy::Accessory` + `tauri-plugin-positioner` (`Position::TrayBottomCenter`)
  │    ├─ Popover + multi-display notch-aware Dynamic Island (`native_window.rs`)
  │    ├─ @2x Retina RGBA menu bar icon & Traffic Light severity styling (`menu_bar.rs`, `tray_menu.rs`)
  │    └─ `macos_bridge.m`: `IOReport` SoC power & AMC DRAM bandwidth, CoreAudio, CoreWLAN, Carbon hotkeys, `UserNotifications`
  ├─ Thermal policy (`thermal_policy/*`, `shared/thermal-presets.json`)
  │    ├─ authoritative settings and runtime state
  │    ├─ pure Hardware telemetry snapshot → Fan plan evaluator
  │    └─ internal settings-store and Fan-actuation adapters
  ├─ Fan actuation (`fan_actuation/*`, `bin/fan-actuation-daemon.rs`, `bundle/macos/fan-actuation/*`)
  │    ├─ administrator-authorized installer adapter
  │    └─ Unix socket → privileged launchd daemon → Apple SMC
  └─ CLI, MCP & App updater (`mcp.rs`, `app_update.rs`)
       ├─ one-shot CLI (`status`, `ports`, `metrics`, `export-csv`, `audio`, `fan`, etc.) & `--mcp` stdio JSON-RPC server
       └─ official DMG SHA-256 & bundle-ID (`com.statpulse.app`) verification, staged replacement, two-phase startup ack
```

The Application preferences module owns display temperature unit, telemetry cadence, launch-at-login state, menu bar style, Traffic Light menu bar behavior, popover look, layout density, Dynamic Island display mode, chart palette, Keep Awake rules, global/mic shortcuts, alert thresholds, auto-thermal triggers, and workspace roots. Its interface returns one authoritative snapshot and accepts one tagged change (plus full preference replacement during profile import). It validates supported cadence values, persists preferences atomically via `ProfileFiles`, reconciles launch-at-login with macOS via `MacLoginItem`, refreshes the tray presentation, and notifies the telemetry loop when cadence changes.

The Hardware telemetry snapshot module normalizes temperatures, fan measurements, battery measurements, top CPU-consuming applications (rolled up by `.app` bundle), and availability. Its interface uses explicit Celsius, RPM, percent, watts, megabytes, and capture-time fields. Each hardware group reports `available`, `not_present`, or `unavailable`; the module does not invent display values.

The Menu bar renderer (`src-tauri/src/menu_bar.rs`) translates each Hardware telemetry snapshot and Application preferences snapshot into a macOS tray title and @2x Retina RGBA status icon (`CompactText`, `CombinedTempFan`, `CombinedTempPower`, `StackedReadout`, `TachometerGauge`, `SparklineMini`, `PerCoreBars`, `BarChart`, `Pie`, `Ring`, `Meter`, `UpDownGraph`, or `BatteryIcon`) with optional Traffic Light (`<60°C` monochrome template, amber/rose under thermal load).

The Windowing & Activation Policy module configures StatPulse as a macOS Accessory application. Tauri supplies the main bundle identifier (`com.statpulse.app`); Cargo supplies the executable (`Contents/MacOS/statpulse`). The source `Info.plist` sets `LSUIElement` to `true`, and Rust setup calls `set_activation_policy(tauri::ActivationPolicy::Accessory)`. The app icon is hidden from the macOS Dock and Cmd+Tab app switcher. Tray clicks and `toggle_popover` invoke `window.move_window(Position::TrayBottomCenter)` from `tauri-plugin-positioner` to position the frameless window directly underneath the menu bar icon.

The Thermal policy module is authoritative for current settings, atomic settings transitions, direct Fan actuation policy gating, and runtime processing of each Hardware telemetry snapshot. It owns evaluator history, changed-target suppression, heartbeat behavior, and fail-safe restoration while coordinating persistence and Fan actuation through internal adapters. The evaluator remains pure: it translates one Hardware telemetry snapshot and authoritative settings into one Fan plan without transport, persistence, or socket behavior. Hardware telemetry capture and cadence remain outside the module. TypeScript interfaces are generated from the Rust contracts with `npm run generate:types`.

The Fan actuation module owns privileged writes. An administrator-authorized installer places the helper in `/Library/PrivilegedHelperTools` and its traditional plist in `/Library/LaunchDaemons`, with transactional rollback on launchd failure. The application communicates with the installed daemon through a narrow Unix socket protocol. The daemon authorizes the active console user, validates targets against hardware ranges, and restores all fans to System Auto when communication or its heartbeat lease fails.

## Thermal policy behavior

- System Auto always produces a System Auto Fan plan.
- Missing, unavailable, or more than five-second-old hardware data produces System Auto.
- Quiet uses 50–85°C and 20–75% fan output.
- Performance uses 40–75°C and 40–100% fan output.
- Custom rules can target CPU, GPU, a sensor key, or the hottest available temperature.
- When multiple rules apply, the highest requested fan target wins.
- RPM increases apply immediately.
- RPM decreases require a 2°C temperature drop and are limited to 400 RPM per second.
- Direct manual controls are disabled while Quiet, Performance, or Custom policy is active, preserving one writer for Fan actuation.

## Tauri commands

Registered via `tauri::generate_handler!` in [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs) (with updater transactions in [`src-tauri/src/app_update.rs`](../src-tauri/src/app_update.rs)):

- **Preferences & Configuration**: `application_preferences`, `confirmed_application_preferences`, `update_application_preferences`, `export_configuration_bundle`, `import_configuration_bundle`
- **Telemetry, Thermal Policy & Fan Actuation**: `fetch_telemetry`, `thermal_policy_settings`, `select_thermal_policy_mode`, `upsert_thermal_rule`, `delete_thermal_rule`, `fan_actuation_status`, `set_fan_speed`, `set_fan_mode`, `install_fan_actuation_helper`, `start_fan_dust_blast`, `cancel_fan_dust_blast`
- **Windowing & Dynamic Island**: `toggle_popover`, `set_island_collapsed`, `dismiss_dynamic_island`, `expand_island_on_display`
- **Processes, Ports, Scripts & Quick Actions**: `terminate_process`, `set_process_paused`, `inspect_process`, `cleanup_orphaned_servers`, `fetch_listening_ports`, `probe_http_ports`, `open_in_editor`, `reveal_in_finder`, `list_user_scripts`, `run_user_script`, `run_quick_switch`, `trigger_custom_alert`
- **System Extras, Devices, Network & Cleanup**: `fetch_system_extras`, `fetch_pill_extras`, `fetch_devices_snapshot`, `fetch_app_connections`, `run_speed_test`, `run_disk_benchmark`, `run_system_maintenance`, `clean_rebuildable_cache`, `clean_stale_caches`, `unload_ollama_model`, `control_container`
- **Keep Awake, Audio & Media**: `fetch_awake_status`, `set_keep_awake`, `set_keep_awake_rules`, `update_audio_volume`, `set_audio_device`, `toggle_mic_mute`, `control_media_playback`
- **App Updates**: `check_for_app_update`, `prepare_app_update`, `install_prepared_app_update`, `discard_prepared_app_update`, `confirm_update_startup`

## Safety & runtime invariants

- **Thermal & sleep safety**: Telemetry older than 5 seconds, clock rollbacks, unavailable hardware, Dust Blast token expiration, and macOS sleep (checked under the policy decision lock) all yield System Auto.
- **SMC & socket bounds**: Firmware writes require hardware acknowledgement (not elapsed time alone) and bounded Unix-socket deadlines so idle or slow clients never delay System Auto restoration.
- **Profile & updater transactions**: `ProfileFiles` uses atomic journaled writes with startup recovery; `app_update.rs` verifies official DMG byte length, SHA-256 digest, and `com.statpulse.app` bundle identity, with handshake timeout and rollback on failure.
- **Safe cache cleanup**: `cache_cleanup.rs` and `cache_permissions.c` pin directory descriptors (`openat` + `O_NOFOLLOW`), enforce filesystem boundaries and user ownership/ACL checks, and keep shared-cache vs project-root removal semantics distinct.
- **SWR cache separation**: Snapshot reuse (1.5 s) and port caching (2.5 s) stay separate from full collection; the fast/cached-only pill path never triggers full diagnostics, and one-shot CLI invocations disable background SWR refreshes.
- **Test lanes**: Deterministic unit, fixture, and C firmware mock (`tests/smc_restore.rs`) suites run by default; live-host sensor diagnostics and the `cache_cost_fixture.rs` `du` timing benchmark remain `#[ignore]` opt-ins.

## SMC and IOKit readings

Temperature adapters probe known Apple Silicon and Intel SMC keys. Fan measurements use `F*Ac`, `F*Mn`, `F*Mx`, `F*Tg`, and `F*Md`. Battery measurements use `IOPowerSources` and `AppleSmartBattery` properties, with fallback probing of `AppleSmartBatteryPack` (and nested `BatteryData` dictionaries). Unreadable optional measurements remain absent rather than receiving fallback values.

## Fail-safe authority

macOS remains the fail-safe authority through System Auto. Thermal policy evaluation, persistence, socket errors, stale telemetry, unavailable hardware, application exit, daemon restart, and heartbeat timeout must never leave an unowned manual Fan plan active.

## CI/CD Pipeline

- [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs `npm test`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, and generated-type drift checks on `main` pushes and pull requests.
- [`.github/workflows/build-macos.yml`](../.github/workflows/build-macos.yml) builds Universal macOS binary targets (`aarch64-apple-darwin` and `x86_64-apple-darwin`), verifies fan actuation helper bundling integrity, and packages `.app.tar.gz` and `.dmg` release artifacts on `v*` tags.
