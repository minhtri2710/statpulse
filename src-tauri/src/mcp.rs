use crate::awake;
use crate::hardware_telemetry::contract::Availability;
use crate::ports;
use crate::system_extras;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn init_cli_runtime_context() {
    system_extras::set_swr_blocking_mode(true);
    if let Ok(dir) = crate::profile_files::default_macos_app_data_dir() {
        let files = crate::profile_files::ProfileFiles::new(dir);
        if let Ok(Some(prefs)) =
            crate::application_preferences::preferences::PreferencesStore::load(&files)
        {
            system_extras::set_workspace_roots(prefs.workspace_roots);
        }
    }
}

pub fn maybe_handle_cli_args() -> bool {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("-psn_") && a != "--autostart")
        .collect();

    if args.is_empty() {
        return false;
    }

    let json_flag = args.iter().any(|a| a == "--json" || a == "-j");

    match args[0].as_str() {
        "--mcp" | "mcp" => {
            init_cli_runtime_context();
            run_mcp_stdio_server();
            true
        }
        "status" | "--status" => {
            init_cli_runtime_context();
            if json_flag {
                print_cli_status_json();
            } else {
                print_cli_status();
            }
            true
        }
        "ports" | "--ports" => {
            if json_flag {
                print_cli_ports_json();
            } else {
                print_cli_ports();
            }
            true
        }
        "audio" | "--audio" => {
            let sub = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if sub == "mute-mic" {
                match system_extras::toggle_mic_mute() {
                    Ok(a) => {
                        if json_flag {
                            println!("{}", serde_json::to_string_pretty(&a).unwrap_or_default());
                        } else {
                            println!("Mic {}", if a.input_muted { "muted" } else { "unmuted" });
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                let (outputs, inputs) = system_extras::list_audio_devices();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "output_devices": outputs,
                        "input_devices": inputs,
                    }))
                    .unwrap_or_default()
                );
            }
            true
        }
        "pause" | "resume" => {
            let paused = args[0] == "pause";
            let Some(pid_arg) = args.get(1) else {
                eprintln!("Usage: {} {} <pid>", env!("CARGO_PKG_NAME"), args[0]);
                std::process::exit(2);
            };
            let Ok(pid) = pid_arg.parse::<u32>() else {
                eprintln!("Invalid PID: {pid_arg}");
                std::process::exit(2);
            };
            match system_extras::set_process_paused(pid, paused) {
                Ok(()) => println!("{} PID {pid}.", if paused { "Paused" } else { "Resumed" }),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            true
        }
        "cleanup-orphans" | "--cleanup-orphans" => {
            match system_extras::cleanup_orphaned_dev_servers() {
                Ok(n) => println!("Cleaned up {n} orphaned dev server process(es)."),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            true
        }
        "metrics" | "--metrics" => {
            init_cli_runtime_context();
            println!("{}", format_prometheus_metrics());
            true
        }
        "export-csv" | "--export-csv" => {
            init_cli_runtime_context();
            println!("{}", format_telemetry_csv());
            true
        }
        "fan" => {
            let Some(mode_arg) = args.get(1).map(|s| s.as_str()) else {
                eprintln!(
                    "Usage: {} fan <auto|quiet|performance>",
                    env!("CARGO_PKG_NAME")
                );
                std::process::exit(2);
            };
            match apply_cli_thermal_mode(mode_arg) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            true
        }
        "kill" => {
            let Some(port_arg) = args.get(1) else {
                eprintln!("Usage: {} kill <port>", env!("CARGO_PKG_NAME"));
                std::process::exit(2);
            };
            let Ok(port_num) = port_arg.parse::<u16>() else {
                eprintln!("Invalid port number: {port_arg}");
                std::process::exit(2);
            };
            match ports::kill_listening_port(port_num) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            true
        }
        "--help" | "-h" | "help" => {
            print_help();
            true
        }
        _ => false,
    }
}

fn print_help() {
    let cli = env!("CARGO_PKG_NAME");
    println!(
        "\
StatPulse — macOS System, Developer & Thermal Monitor

USAGE:
  {cli}                   Launch menu bar GUI application
  {cli} --mcp             Run JSON-RPC 2.0 MCP server over stdio
  {cli} status [--json]   Print CPU/GPU temp, fans, memory, disk & AI agents
  {cli} ports [--json]    List listening TCP ports, project folders & launchers
  {cli} kill <port>       Terminate user-owned process listening on <port>
  {cli} pause <pid>       Freeze a user-owned process (SIGSTOP)
  {cli} resume <pid>      Resume a frozen process (SIGCONT)
  {cli} audio [mute-mic]  List CoreAudio devices (JSON) or toggle mic mute
  {cli} cleanup-orphans   Terminate orphaned dev servers (ppid == 1)
  {cli} metrics           Export live telemetry in Prometheus / OpenMetrics format
  {cli} export-csv        Print live hardware & power telemetry as CSV
  {cli} fan <mode>        Set Thermal policy (auto, quiet, performance)
"
    );
}

fn capture_full_status_pair() -> (
    crate::hardware_telemetry::contract::HardwareTelemetrySnapshot,
    system_extras::SystemExtrasSnapshot,
) {
    let snap = capture_seeded_with(crate::telemetry_snapshot);
    let extras = system_extras::collect_system_extras();
    (snap, extras)
}

fn capture_metrics_pair(
    include_agents_and_tokens: bool,
) -> (
    crate::hardware_telemetry::contract::HardwareTelemetrySnapshot,
    system_extras::SystemExtrasSnapshot,
) {
    let snap = capture_seeded_with(crate::hardware_telemetry::capture_metrics_now);
    let extras = system_extras::collect_metrics_extras(include_agents_and_tokens);
    (snap, extras)
}

// Telemetry capture that makes sure the Apple Silicon sampler has a baseline
// before extras read it: a cold process needs a 25 ms window between the seed
// and the real sample, an already-seeded persistent session does not.
fn capture_seeded_with(
    capture: fn() -> crate::hardware_telemetry::contract::HardwareTelemetrySnapshot,
) -> crate::hardware_telemetry::contract::HardwareTelemetrySnapshot {
    let t0 = std::time::Instant::now();
    let seeded = crate::macos_bridge::sample_apple_silicon_metrics().is_some();
    let snap = capture();
    if !seeded {
        if let Some(rem) = std::time::Duration::from_millis(25).checked_sub(t0.elapsed()) {
            std::thread::sleep(rem);
        }
    }
    snap
}

fn format_prometheus_metrics() -> String {
    let (snap, extras) = capture_metrics_pair(true);
    let mut lines = Vec::new();
    if let Availability::Available { value } = &snap.temperatures {
        if let Some(cpu) = value.cpu_celsius {
            lines.push(format!("statpulse_cpu_temperature_celsius {cpu:.2}"));
        }
        if let Some(gpu) = value.gpu_celsius {
            lines.push(format!("statpulse_gpu_temperature_celsius {gpu:.2}"));
        }
    }
    if let Availability::Available { value } = &snap.fans {
        for fan in value {
            lines.push(format!(
                "statpulse_fan_speed_rpm{{fan_id=\"{}\"}} {}",
                fan.id, fan.speed_rpm
            ));
        }
    }
    lines.push(format!(
        "statpulse_package_power_watts {:.2}",
        extras.silicon_power.package_watts
    ));
    lines.push(format!(
        "statpulse_memory_used_gb {:.2}",
        extras.memory.used_gb
    ));
    lines.push(format!("statpulse_disk_free_gb {:.2}", extras.disk.free_gb));
    lines.push(format!(
        "statpulse_network_rx_bytes_per_sec {:.0}",
        extras.network.rx_bytes_per_sec
    ));
    lines.push(format!(
        "statpulse_network_tx_bytes_per_sec {:.0}",
        extras.network.tx_bytes_per_sec
    ));
    lines.push(format!("statpulse_active_agents {}", extras.agents.len()));
    lines.push(format!(
        "statpulse_e_cluster_mhz {}",
        extras.silicon_power.e_cluster_mhz
    ));
    lines.push(format!(
        "statpulse_p_cluster_mhz {}",
        extras.silicon_power.p_cluster_mhz
    ));
    lines.push(format!(
        "statpulse_gpu_mhz {}",
        extras.silicon_power.gpu_mhz
    ));
    lines.push(format!(
        "statpulse_ai_tokens_24h {}",
        extras.ai_token_usage.window_24h_tokens
    ));
    lines.push(format!(
        "statpulse_dram_bandwidth_gbps {:.2}",
        extras.silicon_power.dram_bw_gbps
    ));
    lines.push(format!(
        "statpulse_disk_purgeable_gb {:.2}",
        extras.disk.purgeable_gb
    ));
    lines.join("\n")
}

fn apply_cli_thermal_mode(mode_raw: &str) -> Result<String, String> {
    use crate::thermal_policy::adapters::ProductionFanActuation;
    use crate::thermal_policy::contract::ThermalPolicyMode;
    use crate::thermal_policy::state::{ThermalPolicyChange, ThermalPolicyState};

    let mode = ThermalPolicyMode::parse_preset_arg(mode_raw)?;
    let mode_label = format!("{mode:?}");
    let dir = crate::profile_files::default_macos_app_data_dir()?;
    let profile_files = crate::profile_files::ProfileFiles::new(dir);
    let mut state = ThermalPolicyState::load(profile_files, ProductionFanActuation);
    state.update(ThermalPolicyChange::SelectMode(mode.clone()))?;
    if mode != ThermalPolicyMode::SystemAuto {
        let snap = crate::telemetry_snapshot();
        let _ = state.process_snapshot(&snap);
    }
    Ok(format!("Thermal policy set to {mode_label}."))
}

fn format_telemetry_csv() -> String {
    let (snap, extras) = capture_metrics_pair(false);
    let (cpu_c, gpu_c) = match &snap.temperatures {
        Availability::Available { value } => (
            value
                .cpu_celsius
                .map(|c| format!("{c:.1}"))
                .unwrap_or_default(),
            value
                .gpu_celsius
                .map(|c| format!("{c:.1}"))
                .unwrap_or_default(),
        ),
        _ => (String::new(), String::new()),
    };
    let fan0 = match &snap.fans {
        Availability::Available { value } => value
            .first()
            .map(|f| f.speed_rpm.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    format!(
        "timestamp_ms,cpu_c,gpu_c,fan0_rpm,pkg_w,cpu_w,gpu_w,e_mhz,p_mhz,gpu_mhz,mem_used_gb,swap_mb,disk_free_gb,rx_bps,tx_bps,dram_bw_gbps,disk_purgeable_gb\n{},{cpu_c},{gpu_c},{fan0},{:.1},{:.1},{:.1},{},{},{},{:.1},{:.1},{:.1},{:.0},{:.0},{:.1},{:.1}",
        snap.captured_at_unix_ms,
        extras.silicon_power.package_watts,
        extras.silicon_power.cpu_watts,
        extras.silicon_power.gpu_watts,
        extras.silicon_power.e_cluster_mhz,
        extras.silicon_power.p_cluster_mhz,
        extras.silicon_power.gpu_mhz,
        extras.memory.used_gb,
        extras.memory.swap_used_mb,
        extras.disk.free_gb,
        extras.network.rx_bytes_per_sec,
        extras.network.tx_bytes_per_sec,
        extras.silicon_power.dram_bw_gbps,
        extras.disk.purgeable_gb,
    )
}

fn print_cli_status_json() {
    let (snap, extras) = capture_full_status_pair();
    let ports = ports::list_listening_ports_fast().unwrap_or_default();
    let payload = json!({
        "telemetry": snap,
        "extras": extras,
        "ports_count": ports.len(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );
}

fn print_cli_ports_json() {
    match ports::list_listening_ports() {
        Ok(list) => println!(
            "{}",
            serde_json::to_string_pretty(&list).unwrap_or_default()
        ),
        Err(e) => {
            eprintln!("Failed to list ports: {e}");
            std::process::exit(1);
        }
    }
}

fn print_cli_status() {
    let (snap, extras) = capture_full_status_pair();
    let ports = ports::list_listening_ports_fast().unwrap_or_default();

    println!("=== StatPulse System Status ===");
    match &snap.temperatures {
        Availability::Available { value } => {
            let cpu = value
                .cpu_celsius
                .map(|c| format!("{c:.1}°C"))
                .unwrap_or_else(|| "--".into());
            let gpu = value
                .gpu_celsius
                .map(|c| format!("{c:.1}°C"))
                .unwrap_or_else(|| "--".into());
            println!(
                "Thermals : CPU {cpu} | GPU {gpu} ({} sensors)",
                value.sensors.len()
            );
        }
        Availability::NotPresent => println!("Thermals : Not present"),
        Availability::Unavailable { reason } => println!("Thermals : Unavailable ({reason})"),
    }

    match &snap.fans {
        Availability::Available { value } => {
            let fan_str = value
                .iter()
                .map(|f| format!("{}: {} RPM", f.label, f.speed_rpm))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "Fans     : {}",
                if fan_str.is_empty() { "None" } else { &fan_str }
            );
        }
        _ => println!("Fans     : Unavailable"),
    }

    println!(
        "Memory   : {:.1}/{:.0} GB (pressure: {}, swap: {:.0} MB)",
        extras.memory.used_gb,
        extras.memory.total_gb,
        extras.memory.pressure_level,
        extras.memory.swap_used_mb
    );
    println!(
        "Disk     : {:.0} GB free / {:.0} GB total ({}% used)",
        extras.disk.free_gb, extras.disk.total_gb, extras.disk.used_percent
    );
    println!(
        "Network  : {} ({}) | VPN: {}",
        extras.network.interface,
        extras.network.local_ip,
        extras.network.vpn_ip.as_deref().unwrap_or("Off")
    );
    println!(
        "Developer: {} listening port(s) | {} AI agent(s) | {} orphaned dev server(s)",
        ports.len(),
        extras.agents.len(),
        extras.orphaned_servers.len()
    );
}

fn print_cli_ports() {
    match ports::list_listening_ports() {
        Ok(list) if list.is_empty() => println!("No listening TCP ports found."),
        Ok(list) => {
            println!(
                "{:<7} {:<8} {:<18} {:<16} {:<16} PROJECT",
                "PORT", "PID", "PROCESS", "BIND", "LAUNCHER"
            );
            for p in list {
                println!(
                    "{:<7} {:<8} {:<18} {:<16} {:<16} {}",
                    p.port,
                    p.pid,
                    p.process_name,
                    p.bind_address,
                    p.launcher.as_deref().unwrap_or("-"),
                    p.project_path.as_deref().unwrap_or("-")
                );
            }
        }
        Err(e) => {
            eprintln!("Failed to list ports: {e}");
            std::process::exit(1);
        }
    }
}

fn run_mcp_stdio_server() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(raw) = line else {
            break;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(resp) = handle_mcp_request(trimmed) {
            let _ = serde_json::to_writer(&mut out, &resp);
            let _ = out.write_all(b"\n");
            let _ = out.flush();
        }
    }
}

pub fn handle_mcp_request(line: &str) -> Option<Value> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {e}") }
            }));
        }
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    if id.is_none() || method == "notifications/initialized" {
        return None;
    }
    let id = id.unwrap_or(Value::Null);

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "statpulse-mcp",
                    "version": env!("STATPULSE_VERSION")
                }
            }
        })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "get_system_status",
                        "description": "Get live macOS CPU/GPU temperatures, fan RPM, memory pressure, disk free space, network throughput, active AI coding agents, and orphaned dev servers.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "list_ports",
                        "description": "List all listening TCP ports with PID, process name, project folder (cwd), parent launcher (Claude Code, Cursor, Terminal, Docker), and LAN exposure.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "kill_port",
                        "description": "Terminate the user-owned process listening on a given TCP port.",
                        "inputSchema": {
                            "type": "object",
                            "properties": { "port": { "type": "integer", "minimum": 1, "maximum": 65535 } },
                            "required": ["port"]
                        }
                    },
                    {
                        "name": "cleanup_orphans",
                        "description": "Detect and terminate orphaned dev server processes (ppid == 1) such as lingering node, vite, next, or python workers.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "inspect_process",
                        "description": "Deep-inspect a running process by PID (architecture arm64 vs Rosetta, parent process, cwd, code signature, hardened runtime, and open sockets/files).",
                        "inputSchema": {
                            "type": "object",
                            "properties": { "pid": { "type": "integer", "minimum": 2 } },
                            "required": ["pid"]
                        }
                    },
                    {
                        "name": "unload_ollama_model",
                        "description": "Unload a local LLM model from Ollama Unified Memory/VRAM (`ollama stop <name>`) to free RAM.",
                        "inputSchema": {
                            "type": "object",
                            "properties": { "name": { "type": "string" } },
                            "required": ["name"]
                        }
                    },
                    {
                        "name": "list_containers",
                        "description": "List running Docker / OrbStack containers with image, status, and mapped ports.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "get_prometheus_metrics",
                        "description": "Return live StatPulse hardware, thermal, memory, disk, network, and AI-agent telemetry formatted as Prometheus / OpenMetrics text.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "probe_http_ports",
                        "description": "Probe listening localhost TCP ports for HTTP status codes and page <title> metadata.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "ports": {
                                    "type": "array",
                                    "items": { "type": "integer", "minimum": 1, "maximum": 65535 }
                                }
                            },
                            "required": ["ports"]
                        }
                    },
                    {
                        "name": "get_devices_health",
                        "description": "Get Bluetooth devices (battery & RSSI), USB tree, AC charger wattage, and battery health vitals.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "set_thermal_mode",
                        "description": "Switch StatPulse Thermal policy between system_auto, quiet, and performance.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "mode": { "type": "string", "enum": ["system_auto", "quiet", "performance"] }
                            },
                            "required": ["mode"]
                        }
                    },
                    {
                        "name": "get_ai_token_usage",
                        "description": "Get local AI coding agent token usage (5h/24h windows, estimated API cost), active agent sessions, and loaded local LLM models.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "export_telemetry_csv",
                        "description": "Return a CSV snapshot of live hardware temperatures, fan RPM, Apple Silicon power/clock rails, memory, disk, and network rates.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "set_process_paused",
                        "description": "Freeze (SIGSTOP) or resume (SIGCONT) a user-owned process by PID to curb background CPU/heat without losing state.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "pid": { "type": "integer", "minimum": 2 },
                                "paused": { "type": "boolean" }
                            },
                            "required": ["pid", "paused"]
                        }
                    },
                    {
                        "name": "list_audio_devices",
                        "description": "List available CoreAudio output and input devices with current default flags.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "set_audio_device",
                        "description": "Switch the macOS default CoreAudio output or input device by ID.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "device_id": { "type": "integer", "minimum": 1 },
                                "kind": { "type": "string", "enum": ["output", "input"] }
                            },
                            "required": ["device_id", "kind"]
                        }
                    },
                    {
                        "name": "toggle_mic_mute",
                        "description": "Toggle the default microphone input mute state.",
                        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                    },
                    {
                        "name": "clean_stale_caches",
                        "description": "Safely clean rebuildable developer/AI caches older than min_age_days (minimum 7 days).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "min_age_days": { "type": "integer", "minimum": 7 }
                            },
                            "required": ["min_age_days"]
                        }
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let tool_result = match tool_name {
                "get_system_status" => {
                    let telemetry = crate::telemetry_snapshot();
                    let extras = system_extras::collect_system_extras();
                    let sleep_assertions = awake::read_sleep_assertions();
                    Ok(json!({
                        "telemetry": telemetry,
                        "extras": extras,
                        "sleep_assertions": sleep_assertions
                    }))
                }
                "list_ports" => ports::list_listening_ports().map(|p| json!({ "ports": p })),
                "kill_port" => {
                    let port_num = args
                        .get("port")
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u16::try_from(n).ok());
                    match port_num {
                        Some(p) => {
                            ports::kill_listening_port(p).map(|msg| json!({ "message": msg }))
                        }
                        None => Err("Missing or invalid integer 'port' argument".into()),
                    }
                }
                "cleanup_orphans" => system_extras::cleanup_orphaned_dev_servers()
                    .map(|count| json!({ "terminated_count": count })),
                "inspect_process" => {
                    let pid = args
                        .get("pid")
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u32::try_from(n).ok());
                    match pid {
                        Some(p) => system_extras::inspect_process_details(p)
                            .and_then(|info| serde_json::to_value(info).map_err(|e| e.to_string())),
                        None => Err("Missing or invalid integer 'pid' argument".into()),
                    }
                }
                "unload_ollama_model" => {
                    let model_name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    system_extras::unload_ollama_model(model_name)
                        .map(|models| json!({ "remaining_models": models }))
                }
                "list_containers" => Ok(json!({
                    "containers": system_extras::collect_containers()
                })),
                "get_prometheus_metrics" => Ok(json!({ "metrics": format_prometheus_metrics() })),
                "probe_http_ports" => {
                    let port_list: Vec<u16> = args
                        .get("ports")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_u64().and_then(|n| u16::try_from(n).ok()))
                                .collect()
                        })
                        .unwrap_or_default();
                    if port_list.is_empty() {
                        Err("Missing or empty 'ports' integer array argument".into())
                    } else {
                        Ok(json!({ "probes": system_extras::probe_localhost_ports(&port_list) }))
                    }
                }
                "get_devices_health" => {
                    let devices = system_extras::collect_devices_snapshot();
                    serde_json::to_value(devices).map_err(|e| e.to_string())
                }
                "set_thermal_mode" => {
                    let mode_str = args.get("mode").and_then(|v| v.as_str()).unwrap_or("");
                    apply_cli_thermal_mode(mode_str).map(|msg| json!({ "message": msg }))
                }
                "get_ai_token_usage" => {
                    let (ai_token_usage, agents, local_models) =
                        system_extras::collect_ai_usage_snapshot();
                    Ok(json!({
                        "ai_token_usage": ai_token_usage,
                        "agents": agents,
                        "local_models": local_models,
                    }))
                }
                "export_telemetry_csv" => Ok(json!({ "csv": format_telemetry_csv() })),
                "set_process_paused" => {
                    let pid = args
                        .get("pid")
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u32::try_from(n).ok());
                    let paused = args.get("paused").and_then(|v| v.as_bool());
                    match (pid, paused) {
                        (Some(p), Some(flag)) => system_extras::set_process_paused(p, flag)
                            .map(|()| json!({ "pid": p, "paused": flag })),
                        _ => Err("Missing or invalid 'pid' (integer) or 'paused' (boolean)".into()),
                    }
                }
                "list_audio_devices" => {
                    let (output_devices, input_devices) = system_extras::list_audio_devices();
                    Ok(json!({
                        "output_devices": output_devices,
                        "input_devices": input_devices
                    }))
                }
                "set_audio_device" => {
                    let dev_id = args
                        .get("device_id")
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u32::try_from(n).ok());
                    let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    match dev_id {
                        Some(id) => system_extras::set_audio_device(id, kind)
                            .and_then(|a| serde_json::to_value(a).map_err(|e| e.to_string())),
                        None => Err("Missing or invalid integer 'device_id' argument".into()),
                    }
                }
                "toggle_mic_mute" => system_extras::toggle_mic_mute()
                    .and_then(|a| serde_json::to_value(a).map_err(|e| e.to_string())),
                "clean_stale_caches" => {
                    let min_days = args
                        .get("min_age_days")
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(30)
                        .max(7);
                    system_extras::clean_stale_rebuildable_caches(min_days).map(
                        |(cleaned_count, remaining)| {
                            json!({ "cleaned_count": cleaned_count, "remaining": remaining })
                        },
                    )
                }
                _ => Err(format!("Unknown tool: {tool_name}")),
            };

            match tool_result {
                Ok(val) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string())
                        }]
                    }
                })),
                Err(err_msg) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "isError": true,
                        "content": [{ "type": "text", "text": err_msg }]
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Method not found: {method}") }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_handles_initialize_notification_and_tools_list() {
        let init =
            handle_mcp_request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
                .expect("initialize returns result");
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "statpulse-mcp");

        assert!(
            handle_mcp_request(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .is_none()
        );

        let tools = handle_mcp_request(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .expect("tools/list returns tools");
        let list = tools["result"]["tools"].as_array().unwrap();
        // The protocol does not guarantee tool list order, so compare sorted sets.
        let mut names: Vec<&str> = list
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "clean_stale_caches",
                "cleanup_orphans",
                "export_telemetry_csv",
                "get_ai_token_usage",
                "get_devices_health",
                "get_prometheus_metrics",
                "get_system_status",
                "inspect_process",
                "kill_port",
                "list_audio_devices",
                "list_containers",
                "list_ports",
                "probe_http_ports",
                "set_audio_device",
                "set_process_paused",
                "set_thermal_mode",
                "toggle_mic_mute",
                "unload_ollama_model",
            ]
        );
    }
}
