use crate::awake;
use crate::hardware_telemetry::contract::Availability;
use crate::ports;
use crate::system_extras;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

pub fn maybe_handle_cli_args() -> bool {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("-psn_") && a != "--autostart")
        .collect();

    if args.is_empty() {
        return false;
    }

    match args[0].as_str() {
        "--mcp" | "mcp" => {
            run_mcp_stdio_server();
            true
        }
        "status" | "--status" => {
            print_cli_status();
            true
        }
        "ports" | "--ports" => {
            print_cli_ports();
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
            println!("{}", format_prometheus_metrics());
            true
        }
        "kill" => {
            let Some(port_arg) = args.get(1) else {
                eprintln!("Usage: StatPulse kill <port>");
                std::process::exit(2);
            };
            let Ok(port_num) = port_arg.parse::<u16>() else {
                eprintln!("Invalid port number: {port_arg}");
                std::process::exit(2);
            };
            match kill_listening_port(port_num) {
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
    println!(
        "\
StatPulse — macOS System, Developer & Thermal Monitor

USAGE:
  StatPulse                   Launch menu bar GUI application
  StatPulse --mcp             Run JSON-RPC 2.0 MCP server over stdio
  StatPulse status            Print CPU/GPU temp, fans, memory, disk & AI agents
  StatPulse ports             List listening TCP ports, project folders & launchers
  StatPulse kill <port>       Terminate user-owned process listening on <port>
  StatPulse cleanup-orphans   Terminate orphaned dev servers (ppid == 1)
  StatPulse metrics           Export live telemetry in Prometheus / OpenMetrics format
"
    );
}

pub fn format_prometheus_metrics() -> String {
    system_extras::set_background_swr_enabled(false);
    let (snap, extras) = std::thread::scope(|s| {
        let extras_h = s.spawn(system_extras::collect_system_extras);
        let snap = crate::telemetry_snapshot();
        (snap, extras_h.join().unwrap_or_else(|_| system_extras::collect_system_extras()))
    });
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
    lines.push(format!(
        "statpulse_disk_free_gb {:.2}",
        extras.disk.free_gb
    ));
    lines.push(format!(
        "statpulse_network_rx_bytes_per_sec {:.0}",
        extras.network.rx_bytes_per_sec
    ));
    lines.push(format!(
        "statpulse_network_tx_bytes_per_sec {:.0}",
        extras.network.tx_bytes_per_sec
    ));
    lines.push(format!("statpulse_active_agents {}", extras.agents.len()));
    lines.join("\n")
}

fn print_cli_status() {
    system_extras::set_background_swr_enabled(false);
    let (snap, extras) = std::thread::scope(|s| {
        let extras_h = s.spawn(system_extras::collect_system_extras);
        let snap = crate::telemetry_snapshot();
        (snap, extras_h.join().unwrap_or_else(|_| system_extras::collect_system_extras()))
    });
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
            println!("Thermals : CPU {cpu} | GPU {gpu} ({} sensors)", value.sensors.len());
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
            println!("Fans     : {}", if fan_str.is_empty() { "None" } else { &fan_str });
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
            println!("{:<7} {:<8} {:<18} {:<16} {:<16} {}", "PORT", "PID", "PROCESS", "BIND", "LAUNCHER", "PROJECT");
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

fn kill_listening_port(port: u16) -> Result<String, String> {
    let list = ports::list_listening_ports()?;
    let targets: Vec<_> = list.into_iter().filter(|p| p.port == port).collect();
    if targets.is_empty() {
        return Err(format!("No listening TCP process found on port {port}."));
    }
    let mut terminated = Vec::new();
    for item in targets {
        system_extras::terminate_user_process(item.pid)?;
        terminated.push(format!("{} (PID {})", item.process_name, item.pid));
    }
    Ok(format!("Sent SIGTERM to {} on port {port}.", terminated.join(", ")))
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
                    "version": env!("CARGO_PKG_VERSION")
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
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

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
                        Some(p) => kill_listening_port(p).map(|msg| json!({ "message": msg })),
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
                "list_containers" => {
                    let extras = system_extras::collect_system_extras();
                    Ok(json!({ "containers": extras.containers }))
                }
                "get_prometheus_metrics" => Ok(json!({ "metrics": format_prometheus_metrics() })),
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
        let init = handle_mcp_request(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            .expect("initialize returns result");
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["serverInfo"]["name"], "statpulse-mcp");

        assert!(handle_mcp_request(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());

        let tools = handle_mcp_request(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .expect("tools/list returns tools");
        let list = tools["result"]["tools"].as_array().unwrap();
        assert_eq!(list.len(), 8);
        assert_eq!(list[0]["name"], "get_system_status");
        assert_eq!(list[1]["name"], "list_ports");
        assert_eq!(list[2]["name"], "kill_port");
        assert_eq!(list[3]["name"], "cleanup_orphans");
        assert_eq!(list[4]["name"], "inspect_process");
        assert_eq!(list[5]["name"], "unload_ollama_model");
        assert_eq!(list[6]["name"], "list_containers");
        assert_eq!(list[7]["name"], "get_prometheus_metrics");
    }
}
