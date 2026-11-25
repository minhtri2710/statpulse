use crate::proc_info::{classify_ai_agent, is_dev_server_binary};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

const PORTS_CACHE_TTL: Duration = Duration::from_millis(2500);

#[derive(Clone)]
struct PidMetadata {
    process_name: String,
    cwd: Option<String>,
    launcher: Option<String>,
}

type PortsCache = OnceLock<Mutex<Option<(Instant, Vec<ListeningPort>)>>>;

static LISTENING_PORTS_CACHE: PortsCache = OnceLock::new();
static FAST_PORTS_CACHE: PortsCache = OnceLock::new();
static PID_METADATA_CACHE: OnceLock<Mutex<HashMap<u32, PidMetadata>>> = OnceLock::new();

pub fn invalidate_ports_cache() {
    if let Some(cache) = LISTENING_PORTS_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            *guard = None;
        }
    }
    if let Some(cache) = FAST_PORTS_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            *guard = None;
        }
    }
    if let Some(cache) = PID_METADATA_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.clear();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ListeningPort {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub bind_address: String,
    pub is_exposed: bool,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub launcher: Option<String>,
}

pub fn list_listening_ports_fast() -> Result<Vec<ListeningPort>, String> {
    let cache = LISTENING_PORTS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((captured_at, cached)) = guard.as_ref() {
            if captured_at.elapsed() < PORTS_CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }
    let fast_cache = FAST_PORTS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = fast_cache.lock() {
        if let Some((captured_at, cached)) = guard.as_ref() {
            if captured_at.elapsed() < PORTS_CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }

    let output = Command::new("/usr/sbin/lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P"])
        .output()
        .map_err(|e| format!("Failed to run lsof: {e}"))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut list = parse_lsof_listening_ports(&stdout);
    if let Some(pid_cache) = PID_METADATA_CACHE.get() {
        if let Ok(guard) = pid_cache.lock() {
            for port in list.iter_mut() {
                if let Some(meta) = guard.get(&port.pid) {
                    if meta.process_name == port.process_name {
                        port.project_path = meta.cwd.clone();
                        port.launcher = meta.launcher.clone();
                    }
                }
            }
        }
    }
    if let Ok(mut guard) = fast_cache.lock() {
        *guard = Some((Instant::now(), list.clone()));
    }
    Ok(list)
}

pub fn cached_listening_ports_snapshot() -> Vec<ListeningPort> {
    if let Some(cache) = LISTENING_PORTS_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            if let Some((_, cached)) = guard.as_ref() {
                return cached.clone();
            }
        }
    }
    if let Some(cache) = FAST_PORTS_CACHE.get() {
        if let Ok(guard) = cache.lock() {
            if let Some((_, cached)) = guard.as_ref() {
                return cached.clone();
            }
        }
    }
    Vec::new()
}

pub fn list_listening_ports() -> Result<Vec<ListeningPort>, String> {
    let cache = LISTENING_PORTS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((captured_at, cached)) = guard.as_ref() {
            if captured_at.elapsed() < PORTS_CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }

    let output = Command::new("/usr/sbin/lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-n", "-P"])
        .output()
        .map_err(|e| format!("Failed to run lsof: {e}"))?;

    if !output.status.success() && output.stdout.is_empty() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut list = parse_lsof_listening_ports(&stdout);
    enrich_listening_ports(&mut list);

    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), list.clone()));
    }
    Ok(list)
}

pub fn enrich_listening_ports(list: &mut [ListeningPort]) {
    if list.is_empty() {
        return;
    }

    let active_pids: HashSet<u32> = list.iter().map(|p| p.pid).collect();
    let pid_cache = PID_METADATA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut missing_pids: Vec<u32> = Vec::new();
    {
        if let Ok(mut guard) = pid_cache.lock() {
            guard.retain(|pid, _| active_pids.contains(pid));
            for port in list.iter() {
                let needs_fetch = match guard.get(&port.pid) {
                    Some(meta) => meta.process_name != port.process_name,
                    None => true,
                };
                if needs_fetch {
                    missing_pids.push(port.pid);
                }
            }
        } else {
            missing_pids = active_pids.iter().copied().collect();
        }
    }
    missing_pids.sort_unstable();
    missing_pids.dedup();

    if !missing_pids.is_empty() {
        let pid_arg = missing_pids
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let cwd_map = Command::new("/usr/sbin/lsof")
            .args(["-a", "-d", "cwd", "-Fn", "-p", &pid_arg])
            .output()
            .ok()
            .map(|o| parse_lsof_cwd_output(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default();

        let ppid_map = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,comm="])
            .output()
            .ok()
            .map(|o| parse_ps_parent_map(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default();

        if let Ok(mut guard) = pid_cache.lock() {
            for &pid in &missing_pids {
                let proc_name = list
                    .iter()
                    .find(|p| p.pid == pid)
                    .map(|p| p.process_name.clone())
                    .unwrap_or_default();
                let cwd = cwd_map.get(&pid).cloned();
                let launcher = resolve_port_launcher(pid, &proc_name, &ppid_map);
                guard.insert(
                    pid,
                    PidMetadata {
                        process_name: proc_name,
                        cwd,
                        launcher,
                    },
                );
            }
        }
    }

    if let Ok(guard) = pid_cache.lock() {
        for port in list.iter_mut() {
            if let Some(meta) = guard.get(&port.pid) {
                port.project_path = meta.cwd.clone();
                port.launcher = meta.launcher.clone();
            }
        }
    }
}

pub fn parse_lsof_cwd_output(raw: &str) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let mut current_pid: Option<u32> = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(pid_str) = trimmed.strip_prefix('p') {
            current_pid = pid_str.parse::<u32>().ok();
        } else if let Some(path) = trimmed.strip_prefix('n') {
            if let Some(pid) = current_pid {
                if !path.is_empty() && path != "/" {
                    map.insert(pid, path.to_string());
                }
            }
        }
    }
    map
}

pub fn parse_ps_parent_map(raw: &str) -> HashMap<u32, (u32, String)> {
    let mut map = HashMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(3, char::is_whitespace);
        let Some(pid_s) = parts.next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let rest = trimmed[pid_s.len()..].trim_start();
        let Some(space_idx) = rest.find(char::is_whitespace) else {
            continue;
        };
        let ppid_s = &rest[..space_idx];
        let comm = rest[space_idx..].trim();
        if let (Ok(pid), Ok(ppid)) = (pid_s.parse::<u32>(), ppid_s.parse::<u32>()) {
            map.insert(pid, (ppid, comm.to_string()));
        }
    }
    map
}

pub fn resolve_port_launcher(
    pid: u32,
    process_name: &str,
    ppid_map: &HashMap<u32, (u32, String)>,
) -> Option<String> {
    let mut direct_ppid = None;
    let mut curr = pid;
    for hop in 0..10 {
        let Some((ppid, comm)) = ppid_map.get(&curr) else {
            break;
        };
        if hop == 0 {
            direct_ppid = Some(*ppid);
        } else {
            if let Some(agent) = classify_ai_agent(comm) {
                if agent != "Ollama" {
                    return Some(agent.into());
                }
            }
            let lower = comm.to_ascii_lowercase();
            if lower.contains("visual studio code") || lower.contains("/code.app/") {
                return Some("VS Code".into());
            }
            if lower.contains("iterm2")
                || lower.contains("terminal.app")
                || lower.contains("warp")
                || lower.contains("ghostty")
                || lower.contains("alacritty")
                || lower.contains("kitty")
            {
                return Some("Terminal".into());
            }
            if lower.contains("docker") || lower.contains("orbstack") || lower.contains("vpnkit") {
                return Some("Docker".into());
            }
        }
        if *ppid <= 1 || *ppid == curr {
            break;
        }
        curr = *ppid;
    }

    if direct_ppid == Some(1) {
        let pname = process_name.to_ascii_lowercase();
        if is_dev_server_binary(&pname) {
            return Some("Orphaned (launchd)".into());
        }
    }
    None
}

pub fn parse_lsof_listening_ports(raw: &str) -> Vec<ListeningPort> {
    let mut ports: BTreeMap<(u16, u32), ListeningPort> = BTreeMap::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("COMMAND") {
            continue;
        }
        let Some(before_listen) = trimmed.strip_suffix("(LISTEN)") else {
            continue;
        };
        let parts: Vec<&str> = before_listen.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let raw_cmd = parts[0].replace("\\x20", " ");
        let Ok(pid) = parts[1].parse::<u32>() else {
            continue;
        };
        let endpoint = parts[parts.len() - 1];
        let Some((bind_address, port_str)) = endpoint.rsplit_once(':') else {
            continue;
        };
        let Ok(port) = port_str.parse::<u16>() else {
            continue;
        };

        let is_exposed = bind_address == "*" || bind_address == "0.0.0.0" || bind_address == "[::]";
        let key = (port, pid);
        match ports.get_mut(&key) {
            Some(existing) => {
                if is_exposed && !existing.is_exposed {
                    existing.is_exposed = true;
                    existing.bind_address = bind_address.to_string();
                }
            }
            None => {
                ports.insert(
                    key,
                    ListeningPort {
                        port,
                        pid,
                        process_name: raw_cmd,
                        bind_address: bind_address.to_string(),
                        is_exposed,
                        project_path: None,
                        launcher: None,
                    },
                );
            }
        }
    }

    let mut list: Vec<ListeningPort> = ports.into_values().collect();
    list.sort_by_key(|p| (!(1024..=49151).contains(&p.port), p.port, p.pid));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_deduplicates_listening_ports_flagging_exposed_binds() {
        let sample = "\
COMMAND     PID    USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
ControlCe  1129 beowulf    9u  IPv4 0x111      0t0  TCP *:7000 (LISTEN)
ControlCe  1129 beowulf   10u  IPv6 0x222      0t0  TCP *:7000 (LISTEN)
node      46409 beowulf  142u  IPv4 0x333      0t0  TCP 127.0.0.1:3000 (LISTEN)
vite      48211 beowulf   18u  IPv6 0x444      0t0  TCP [::]:5173 (LISTEN)
Google\\x20Chrome 500 beowulf 12u IPv4 0x555  0t0  TCP 127.0.0.1:59000 (LISTEN)
";
        let parsed = parse_lsof_listening_ports(sample);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].port, 3000);
        assert!(!parsed[0].is_exposed);
        assert_eq!(parsed[0].bind_address, "127.0.0.1");

        assert_eq!(parsed[1].port, 5173);
        assert!(parsed[1].is_exposed);

        assert_eq!(parsed[2].port, 7000);
        assert!(parsed[2].is_exposed);

        assert_eq!(parsed[3].port, 59000);
        assert_eq!(parsed[3].process_name, "Google Chrome");
    }

    #[test]
    fn parses_lsof_cwd_and_classifies_parent_launcher() {
        let cwd_raw = "\
p46409
fcwd
n/Users/dev/Work/example-project
p1
fcwd
n/
";
        let cwds = parse_lsof_cwd_output(cwd_raw);
        assert_eq!(
            cwds.get(&46409).map(String::as_str),
            Some("/Users/dev/Work/example-project")
        );
        assert!(!cwds.contains_key(&1));

        let ps_raw = "\
  46409   46000 /usr/local/bin/node
  46000   45000 /bin/zsh
  45000       1 /Applications/Cursor.app/Contents/MacOS/Cursor
  99999       1 /usr/local/bin/node
";
        let ppid_map = parse_ps_parent_map(ps_raw);
        assert_eq!(
            resolve_port_launcher(46409, "node", &ppid_map).as_deref(),
            Some("Cursor")
        );
        assert_eq!(
            resolve_port_launcher(99999, "node", &ppid_map).as_deref(),
            Some("Orphaned (launchd)")
        );
    }
}
