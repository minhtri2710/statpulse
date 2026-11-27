// Storage probes: disk metrics, rebuildable-cache scanning/cleaning, SSD
// health, APFS snapshots, and the disk benchmark.

use super::cache_cleanup;
use super::SwrCache;
use crate::proc_info::{cmd_stdout, round1};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DiskMetrics {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub used_percent: u8,
    #[serde(default)]
    pub apfs_snapshots_count: u32,
    #[serde(default)]
    pub strict_free_gb: f64,
    #[serde(default)]
    pub purgeable_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct RebuildableCacheItem {
    pub id: String,
    pub label: String,
    pub path: String,
    pub size_mb: f64,
    #[serde(default)]
    pub age_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DiskBenchmarkResult {
    pub write_mb_per_sec: f64,
    pub read_mb_per_sec: f64,
    pub test_size_mb: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SsdHealthMetrics {
    pub smart_status: String,
    #[ts(type = "number | null")]
    pub wear_used_percent: Option<u8>,
    pub data_written_tb: Option<f64>,
    pub device_model: String,
}

static SSD_HEALTH_CACHE: SwrCache<SsdHealthMetrics> = SwrCache::new();

static CACHE_SCAN_CACHE: SwrCache<Vec<RebuildableCacheItem>> = SwrCache::new();

static APFS_SNAPSHOT_CACHE: SwrCache<u32> = SwrCache::new();

static WORKSPACE_ROOTS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

pub fn set_workspace_roots(roots: Vec<String>) {
    let lock = WORKSPACE_ROOTS.get_or_init(|| Mutex::new(vec!["Work".to_string()]));
    if let Ok(mut guard) = lock.lock() {
        *guard = if roots.is_empty() {
            vec!["Work".to_string()]
        } else {
            roots
        };
    }
    CACHE_SCAN_CACHE.store(Vec::new());
    super::EXTRAS_CACHE.invalidate();
}

fn current_workspace_roots() -> Vec<String> {
    WORKSPACE_ROOTS
        .get_or_init(|| Mutex::new(vec!["Work".to_string()]))
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| vec!["Work".to_string()])
}

pub fn parse_df_root(df_raw: &str) -> DiskMetrics {
    for line in df_raw.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4 {
            let total_kb = cols[1].parse::<u64>().unwrap_or(0);
            let avail_kb = cols[3].parse::<u64>().unwrap_or(0);
            let gb_denom = 1_000_000.0; // macOS displays decimal GB for storage
            let total_gb = round1((total_kb as f64 * 1.024) / gb_denom);
            let free_gb = round1((avail_kb as f64 * 1.024) / gb_denom);
            let used_gb = round1((total_gb - free_gb).max(0.0));
            let used_percent = if total_gb > 0.0 {
                ((used_gb / total_gb) * 100.0).round().clamp(0.0, 100.0) as u8
            } else {
                0
            };
            return DiskMetrics {
                total_gb,
                used_gb,
                free_gb,
                used_percent,
                apfs_snapshots_count: 0,
                strict_free_gb: free_gb,
                purgeable_gb: 0.0,
            };
        }
    }
    DiskMetrics {
        total_gb: 0.0,
        used_gb: 0.0,
        free_gb: 0.0,
        used_percent: 0,
        apfs_snapshots_count: 0,
        strict_free_gb: 0.0,
        purgeable_gb: 0.0,
    }
}

pub fn build_apfs_disk_metrics(
    total_bytes: u64,
    important_avail_bytes: u64,
    strict_avail_bytes: u64,
) -> DiskMetrics {
    let gb_denom = 1_000_000_000.0; // macOS displays decimal GB for storage
    let total_gb = round1(total_bytes as f64 / gb_denom);
    let free_gb = round1(important_avail_bytes as f64 / gb_denom).min(total_gb);
    let strict_free_gb = round1(strict_avail_bytes as f64 / gb_denom).min(free_gb);
    let purgeable_gb = round1((free_gb - strict_free_gb).max(0.0));
    let used_gb = round1((total_gb - free_gb).max(0.0));
    let used_percent = if total_gb > 0.0 {
        ((used_gb / total_gb) * 100.0).round().clamp(0.0, 100.0) as u8
    } else {
        0
    };
    DiskMetrics {
        total_gb,
        used_gb,
        free_gb,
        used_percent,
        apfs_snapshots_count: 0,
        strict_free_gb,
        purgeable_gb,
    }
}

pub fn sample_root_disk_metrics(df_fallback_raw: Option<&str>) -> DiskMetrics {
    if let Some(apfs) = crate::macos_bridge::read_native_apfs_space() {
        return build_apfs_disk_metrics(
            apfs.total_bytes,
            apfs.important_avail_bytes,
            apfs.strict_avail_bytes,
        );
    }
    let raw = df_fallback_raw
        .map(str::to_string)
        .or_else(|| cmd_stdout("/bin/df", &["-k", "/"]))
        .unwrap_or_default();
    parse_df_root(&raw)
}

/// `(logical id, label, home-relative path, minimum KiB)` for every cache
/// eligible for cleanup. Shared entries come from the module allowlist so
/// discovery and the deletion gate share one source of truth.
fn discover_cache_targets(home: &Path, roots: &[String]) -> Vec<(String, String, PathBuf, u64)> {
    let mut targets: Vec<(String, String, PathBuf, u64)> = cache_cleanup::SHARED_CACHES
        .iter()
        .map(|(id, label, rel)| (id.to_string(), label.to_string(), PathBuf::from(rel), 1024))
        .collect();
    targets.extend(
        cache_cleanup::project_cache_candidates_in_roots(home, roots)
            .into_iter()
            .map(|(id, label, rel)| (id, label, rel, 50 * 1024)),
    );
    targets
}

pub(super) fn scan_rebuildable_caches() -> Vec<RebuildableCacheItem> {
    CACHE_SCAN_CACHE.get_or_refresh(Duration::from_secs(90), scan_rebuildable_caches_uncached)
}

fn scan_rebuildable_caches_uncached() -> Vec<RebuildableCacheItem> {
    let Ok(home) = cache_cleanup::account_home() else {
        return Vec::new();
    };
    let roots = current_workspace_roots();
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut results = Vec::new();
    for (id, label, rel, min_kib) in discover_cache_targets(&home, &roots) {
        let Ok(Some(pin)) = cache_cleanup::inspect_in_roots(&home, &rel, min_kib, &roots) else {
            continue;
        };
        let age_days = if pin.mtime_unix_secs > 0 && now_secs > pin.mtime_unix_secs {
            ((now_secs - pin.mtime_unix_secs) / 86_400) as u32
        } else {
            0
        };
        results.push(RebuildableCacheItem {
            id: format!("{id}@{}:{}", pin.dev, pin.ino),
            label,
            path: home.join(&rel).to_string_lossy().into_owned(),
            size_mb: round1(pin.kib as f64 / 1024.0),
            age_days,
        });
    }
    results.sort_by(|a, b| b.size_mb.total_cmp(&a.size_mb));
    results
}

pub fn clean_rebuildable_cache(cache_id: &str) -> Result<Vec<RebuildableCacheItem>, String> {
    let home =
        cache_cleanup::account_home().map_err(|e| format!("account home is unavailable: {e}"))?;
    let roots = current_workspace_roots();

    let (logical_id, identity) = cache_id
        .rsplit_once('@')
        .ok_or_else(|| format!("Malformed cache target: {cache_id}"))?;
    let (dev, ino) = identity
        .split_once(':')
        .ok_or_else(|| format!("Malformed cache target: {cache_id}"))?;
    let expected_dev: u64 = dev
        .parse()
        .map_err(|_| format!("Malformed cache target: {cache_id}"))?;
    let expected_ino: u64 = ino
        .parse()
        .map_err(|_| format!("Malformed cache target: {cache_id}"))?;

    let (_, _, target_rel, _) = discover_cache_targets(&home, &roots)
        .into_iter()
        .find(|(id, _, _, _)| id == logical_id)
        .ok_or_else(|| format!("Unknown cache target: {cache_id}"))?;

    cache_cleanup::clean_in_roots(&home, &target_rel, expected_dev, expected_ino, &roots)
        .map_err(|e| e.to_string())?;

    let fresh = scan_rebuildable_caches_uncached();
    CACHE_SCAN_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok(fresh)
}

pub fn clean_stale_rebuildable_caches(
    min_age_days: u32,
) -> Result<(u32, Vec<RebuildableCacheItem>), String> {
    let home =
        cache_cleanup::account_home().map_err(|e| format!("account home is unavailable: {e}"))?;
    let roots = current_workspace_roots();
    let targets = discover_cache_targets(&home, &roots);
    let current = scan_rebuildable_caches_uncached();
    let mut cleaned = 0_u32;
    for item in current.iter().filter(|c| c.age_days >= min_age_days) {
        let Some((logical_id, identity)) = item.id.rsplit_once('@') else {
            continue;
        };
        let Some((dev_s, ino_s)) = identity.split_once(':') else {
            continue;
        };
        let (Ok(expected_dev), Ok(expected_ino)) = (dev_s.parse::<u64>(), ino_s.parse::<u64>())
        else {
            continue;
        };
        if let Some((_, _, rel, _)) = targets.iter().find(|(id, _, _, _)| id == logical_id) {
            if cache_cleanup::clean_in_roots(&home, rel, expected_dev, expected_ino, &roots).is_ok()
            {
                cleaned += 1;
            }
        }
    }
    let fresh = scan_rebuildable_caches_uncached();
    CACHE_SCAN_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok((cleaned, fresh))
}

pub fn parse_ssd_smart_json(nvme_json: &str, smartctl_raw: &str) -> SsdHealthMetrics {
    let mut metrics = SsdHealthMetrics {
        smart_status: "Unknown".into(),
        wear_used_percent: None,
        data_written_tb: None,
        device_model: "Apple NVMe SSD".into(),
    };

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(nvme_json) {
        if let Some(item) = val["SPNVMeDataType"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|c| c["_items"].as_array())
            .and_then(|items| items.first())
        {
            if let Some(model) = item["device_model"].as_str() {
                metrics.device_model = model.to_string();
            }
            if let Some(smart) = item["smart_status"].as_str() {
                metrics.smart_status = smart.to_string();
            }
        }
    }

    for line in smartctl_raw.lines() {
        let trimmed = line.trim();
        if let Some((_, val)) = trimmed.split_once("Percentage Used:") {
            if let Ok(pct) = val.trim().trim_end_matches('%').parse::<u8>() {
                metrics.wear_used_percent = Some(pct);
            }
        } else if let Some((_, val)) = trimmed.split_once("Data Units Written:") {
            if let Some(start) = val.find('[') {
                if let Some(end) = val[start + 1..].find(" TB]") {
                    if let Ok(tb) = val[start + 1..start + 1 + end].trim().parse::<f64>() {
                        metrics.data_written_tb = Some(round1(tb));
                    }
                }
            }
        }
    }

    metrics
}

pub(super) fn collect_ssd_health() -> SsdHealthMetrics {
    SSD_HEALTH_CACHE.get_or_refresh(Duration::from_secs(180), || {
        let nvme_json = cmd_stdout("/usr/sbin/system_profiler", &["SPNVMeDataType", "-json"])
            .unwrap_or_default();
        let smartctl_raw = ["/opt/homebrew/bin/smartctl", "/usr/local/bin/smartctl"]
            .iter()
            .find(|p| Path::new(p).exists())
            .and_then(|bin| cmd_stdout(bin, &["-A", "/dev/disk0"]))
            .unwrap_or_default();
        parse_ssd_smart_json(&nvme_json, &smartctl_raw)
    })
}

pub fn count_apfs_snapshots() -> u32 {
    APFS_SNAPSHOT_CACHE.get_or_refresh(Duration::from_secs(60), || {
        cmd_stdout("/usr/bin/tmutil", &["listlocalsnapshots", "/"])
            .map(|raw| {
                raw.lines()
                    .filter(|l| l.trim().starts_with("com.apple.TimeMachine."))
                    .count() as u32
            })
            .unwrap_or(0)
    })
}

pub fn run_nvme_benchmark() -> Result<DiskBenchmarkResult, String> {
    use std::io::{Read, Write};

    let size_mb: u32 = 64;
    let total_bytes = (size_mb as usize) * 1024 * 1024;
    let path = std::env::temp_dir().join(format!("statpulse_bench_{}.bin", std::process::id()));
    let chunk = vec![0x5A_u8; 1024 * 1024];

    let w_start = Instant::now();
    {
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        for _ in 0..size_mb {
            file.write_all(&chunk).map_err(|e| e.to_string())?;
        }
        file.sync_all().map_err(|e| e.to_string())?;
    }
    let w_secs = w_start.elapsed().as_secs_f64().max(0.001);

    let r_start = Instant::now();
    {
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut read_buf = vec![0_u8; 1024 * 1024];
        while file.read(&mut read_buf).map_err(|e| e.to_string())? > 0 {}
    }
    let r_secs = r_start.elapsed().as_secs_f64().max(0.001);
    let _ = std::fs::remove_file(&path);

    let mb_f = total_bytes as f64 / (1024.0 * 1024.0);
    Ok(DiskBenchmarkResult {
        write_mb_per_sec: (mb_f / w_secs).round(),
        read_mb_per_sec: (mb_f / r_secs).round(),
        test_size_mb: size_mb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_df_root() {
        let df = "\
Filesystem     1024-blocks      Used Available Capacity iused ifree %iused Mounted on
/dev/disk3s1s1   482797652  13433144 114421544    11% 486811 1144215440 0% /
";
        let disk = parse_df_root(df);
        assert!(disk.total_gb > 490.0);
        assert!(disk.free_gb > 110.0);
        assert_eq!(disk.strict_free_gb, disk.free_gb);
        assert_eq!(disk.purgeable_gb, 0.0);
    }

    #[test]
    fn builds_apfs_disk_metrics_with_purgeable_space() {
        let disk = build_apfs_disk_metrics(500_000_000_000, 162_000_000_000, 150_000_000_000);
        assert_eq!(disk.total_gb, 500.0);
        assert_eq!(disk.free_gb, 162.0);
        assert_eq!(disk.strict_free_gb, 150.0);
        assert_eq!(disk.purgeable_gb, 12.0);
        assert_eq!(disk.used_gb, 338.0);
        assert_eq!(disk.used_percent, 68);
    }

    #[test]
    fn parses_nvme_smart_and_wear() {
        let nvme = r#"{
            "SPNVMeDataType": [{
                "_items": [{ "device_model": "APPLE SSD AP0512R", "smart_status": "Verified" }]
            }]
        }"#;
        let smart = "Percentage Used:                    3%\nData Units Written:                 18,421,100 [9.43 TB]";
        let ssd = parse_ssd_smart_json(nvme, smart);
        assert_eq!(ssd.device_model, "APPLE SSD AP0512R");
        assert_eq!(ssd.smart_status, "Verified");
        assert_eq!(ssd.wear_used_percent, Some(3));
        assert_eq!(ssd.data_written_tb, Some(9.4));
    }
}
