use crate::hardware_telemetry::contract::{Availability, ProcessReading};
use crate::proc_info::{canonical_process_name, round1};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(3);
const MAX_TOP_PROCESSES: usize = 8;

type ProcessCacheEntry = Option<(Instant, Availability<Vec<ProcessReading>>)>;

static PROCESS_CACHE: OnceLock<Mutex<ProcessCacheEntry>> = OnceLock::new();

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

    pub fn cached_top_processes() -> Availability<Vec<ProcessReading>> {
        if let Some(cache) = PROCESS_CACHE.get() {
            if let Ok(guard) = cache.lock() {
                if let Some((captured_at, cached)) = guard.as_ref() {
                    if captured_at.elapsed() < CACHE_TTL {
                        return cached.clone();
                    }
                }
            }
        }
        Availability::NotPresent
    }
}

fn read_ps_top_processes() -> Availability<Vec<ProcessReading>> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,state=,pcpu=,rss=,comm="])
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

fn parse_ps_line(line: &str) -> Option<(u32, bool, f64, u64, &str)> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    let idx1 = s.find(char::is_whitespace)?;
    let pid: u32 = s[..idx1].parse().ok()?;
    let rest1 = s[idx1..].trim_start();
    let idx2 = rest1.find(char::is_whitespace)?;
    let tok2 = &rest1[..idx2];
    let (is_paused, rest_after_state) = if let Ok(_cpu) = tok2.parse::<f64>() {
        (false, rest1)
    } else {
        let paused = tok2.starts_with('T');
        (paused, rest1[idx2..].trim_start())
    };
    let idx_cpu = rest_after_state.find(char::is_whitespace)?;
    let pcpu: f64 = rest_after_state[..idx_cpu].parse().ok()?;
    if !pcpu.is_finite() {
        return None;
    }
    let rest2 = rest_after_state[idx_cpu..].trim_start();
    let idx3 = rest2.find(char::is_whitespace)?;
    let rss_kb: u64 = rest2[..idx3].parse().ok()?;
    let comm = rest2[idx3..].trim();
    if comm.is_empty() {
        return None;
    }
    Some((pid, is_paused, pcpu, rss_kb, comm))
}

#[derive(Default)]
struct AppAccumulator {
    cpu_sum: f64,
    rss_kb_sum: u64,
    process_count: u32,
    paused_count: u32,
    best_cpu_for_pid: f64,
    primary_pid: u32,
}

pub fn aggregate_ps_output(raw: &str, max_items: usize) -> Vec<ProcessReading> {
    let mut grouped: BTreeMap<String, AppAccumulator> = BTreeMap::new();

    for line in raw.lines() {
        let Some((pid, is_paused, pcpu, rss_kb, comm)) = parse_ps_line(line) else {
            continue;
        };
        let name = canonical_process_name(comm);
        if name.is_empty() || name == "ps" {
            continue;
        }
        let entry = grouped.entry(name).or_insert_with(|| AppAccumulator {
            best_cpu_for_pid: -1.0,
            ..Default::default()
        });
        entry.cpu_sum += pcpu;
        entry.rss_kb_sum = entry.rss_kb_sum.saturating_add(rss_kb);
        entry.process_count = entry.process_count.saturating_add(1);
        if is_paused {
            entry.paused_count = entry.paused_count.saturating_add(1);
        }
        if pid > 0 && pcpu >= entry.best_cpu_for_pid {
            entry.best_cpu_for_pid = pcpu;
            entry.primary_pid = pid;
        }
    }

    let mut readings: Vec<ProcessReading> = grouped
        .into_iter()
        .filter_map(|(name, acc)| {
            let cpu_percent = round1(acc.cpu_sum);
            let is_paused = acc.paused_count > 0 && acc.paused_count == acc.process_count;
            if cpu_percent < 0.1 && !is_paused {
                return None;
            }
            let memory_mb = round1(acc.rss_kb_sum as f64 / 1024.0);
            Some(ProcessReading {
                name,
                cpu_percent,
                memory_mb,
                process_count: acc.process_count,
                primary_pid: acc.primary_pid,
                is_paused,
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
            canonical_process_name("/Applications/Cursor.app/Contents/MacOS/Cursor"),
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
                is_paused: false,
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
                is_paused: false,
            }
        );
    }

    #[test]
    fn parses_stopped_process_state_as_paused() {
        let sample = "\
 4001  T   0.0  262144 /Applications/Safari.app/Contents/MacOS/Safari
 4002  S  12.0  131072 /usr/local/bin/node
";
        let top = aggregate_ps_output(sample, 8);
        assert_eq!(top.len(), 2);
        let safari = top.iter().find(|p| p.name == "Safari").unwrap();
        assert!(safari.is_paused);
        let node = top.iter().find(|p| p.name == "node").unwrap();
        assert!(!node.is_paused);
    }
}
