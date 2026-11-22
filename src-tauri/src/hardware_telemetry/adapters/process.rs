use crate::hardware_telemetry::contract::{Availability, ProcessReading};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(3);
const MAX_TOP_PROCESSES: usize = 8;

static PROCESS_CACHE: OnceLock<Mutex<Option<(Instant, Availability<Vec<ProcessReading>>)>>> =
    OnceLock::new();

pub struct ProcessAdapter;

impl ProcessAdapter {
    pub fn top_processes() -> Availability<Vec<ProcessReading>> {
        let cache = PROCESS_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(guard) = cache.lock() {
            if let Some((captured_at, cached)) = guard.as_ref() {
                if captured_at.elapsed() < CACHE_TTL {
                    return cached.clone();
                }
            }
        }

        let fresh = read_ps_top_processes();
        if let Ok(mut guard) = cache.lock() {
            *guard = Some((Instant::now(), fresh.clone()));
        }
        fresh
    }
}

fn read_ps_top_processes() -> Availability<Vec<ProcessReading>> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,pcpu=,rss=,comm="])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            Availability::Available {
                value: aggregate_ps_output(&stdout, MAX_TOP_PROCESSES),
            }
        }
        Ok(out) => Availability::Unavailable {
            reason: format!("ps exited with status {}", out.status),
        },
        Err(error) => Availability::Unavailable {
            reason: format!("Failed to inspect processes: {error}"),
        },
    }
}

fn parse_ps_line(line: &str) -> Option<(u32, f64, u64, &str)> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    let idx1 = s.find(char::is_whitespace)?;
    let tok1 = &s[..idx1];
    let rest1 = s[idx1..].trim_start();

    if !tok1.contains('.') {
        if let Ok(pid) = tok1.parse::<u32>() {
            let idx2 = rest1.find(char::is_whitespace)?;
            let pcpu: f64 = rest1[..idx2].parse().ok()?;
            if !pcpu.is_finite() {
                return None;
            }
            let rest2 = rest1[idx2..].trim_start();
            let idx3 = rest2.find(char::is_whitespace)?;
            let rss_kb: u64 = rest2[..idx3].parse().ok()?;
            let comm = rest2[idx3..].trim();
            if comm.is_empty() {
                return None;
            }
            return Some((pid, pcpu, rss_kb, comm));
        }
    }

    let pcpu: f64 = tok1.parse().ok()?;
    if !pcpu.is_finite() {
        return None;
    }
    let idx2 = rest1.find(char::is_whitespace)?;
    let rss_kb: u64 = rest1[..idx2].parse().ok()?;
    let comm = rest1[idx2..].trim();
    if comm.is_empty() {
        return None;
    }
    Some((0, pcpu, rss_kb, comm))
}

/// Collapses helper executables inside `.app` bundles to their owning application name.
pub fn canonical_process_name(comm: &str) -> String {
    let trimmed = comm.trim();
    if let Some(app_idx) = trimmed.find(".app/") {
        let prefix = &trimmed[..app_idx];
        let app_name = prefix.rsplit('/').next().unwrap_or(prefix).trim();
        if !app_name.is_empty() {
            return app_name.to_string();
        }
    }

    let base = trimmed.rsplit('/').next().unwrap_or(trimmed).trim();
    let stripped = base.strip_prefix('-').unwrap_or(base).trim();
    stripped.to_string()
}

pub fn aggregate_ps_output(raw: &str, max_items: usize) -> Vec<ProcessReading> {
    // (cpu_sum, rss_kb_sum, process_count, best_cpu_for_pid, primary_pid)
    let mut grouped: BTreeMap<String, (f64, u64, u32, f64, u32)> = BTreeMap::new();

    for line in raw.lines() {
        let Some((pid, pcpu, rss_kb, comm)) = parse_ps_line(line) else {
            continue;
        };
        let name = canonical_process_name(comm);
        if name.is_empty() || name == "ps" {
            continue;
        }
        let entry = grouped.entry(name).or_insert((0.0, 0, 0, -1.0, 0));
        entry.0 += pcpu;
        entry.1 = entry.1.saturating_add(rss_kb);
        entry.2 = entry.2.saturating_add(1);
        if pid > 0 && pcpu >= entry.3 {
            entry.3 = pcpu;
            entry.4 = pid;
        }
    }

    let mut readings: Vec<ProcessReading> = grouped
        .into_iter()
        .filter_map(|(name, (cpu_sum, rss_kb_sum, process_count, _, primary_pid))| {
            let cpu_percent = (cpu_sum * 10.0).round() / 10.0;
            if cpu_percent < 0.1 {
                return None;
            }
            let memory_mb = ((rss_kb_sum as f64 / 1024.0) * 10.0).round() / 10.0;
            Some(ProcessReading {
                name,
                cpu_percent,
                memory_mb,
                process_count,
                primary_pid,
            })
        })
        .collect();

    readings.sort_by(|left, right| {
        right
            .cpu_percent
            .total_cmp(&left.cpu_percent)
            .then_with(|| right.memory_mb.total_cmp(&left.memory_mb))
            .then_with(|| left.name.cmp(&right.name))
    });
    readings.truncate(max_items);
    readings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_name_rolls_up_app_bundles_and_strips_paths() {
        assert_eq!(
            canonical_process_name(
                "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper (Renderer)"
            ),
            "Google Chrome"
        );
        assert_eq!(
            canonical_process_name(
                "/Applications/Cursor.app/Contents/MacOS/Cursor"
            ),
            "Cursor"
        );
        assert_eq!(canonical_process_name("/usr/local/bin/node"), "node");
        assert_eq!(canonical_process_name("-zsh"), "zsh");
    }

    #[test]
    fn aggregates_helper_processes_and_sorts_top_culprits() {
        let sample = "\
 1001  14.2  524288 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
 1002  18.5  262144 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)
 2001  28.44 1048576 /Applications/Cursor.app/Contents/MacOS/Cursor
 3001   0.0   10240 /bin/ps
 3002   0.04  51200 /usr/sbin/syslogd
 3003   6.1  131072 /opt/homebrew/bin/rustc
";
        let top = aggregate_ps_output(sample, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(
            top[0],
            ProcessReading {
                name: "Google Chrome".into(),
                cpu_percent: 32.7,
                memory_mb: 768.0,
                process_count: 2,
                primary_pid: 1002,
            }
        );
        assert_eq!(
            top[1],
            ProcessReading {
                name: "Cursor".into(),
                cpu_percent: 28.4,
                memory_mb: 1024.0,
                process_count: 1,
                primary_pid: 2001,
            }
        );
    }
}
