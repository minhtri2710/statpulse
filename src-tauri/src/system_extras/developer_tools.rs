// Developer-tooling probes: AI agent sessions, orphaned dev servers and
// process termination/inspection, containers and Ollama models, launch
// agents, quick switches, and maintenance actions.

use super::{parse_ps_rows, split_token, PsRow, SwrCache};
use crate::ports::{self, ListeningPort};
use crate::proc_info::{
    canonical_process_name, classify_ai_agent, cmd_stdout, is_dev_server_binary, round1, round2,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AgentSessionItem {
    pub agent_name: String,
    pub primary_pid: u32,
    pub process_count: u32,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub uptime: String,
    pub ports: Vec<u16>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct LaunchAgentItem {
    pub label: String,
    pub plist_path: String,
    pub program: String,
    pub scope: String,
    pub signed: bool,
    pub authority: Option<String>,
    pub binary_exists: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct QuickSwitchState {
    pub dark_mode: bool,
    pub show_hidden_files: bool,
    pub show_desktop_icons: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct OrphanedProcessItem {
    pub pid: u32,
    pub name: String,
    pub command: String,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    #[ts(type = "number | null")]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProcessInspection {
    pub pid: u32,
    pub name: String,
    pub executable_path: String,
    pub cwd: Option<String>,
    pub arch: String,
    pub parent_pid: u32,
    pub parent_name: String,
    pub user: String,
    pub cpu_time: String,
    pub elapsed_time: String,
    pub team_id: Option<String>,
    pub authority: Option<String>,
    pub notarized: bool,
    pub hardened_runtime: bool,
    pub sandboxed: bool,
    pub open_files: Vec<String>,
}

fn default_ollama_runtime() -> String {
    "Ollama".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct OllamaModelItem {
    pub name: String,
    pub id: String,
    pub size_gb: f64,
    pub processor: String,
    pub until: String,
    #[serde(default = "default_ollama_runtime")]
    pub runtime: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AiTokenUsageSummary {
    #[ts(type = "number")]
    pub window_5h_tokens: u64,
    #[ts(type = "number")]
    pub window_24h_tokens: u64,
    #[ts(type = "number")]
    pub input_tokens_24h: u64,
    #[ts(type = "number")]
    pub output_tokens_24h: u64,
    #[ts(type = "number")]
    pub cache_read_tokens_24h: u64,
    pub estimated_cost_usd_24h: f64,
    pub active_sessions_24h: u32,
    pub latest_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ContainerItem {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    pub runtime: String,
}

static OLLAMA_CACHE: SwrCache<Vec<OllamaModelItem>> = SwrCache::new();

static CONTAINER_CACHE: SwrCache<Vec<ContainerItem>> = SwrCache::new();

static LAUNCH_AGENTS_CACHE: SwrCache<Vec<LaunchAgentItem>> = SwrCache::new();

static QUICK_SWITCH_CACHE: SwrCache<QuickSwitchState> = SwrCache::new();

static AI_TOKENS_CACHE: SwrCache<AiTokenUsageSummary> = SwrCache::new();

// Cheap yes/no probe for the keep-awake rule: same command classifier as the
// session list, but no ports, SSD, or network collection.
pub fn ai_agent_active() -> bool {
    cmd_stdout("/bin/ps", &["-axo", "comm="]).is_some_and(|raw| {
        raw.lines()
            .any(|comm| classify_ai_agent(comm.trim()).is_some())
    })
}

pub fn detect_ai_agents(ps_raw: &str, ports: &[ListeningPort]) -> Vec<AgentSessionItem> {
    let rows = parse_ps_rows(ps_raw);
    let mut children_by_ppid: HashMap<u32, Vec<&PsRow>> = HashMap::new();
    for row in &rows {
        children_by_ppid.entry(row.ppid).or_default().push(row);
    }

    let mut ports_by_pid: HashMap<u32, Vec<u16>> = HashMap::new();
    for p in ports {
        ports_by_pid.entry(p.pid).or_default().push(p.port);
    }

    let mut grouped: BTreeMap<String, Vec<&PsRow>> = BTreeMap::new();
    for row in &rows {
        if let Some(agent_name) = classify_ai_agent(&row.comm) {
            grouped.entry(agent_name.to_string()).or_default().push(row);
        }
    }

    let mut sessions = Vec::new();
    for (agent_name, roots) in grouped {
        let mut root_pids: HashSet<u32> = roots.iter().map(|r| r.pid).collect();
        let mut all_pids: HashSet<u32> = root_pids.clone();

        for _ in 0..2 {
            let mut next_pids = Vec::new();
            for pid in &root_pids {
                if let Some(children) = children_by_ppid.get(pid) {
                    for child in children {
                        if all_pids.insert(child.pid) {
                            next_pids.push(child.pid);
                        }
                    }
                }
            }
            root_pids = next_pids.into_iter().collect();
        }

        let mut cpu_sum = 0.0;
        let mut rss_sum = 0_u64;
        let mut primary_pid = 0_u32;
        let mut best_cpu = -1.0_f64;
        let mut uptime = String::from("0m");
        let mut child_names: Vec<String> = Vec::new();
        let mut seen_children = HashSet::new();
        let mut agent_ports = Vec::new();
        let mut seen_ports = HashSet::new();

        for row in &rows {
            if !all_pids.contains(&row.pid) {
                continue;
            }
            cpu_sum += row.pcpu;
            rss_sum = rss_sum.saturating_add(row.rss_kb);
            if classify_ai_agent(&row.comm).is_some() && row.pcpu >= best_cpu {
                best_cpu = row.pcpu;
                primary_pid = row.pid;
                uptime = row.etime.clone();
            } else {
                let cname = canonical_process_name(&row.comm);
                if cname != agent_name && seen_children.insert(cname.clone()) {
                    child_names.push(cname);
                }
            }
            if let Some(ps) = ports_by_pid.get(&row.pid) {
                for &port in ps {
                    if seen_ports.insert(port) {
                        agent_ports.push(port);
                    }
                }
            }
        }

        agent_ports.sort_unstable();
        child_names.truncate(4);
        sessions.push(AgentSessionItem {
            agent_name,
            primary_pid,
            process_count: all_pids.len() as u32,
            cpu_percent: round1(cpu_sum),
            memory_mb: round1(rss_sum as f64 / 1024.0),
            uptime,
            ports: agent_ports,
            children: child_names,
        });
    }

    sessions.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    sessions
}

pub fn detect_orphaned_dev_servers(
    ps_raw: &str,
    ports: &[ListeningPort],
) -> Vec<OrphanedProcessItem> {
    let mut port_by_pid = HashMap::new();
    for p in ports {
        port_by_pid.entry(p.pid).or_insert(p.port);
    }

    let mut items = Vec::new();
    for row in parse_ps_rows(ps_raw) {
        if row.ppid != 1 || row.pid <= 1 {
            continue;
        }
        if row.comm.contains(".app/Contents/") {
            continue;
        }
        if is_dev_server_binary(&row.comm) {
            items.push(OrphanedProcessItem {
                pid: row.pid,
                name: canonical_process_name(&row.comm),
                command: row.comm,
                cpu_percent: round1(row.pcpu),
                memory_mb: round1(row.rss_kb as f64 / 1024.0),
                port: port_by_pid.get(&row.pid).copied(),
            });
        }
    }
    items.sort_by(|a, b| b.memory_mb.total_cmp(&a.memory_mb));
    items
}

pub fn terminate_user_process(pid: u32) -> Result<(), String> {
    if pid <= 1 || pid == std::process::id() {
        return Err("Refusing to terminate system or self process.".into());
    }
    let output = Command::new("/bin/ps")
        .args(["-o", "uid=", "-p", &pid.to_string()])
        .output()
        .map_err(|e| format!("Failed to inspect target PID {pid}: {e}"))?;
    if !output.status.success() {
        return Err(format!("Process {pid} was not found."));
    }
    let uid_str = String::from_utf8_lossy(&output.stdout);
    let uid: u32 = uid_str
        .trim()
        .parse()
        .map_err(|_| format!("Process {pid} has unreadable owner UID."))?;
    let current_uid = unsafe { libc::geteuid() };
    if uid != current_uid {
        return Err(format!(
            "Process {pid} is owned by UID {uid}, not current user ({current_uid})."
        ));
    }
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc != 0 {
        return Err(format!(
            "Failed to send SIGTERM to process {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }
    ports::invalidate_ports_cache();
    super::EXTRAS_CACHE.invalidate();
    Ok(())
}

pub fn cleanup_orphaned_dev_servers() -> Result<u32, String> {
    let ps_raw =
        cmd_stdout("/bin/ps", &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="]).unwrap_or_default();
    let ports = ports::list_listening_ports().unwrap_or_default();
    let orphans = detect_orphaned_dev_servers(&ps_raw, &ports);
    let mut terminated = 0_u32;
    for item in orphans {
        if terminate_user_process(item.pid).is_ok() {
            terminated += 1;
        }
    }
    ports::invalidate_ports_cache();
    Ok(terminated)
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedCodeSign {
    pub team_id: Option<String>,
    pub authority: Option<String>,
    pub notarized: bool,
    pub hardened_runtime: bool,
    pub sandboxed: bool,
}

pub fn parse_codesign_output(raw: &str) -> ParsedCodeSign {
    let mut team_id = None;
    let mut authority = None;
    let mut notarized = false;
    let mut hardened_runtime = false;
    let mut sandboxed = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("TeamIdentifier=") {
            if val != "not set" && !val.is_empty() {
                team_id = Some(val.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("Authority=") {
            if authority.is_none() {
                authority = Some(val.to_string());
            }
            if val.contains("Developer ID")
                || val.contains("Apple Mac OS")
                || val.contains("Apple Root CA")
            {
                notarized = true;
            }
        } else if trimmed.starts_with("CodeDirectory ") && trimmed.contains("runtime") {
            hardened_runtime = true;
        } else if trimmed.contains("com.apple.security.app-sandbox") {
            sandboxed = true;
        }
    }

    ParsedCodeSign {
        team_id,
        authority,
        notarized,
        hardened_runtime,
        sandboxed,
    }
}

pub fn inspect_process_details(pid: u32) -> Result<ProcessInspection, String> {
    if pid == 0 {
        return Err("Invalid PID 0".into());
    }
    let ps_out = cmd_stdout(
        "/bin/ps",
        &[
            "-p",
            &pid.to_string(),
            "-o",
            "ppid=,user=,etime=,time=,flags=,comm=",
        ],
    )
    .ok_or_else(|| format!("Process {pid} is no longer running."))?;

    let line = ps_out
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| format!("Process {pid} was not found."))?
        .trim();

    let Some((ppid_s, r1)) = split_token(line) else {
        return Err("Failed to parse process info".into());
    };
    let Some((user_s, r2)) = split_token(r1) else {
        return Err("Failed to parse process info".into());
    };
    let Some((etime_s, r3)) = split_token(r2) else {
        return Err("Failed to parse process info".into());
    };
    let Some((time_s, r4)) = split_token(r3) else {
        return Err("Failed to parse process info".into());
    };
    let Some((flags_s, comm_s)) = split_token(r4) else {
        return Err("Failed to parse process info".into());
    };

    let parent_pid = ppid_s.parse::<u32>().unwrap_or(0);
    let flags = u32::from_str_radix(flags_s.trim(), 16).unwrap_or(0);
    let translated = (flags & 0x0002_0000) != 0;
    let arch = if translated {
        "x86_64 (Rosetta)".to_string()
    } else if cfg!(target_arch = "aarch64") {
        "arm64 (Apple Silicon)".to_string()
    } else {
        "x86_64".to_string()
    };

    let executable_path = comm_s.trim().to_string();
    let name = canonical_process_name(&executable_path);
    let parent_name = if parent_pid > 0 {
        cmd_stdout("/bin/ps", &["-p", &parent_pid.to_string(), "-o", "comm="])
            .map(|s| canonical_process_name(s.trim()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("PID {parent_pid}"))
    } else {
        "kernel_task".into()
    };

    let sign_target = if let Some(pos) = executable_path.find(".app/Contents/") {
        &executable_path[..pos + 4]
    } else {
        executable_path.as_str()
    };
    let codesign_raw = Command::new("/usr/bin/codesign")
        .args(["-dvv", "--entitlements", "-", sign_target])
        .output()
        .map(|o| {
            format!(
                "{}\n{}",
                String::from_utf8_lossy(&o.stderr),
                String::from_utf8_lossy(&o.stdout)
            )
        })
        .unwrap_or_default();
    let parsed_sign = parse_codesign_output(&codesign_raw);

    let cwd = cmd_stdout(
        "/usr/sbin/lsof",
        &["-a", "-d", "cwd", "-Fn", "-p", &pid.to_string()],
    )
    .and_then(|raw| ports::parse_lsof_cwd_output(&raw).get(&pid).cloned());

    let lsof_raw =
        cmd_stdout("/usr/sbin/lsof", &["-n", "-P", "-p", &pid.to_string()]).unwrap_or_default();
    let mut open_files = Vec::new();
    let mut seen_files = HashSet::new();
    for lline in lsof_raw.lines().skip(1) {
        let cols: Vec<&str> = lline.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let kind = cols[4];
        let name_col = cols[8..].join(" ");
        let interesting = matches!(kind, "IPv4" | "IPv6" | "unix")
            || (kind == "REG"
                && !name_col.starts_with("/System/")
                && !name_col.starts_with("/usr/lib")
                && !name_col.ends_with(".dylib")
                && !name_col.ends_with(".ttf")
                && !name_col.ends_with(".car"));
        if interesting && seen_files.insert(name_col.clone()) {
            open_files.push(format!("{kind} · {name_col}"));
            if open_files.len() >= 8 {
                break;
            }
        }
    }

    Ok(ProcessInspection {
        pid,
        name,
        executable_path,
        cwd,
        arch,
        parent_pid,
        parent_name,
        user: user_s.to_string(),
        cpu_time: time_s.to_string(),
        elapsed_time: etime_s.to_string(),
        team_id: parsed_sign.team_id,
        authority: parsed_sign.authority,
        notarized: parsed_sign.notarized,
        hardened_runtime: parsed_sign.hardened_runtime,
        sandboxed: parsed_sign.sandboxed,
        open_files,
    })
}

pub fn parse_ollama_ps(raw: &str) -> Vec<OllamaModelItem> {
    let mut items = Vec::new();
    for line in raw.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let name = cols[0].to_string();
        let id = cols[1].to_string();
        let size_val = cols[2].parse::<f64>().unwrap_or(0.0);
        let unit = cols[3];
        let size_gb = if unit.eq_ignore_ascii_case("MB") {
            round1(size_val / 1024.0)
        } else {
            round1(size_val)
        };
        let processor = cols
            .get(4..6)
            .map(|s| s.join(" "))
            .unwrap_or_else(|| "GPU".into());
        let until = if cols.len() > 6 {
            cols[6..].join(" ")
        } else {
            "Active".into()
        };
        items.push(OllamaModelItem {
            name,
            id,
            size_gb,
            processor,
            until,
            runtime: "Ollama".into(),
        });
    }
    items
}

pub fn parse_openai_models_json(raw: &str) -> Vec<String> {
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(raw);
    let Ok(val) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    val["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(str::trim).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .take(4)
                .collect()
        })
        .unwrap_or_default()
}

fn probe_openai_runtime_models(port: u16, runtime: &str) -> Vec<OllamaModelItem> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(90)) else {
        return Vec::new();
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(90)));
    if stream
        .write_all(b"GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return Vec::new();
    }
    let mut buf = [0_u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    if n == 0 {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf[..n]);
    parse_openai_models_json(&text)
        .into_iter()
        .map(|model_id| OllamaModelItem {
            name: model_id.clone(),
            id: format!("{runtime}-{port}-{model_id}"),
            size_gb: 0.0,
            processor: "Unified Mem".into(),
            until: format!(":{port}"),
            runtime: runtime.into(),
        })
        .collect()
}

fn find_ollama_bin() -> Option<&'static str> {
    [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "ollama",
    ]
    .into_iter()
    .find(|p| !p.starts_with('/') || Path::new(p).exists())
}

fn collect_ollama_models_uncached() -> Vec<OllamaModelItem> {
    let mut models = Vec::new();
    let addr = "127.0.0.1:11434".parse().unwrap();
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(35)).is_ok() {
        if let Some(items) = find_ollama_bin()
            .and_then(|bin| cmd_stdout(bin, &["ps"]))
            .map(|raw| parse_ollama_ps(&raw))
        {
            models.extend(items);
        }
    }
    let open_ports = ports::cached_listening_ports_snapshot();
    for p in open_ports {
        let lower_proc = p.process_name.to_ascii_lowercase();
        let runtime = if p.port == 1234 || lower_proc.contains("lmstudio") || lower_proc == "lms" {
            Some("LM Studio")
        } else if lower_proc.contains("llama-server") || lower_proc.contains("mlx") {
            Some("llama.cpp / MLX")
        } else if p.port == 8000 && lower_proc.contains("python") {
            Some("vLLM / MLX")
        } else {
            None
        };
        if let Some(rt) = runtime {
            models.extend(probe_openai_runtime_models(p.port, rt));
        }
    }
    models
}

pub(super) fn collect_ollama_models() -> Vec<OllamaModelItem> {
    OLLAMA_CACHE.get_or_refresh(Duration::from_secs(6), collect_ollama_models_uncached)
}

pub fn unload_ollama_model(name: &str) -> Result<Vec<OllamaModelItem>, String> {
    if name.trim().is_empty() || name.contains(char::is_whitespace) {
        return Err("Invalid model name".into());
    }
    let bin = find_ollama_bin().ok_or_else(|| "ollama CLI not found".to_string())?;
    let out = Command::new(bin)
        .args(["stop", name.trim()])
        .output()
        .map_err(|e| format!("Failed to stop model {name}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let fresh = collect_ollama_models_uncached();
    OLLAMA_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok(fresh)
}

pub fn parse_docker_ps(raw: &str, runtime: &str) -> Vec<ContainerItem> {
    let mut items = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        items.push(ContainerItem {
            id: cols[0].trim().to_string(),
            name: cols[1].trim().to_string(),
            image: cols[2].trim().to_string(),
            status: cols[3].trim().to_string(),
            ports: cols.get(4).map(|s| s.trim()).unwrap_or("").to_string(),
            runtime: runtime.to_string(),
        });
    }
    items
}

fn docker_socket_active() -> Option<&'static str> {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if home.join(".orbstack/run/docker.sock").exists() {
            return Some("OrbStack");
        }
        if home.join(".docker/run/docker.sock").exists() {
            return Some("Docker");
        }
    }
    if Path::new("/var/run/docker.sock").exists() {
        return Some("Docker");
    }
    None
}

fn find_docker_bin() -> Option<&'static str> {
    ["/opt/homebrew/bin/docker", "/usr/local/bin/docker"]
        .into_iter()
        .find(|p| Path::new(p).exists())
}

fn collect_containers_uncached() -> Vec<ContainerItem> {
    if let (Some(runtime), Some(bin)) = (docker_socket_active(), find_docker_bin()) {
        cmd_stdout(
            bin,
            &[
                "ps",
                "--format",
                "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
            ],
        )
        .map(|raw| parse_docker_ps(&raw, runtime))
        .unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub(super) fn collect_containers() -> Vec<ContainerItem> {
    CONTAINER_CACHE.get_or_refresh(Duration::from_secs(6), collect_containers_uncached)
}

pub fn control_container(id: &str, action: &str) -> Result<Vec<ContainerItem>, String> {
    if !matches!(action, "stop" | "restart") {
        return Err("Container action must be 'stop' or 'restart'".into());
    }
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid container identifier".into());
    }
    let bin = find_docker_bin().ok_or_else(|| "docker CLI not found".to_string())?;
    let out = Command::new(bin)
        .args([action, id])
        .output()
        .map_err(|e| format!("Failed to {action} container {id}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let fresh = collect_containers_uncached();
    CONTAINER_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok(fresh)
}

pub fn run_maintenance_action(action: &str) -> Result<String, String> {
    match action {
        "flush_dns" => {
            let status = Command::new("/usr/bin/dscacheutil")
                .arg("-flushcache")
                .status()
                .map_err(|e| format!("Failed to flush DNS cache: {e}"))?;
            if status.success() {
                Ok("Flushed macOS user DNS directory cache (dscacheutil).".into())
            } else {
                Err("dscacheutil exited with non-zero status.".into())
            }
        }
        "purge_ram" => {
            let out = Command::new("/usr/sbin/purge")
                .output()
                .map_err(|e| format!("Failed to invoke purge: {e}"))?;
            if out.status.success() {
                Ok("Purged inactive disk & memory caches.".into())
            } else {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                Err(if err.is_empty() {
                    "Reclaiming kernel purge memory requires administrator privileges (`sudo purge`).".into()
                } else {
                    err
                })
            }
        }
        _ => Err(format!("Unsupported maintenance action: {action}")),
    }
}

pub(super) fn collect_launch_agents() -> Vec<LaunchAgentItem> {
    LAUNCH_AGENTS_CACHE.get_or_refresh(Duration::from_secs(120), collect_launch_agents_uncached)
}

pub(super) fn parse_launch_agent_plist(plist_path: &Path) -> (String, String) {
    let fallback_label = plist_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let Ok(val) = plist::Value::from_file(plist_path) else {
        return (fallback_label, String::new());
    };
    let Some(dict) = val.as_dictionary() else {
        return (fallback_label, String::new());
    };
    let label = dict
        .get("Label")
        .and_then(|v| v.as_string())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback_label);
    let program = dict
        .get("ProgramArguments")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_string())
        .or_else(|| dict.get("Program").and_then(|v| v.as_string()))
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    (label, program)
}

fn collect_launch_agents_uncached() -> Vec<LaunchAgentItem> {
    let mut dirs = vec![("/Library/LaunchAgents".to_string(), "System")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.insert(
            0,
            (
                home.join("Library/LaunchAgents")
                    .to_string_lossy()
                    .into_owned(),
                "User",
            ),
        );
    }

    let mut items = Vec::new();
    for (dir, scope) in dirs {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten().take(24) {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("plist") {
                continue;
            }
            let plist_path = p.to_string_lossy().into_owned();
            let (label, program) = parse_launch_agent_plist(&p);

            let binary_exists = !program.is_empty() && Path::new(&program).exists();
            let (signed, authority) = if binary_exists {
                let raw = Command::new("/usr/bin/codesign")
                    .args(["-dv", &program])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                    .unwrap_or_default();
                let parsed = parse_codesign_output(&raw);
                (parsed.authority.is_some(), parsed.authority)
            } else {
                (false, None)
            };

            items.push(LaunchAgentItem {
                label,
                plist_path,
                program,
                scope: scope.to_string(),
                signed,
                authority,
                binary_exists,
            });
        }
    }

    items.sort_by(|a, b| {
        a.binary_exists
            .cmp(&b.binary_exists)
            .then(a.signed.cmp(&b.signed))
            .then(a.label.cmp(&b.label))
    });
    items
}

pub fn read_quick_switch_state() -> QuickSwitchState {
    QUICK_SWITCH_CACHE.get_or_refresh(Duration::from_secs(15), read_quick_switch_state_uncached)
}

fn read_quick_switch_state_uncached() -> QuickSwitchState {
    let dark_mode = cmd_stdout("/usr/bin/defaults", &["read", "-g", "AppleInterfaceStyle"])
        .map(|s| s.trim().eq_ignore_ascii_case("Dark"))
        .unwrap_or(false);
    let show_hidden_files = cmd_stdout(
        "/usr/bin/defaults",
        &["read", "com.apple.finder", "AppleShowAllFiles"],
    )
    .map(|s| matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
    .unwrap_or(false);
    let show_desktop_icons = cmd_stdout(
        "/usr/bin/defaults",
        &["read", "com.apple.finder", "CreateDesktop"],
    )
    .map(|s| !matches!(s.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"))
    .unwrap_or(true);
    QuickSwitchState {
        dark_mode,
        show_hidden_files,
        show_desktop_icons,
    }
}

pub fn run_quick_switch(switch_id: &str) -> Result<(QuickSwitchState, String), String> {
    let curr = read_quick_switch_state_uncached();
    let msg = match switch_id {
        "toggle_dark_mode" => {
            let next = if curr.dark_mode { "false" } else { "true" };
            let script = format!("tell application \"System Events\" to tell appearance preferences to set dark mode to {next}");
            let st = Command::new("/usr/bin/osascript")
                .args(["-e", &script])
                .status()
                .map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("Failed to toggle macOS appearance mode.".into());
            }
            format!(
                "Switched appearance to {} Mode.",
                if curr.dark_mode { "Light" } else { "Dark" }
            )
        }
        "toggle_hidden_files" => {
            let next = if curr.show_hidden_files {
                "FALSE"
            } else {
                "TRUE"
            };
            let _ = Command::new("/usr/bin/defaults")
                .args([
                    "write",
                    "com.apple.finder",
                    "AppleShowAllFiles",
                    "-bool",
                    next,
                ])
                .status();
            let _ = Command::new("/usr/bin/killall").arg("Finder").status();
            format!(
                "{} hidden Finder dotfiles.",
                if curr.show_hidden_files {
                    "Hid"
                } else {
                    "Revealed"
                }
            )
        }
        "toggle_desktop_icons" => {
            let next = if curr.show_desktop_icons {
                "FALSE"
            } else {
                "TRUE"
            };
            let _ = Command::new("/usr/bin/defaults")
                .args(["write", "com.apple.finder", "CreateDesktop", "-bool", next])
                .status();
            let _ = Command::new("/usr/bin/killall").arg("Finder").status();
            format!(
                "{} Desktop icons.",
                if curr.show_desktop_icons {
                    "Hid"
                } else {
                    "Restored"
                }
            )
        }
        "clear_clipboard" => {
            let mut child = Command::new("/usr/bin/pbcopy")
                .stdin(std::process::Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            let _ = child.wait();
            "Cleared sensitive clipboard contents.".to_string()
        }
        "empty_trash" => {
            let st = Command::new("/usr/bin/osascript")
                .args(["-e", "tell application \"Finder\" to empty trash"])
                .status()
                .map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("Failed to empty Trash.".into());
            }
            "Emptied macOS Trash.".to_string()
        }
        _ => return Err(format!("Unknown quick switch: {switch_id}")),
    };
    let fresh = read_quick_switch_state_uncached();
    QUICK_SWITCH_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok((fresh, msg))
}

pub fn quick_switch_label(switch_id: &str) -> &'static str {
    match switch_id {
        "toggle_dark_mode" => "Dark Mode",
        "toggle_hidden_files" => "Hidden Files",
        "toggle_desktop_icons" => "Desktop Icons",
        "clear_clipboard" => "Clipboard Cleared",
        "empty_trash" => "Trash Emptied",
        _ => "Quick Switch",
    }
}

pub fn reveal_in_finder(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    if !target.exists() {
        return Err(format!("Path does not exist: {path}"));
    }
    let mut cmd = Command::new("/usr/bin/open");
    if target.is_file() {
        cmd.arg("-R").arg(path);
    } else {
        cmd.arg(path);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to reveal in Finder: {e}"))?;
    if !status.success() {
        return Err("Finder failed to open path.".into());
    }
    Ok(())
}

pub fn parse_iso8601_utc_ms(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if trimmed.len() < 19 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' || (bytes[10] != b'T' && bytes[10] != b' ') {
        return None;
    }
    let year: i64 = trimmed[0..4].parse().ok()?;
    let month: u32 = trimmed[5..7].parse().ok()?;
    let day: u32 = trimmed[8..10].parse().ok()?;
    let hour: u64 = trimmed[11..13].parse().ok()?;
    let min: u64 = trimmed[14..16].parse().ok()?;
    let sec: u64 = trimmed[17..19].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    if days < 0 {
        return None;
    }
    let total_secs = (days as u64) * 86_400 + hour * 3_600 + min * 60 + sec;
    Some(total_secs * 1_000)
}

pub type JsonlLineUsage = (Option<u64>, u64, u64, u64, Option<String>);

pub fn parse_agent_jsonl_usage_line(line: &str) -> Option<JsonlLineUsage> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let ts_ms = val["timestamp"]
        .as_str()
        .and_then(parse_iso8601_utc_ms)
        .or_else(|| val["timestamp"].as_u64());
    let msg = val.get("message").filter(|m| m.is_object());
    let usage = msg
        .and_then(|m| m.get("usage"))
        .or_else(|| val.get("usage"))
        .or_else(|| val.get("payload").and_then(|p| p.get("info")).and_then(|i| i.get("last_token_usage")))?;
    if !usage.is_object() {
        return None;
    }
    let inp = usage["input_tokens"]
        .as_u64()
        .or_else(|| usage["prompt_tokens"].as_u64())
        .unwrap_or(0);
    let out = usage["output_tokens"]
        .as_u64()
        .or_else(|| usage["completion_tokens"].as_u64())
        .unwrap_or(0);
    let cache_read = usage["cache_read_input_tokens"]
        .as_u64()
        .or_else(|| usage["cached_tokens"].as_u64())
        .unwrap_or(0);
    if inp == 0 && out == 0 && cache_read == 0 {
        return None;
    }
    let model = msg
        .and_then(|m| m["model"].as_str())
        .or_else(|| val["model"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((ts_ms, inp, out, cache_read, model))
}

fn collect_recent_jsonl_files(root: &Path, depth: u8, cutoff_ms: u64, out: &mut Vec<(u64, PathBuf)>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_recent_jsonl_files(&p, depth - 1, cutoff_ms, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let mtime_ms = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if mtime_ms >= cutoff_ms {
                        out.push((mtime_ms, p));
                    }
                }
            }
        }
    }
}

fn collect_ai_token_usage_uncached() -> AiTokenUsageSummary {
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return AiTokenUsageSummary::default();
    };
    let now_ms = crate::clock::now_unix_ms();
    let cutoff_24h = now_ms.saturating_sub(24 * 3_600 * 1_000);
    let cutoff_5h = now_ms.saturating_sub(5 * 3_600 * 1_000);

    let mut files = Vec::new();
    collect_recent_jsonl_files(&home.join(".claude/projects"), 3, cutoff_24h, &mut files);
    collect_recent_jsonl_files(&home.join(".codex/sessions"), 5, cutoff_24h, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.truncate(40);

    let mut summary = AiTokenUsageSummary {
        active_sessions_24h: files.len() as u32,
        ..Default::default()
    };

    for (mtime_ms, path) in files {
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len > 8 * 1024 * 1024 {
            let _ = file.seek(SeekFrom::End(-(8 * 1024 * 1024)));
        }
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if !line.contains("usage") {
                continue;
            }
            let Some((line_ts, inp, out, cache_read, model)) = parse_agent_jsonl_usage_line(&line)
            else {
                continue;
            };
            let ts = line_ts.unwrap_or(mtime_ms);
            if ts < cutoff_24h {
                continue;
            }
            let total = inp.saturating_add(out).saturating_add(cache_read);
            summary.window_24h_tokens = summary.window_24h_tokens.saturating_add(total);
            summary.input_tokens_24h = summary.input_tokens_24h.saturating_add(inp);
            summary.output_tokens_24h = summary.output_tokens_24h.saturating_add(out);
            summary.cache_read_tokens_24h =
                summary.cache_read_tokens_24h.saturating_add(cache_read);
            if ts >= cutoff_5h {
                summary.window_5h_tokens = summary.window_5h_tokens.saturating_add(total);
            }
            if summary.latest_model.is_none() && model.is_some() {
                summary.latest_model = model;
            }
        }
    }

    summary.estimated_cost_usd_24h = round2(
        (summary.input_tokens_24h as f64 * 3.0
            + summary.output_tokens_24h as f64 * 15.0
            + summary.cache_read_tokens_24h as f64 * 0.30)
            / 1_000_000.0,
    );
    summary
}

pub(super) fn collect_ai_token_usage() -> AiTokenUsageSummary {
    AI_TOKENS_CACHE.get_or_refresh(Duration::from_secs(20), collect_ai_token_usage_uncached)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::ListeningPort;

    #[test]
    fn detects_ai_agents_and_orphaned_servers() {
        let ps = "\
  500     1   14:22   4.5  204800 /usr/local/bin/claude
  501   500   10:00   1.2   65536 /usr/local/bin/node
";
        let ports = vec![ListeningPort {
            port: 3000,
            pid: 501,
            process_name: "node".into(),
            bind_address: "127.0.0.1".into(),
            is_exposed: false,
            project_path: None,
            launcher: None,
        }];
        let agents = detect_ai_agents(ps, &ports);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_name, "Claude Code");
        assert_eq!(agents[0].process_count, 2);
        assert_eq!(agents[0].ports, vec![3000]);
        assert_eq!(agents[0].children, vec!["node"]);

        let ps_orphans = "\
  9001     1   12:10   1.4   92160 /usr/local/bin/vite
  9002   500   04:00   0.5   40960 /usr/local/bin/node
  9003     1   08:00   2.0  150000 /Applications/Cursor.app/Contents/MacOS/Cursor
";
        let orphans = detect_orphaned_dev_servers(
            ps_orphans,
            &[ListeningPort {
                port: 5173,
                pid: 9001,
                process_name: "vite".into(),
                bind_address: "127.0.0.1".into(),
                is_exposed: false,
                project_path: None,
                launcher: None,
            }],
        );
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].pid, 9001);
        assert_eq!(orphans[0].port, Some(5173));
    }

    #[test]
    fn parses_codesign_ollama_and_docker() {
        let sign_raw = "\
Executable=/Applications/Example.app/Contents/MacOS/Example
CodeDirectory v=20500 size=1000 flags=0x10000(runtime) hashes=10+3 location=embedded
Authority=Developer ID Application: Example Inc (ABC123XYZ)
TeamIdentifier=ABC123XYZ
com.apple.security.app-sandbox
";
        let parsed = parse_codesign_output(sign_raw);
        assert_eq!(parsed.team_id.as_deref(), Some("ABC123XYZ"));
        assert!(parsed.notarized);
        assert!(parsed.hardened_runtime);
        assert!(parsed.sandboxed);

        let ollama_raw = "\
NAME                    ID              SIZE      PROCESSOR          CONTEXT    UNTIL
qwen2.5-coder:14b       abc123def       9.0 GB    100% GPU           8192       4 minutes from now
";
        let models = parse_ollama_ps(ollama_raw);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "qwen2.5-coder:14b");
        assert_eq!(models[0].size_gb, 9.0);
        assert_eq!(models[0].processor, "100% GPU");

        let docker_raw = "1a2b3c\tpostgres-dev\tpostgres:16\tUp 2 hours\t0.0.0.0:5432->5432/tcp\n";
        let ctrs = parse_docker_ps(docker_raw, "OrbStack");
        assert_eq!(ctrs.len(), 1);
        assert_eq!(ctrs[0].name, "postgres-dev");
        assert_eq!(ctrs[0].runtime, "OrbStack");
    }

    #[test]
    fn parses_launch_agent_plist_natively() {
        let dir = tempfile::tempdir().unwrap();
        let plist_path = dir.path().join("com.example.agent.plist");
        let mut dict = plist::Dictionary::new();
        dict.insert("Label".into(), plist::Value::String("com.example.demo".into()));
        dict.insert(
            "ProgramArguments".into(),
            plist::Value::Array(vec![
                plist::Value::String("/usr/local/bin/demo-agent".into()),
                plist::Value::String("--serve".into()),
            ]),
        );
        plist::Value::Dictionary(dict)
            .to_file_xml(&plist_path)
            .unwrap();
        let (label, program) = parse_launch_agent_plist(&plist_path);
        assert_eq!(label, "com.example.demo");
        assert_eq!(program, "/usr/local/bin/demo-agent");
    }

    #[test]
    fn parses_agent_jsonl_usage_and_openai_models() {
        let ts = parse_iso8601_utc_ms("2026-11-24T10:15:30.123Z").unwrap();
        assert!(ts > 1_700_000_000_000);

        let line = r#"{"timestamp":"2026-11-24T10:15:30.123Z","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":1200,"output_tokens":340,"cache_read_input_tokens":8000}}}"#;
        let parsed = parse_agent_jsonl_usage_line(line).unwrap();
        assert_eq!(parsed.0, Some(ts));
        assert_eq!(parsed.1, 1200);
        assert_eq!(parsed.2, 340);
        assert_eq!(parsed.3, 8000);
        assert_eq!(parsed.4.as_deref(), Some("claude-sonnet-4-5"));

        let http_raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"data\":[{\"id\":\"qwen2.5-coder-32b\"}]}";
        assert_eq!(
            parse_openai_models_json(http_raw),
            vec!["qwen2.5-coder-32b"]
        );
    }
}
