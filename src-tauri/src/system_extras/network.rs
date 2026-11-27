// Network probes: interface counters/rates, per-app throughput, ping,
// Wi-Fi/Tailscale/SSH tunnel state, hosts file, HTTP port probes, and the
// networkQuality speed test.

use super::{split_token, SwrCache};
use crate::proc_info::{canonical_process_name, cmd_stdout, round1, round2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct AppNetworkUsage {
    pub name: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct WifiDiagnostics {
    pub channel: String,
    pub phy_mode: String,
    pub security: String,
    #[ts(type = "number | null")]
    pub tx_rate_mbps: Option<u32>,
    #[ts(type = "number | null")]
    pub mcs_index: Option<u32>,
    pub rssi_dbm: Option<i32>,
    pub noise_dbm: Option<i32>,
    pub snr_db: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub host_name: String,
    pub dns_name: String,
    pub tailscale_ip: Option<String>,
    pub online_peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SshTunnelItem {
    pub pid: u32,
    pub kind: String,
    pub spec: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct NetworkSnapshot {
    pub interface: String,
    pub local_ip: String,
    pub vpn_active: bool,
    pub vpn_ip: Option<String>,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub total_rx_gb: f64,
    pub total_tx_gb: f64,
    pub latency_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub packet_loss_percent: Option<f64>,
    #[serde(default)]
    pub top_net_apps: Vec<AppNetworkUsage>,
    #[serde(default)]
    pub wifi: WifiDiagnostics,
    #[serde(default)]
    pub tailscale: TailscaleStatus,
    #[serde(default)]
    pub ssh_tunnels: Vec<SshTunnelItem>,
    #[serde(default)]
    pub custom_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SpeedTestResult {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub responsiveness_rpm: u32,
    pub interface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PortHttpProbe {
    pub port: u16,
    #[ts(type = "number | null")]
    pub status_code: Option<u16>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AppConnectionItem {
    pub protocol: String,
    pub local_endpoint: String,
    pub remote_endpoint: String,
    pub state: String,
}

type PingTuple = (Option<f64>, Option<f64>, Option<f64>);

static WIFI_CACHE: SwrCache<WifiDiagnostics> = SwrCache::new();

static TAILSCALE_CACHE: SwrCache<TailscaleStatus> = SwrCache::new();

static NET_SAMPLE_STATE: OnceLock<Mutex<Option<(Instant, u64, u64)>>> = OnceLock::new();

type NettopAppState = Option<(Instant, HashMap<String, (u64, u64)>)>;

static NETTOP_APP_STATE: OnceLock<Mutex<NettopAppState>> = OnceLock::new();

static TOP_NET_APPS_CACHE: SwrCache<Vec<AppNetworkUsage>> = SwrCache::new();

static PING_CACHE: SwrCache<PingTuple> = SwrCache::new();

static SSH_TUNNELS_CACHE: SwrCache<Vec<SshTunnelItem>> = SwrCache::new();

static CUSTOM_HOSTS_CACHE: SwrCache<Vec<String>> = SwrCache::new();

pub fn parse_netstat_ibn(raw: &str) -> (String, String, bool, Option<String>, u64, u64) {
    let mut best_iface = String::from("en0");
    let mut link_bytes: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut iface_ipv4: BTreeMap<String, String> = BTreeMap::new();
    let mut vpn_ip: Option<String> = None;

    for line in raw.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 7 {
            continue;
        }
        let name = cols[0].trim_end_matches('*');
        if name.starts_with("en") {
            if cols.len() >= 11 && cols[2].starts_with("<Link#") {
                let ibytes = cols[6].parse::<u64>().unwrap_or(0);
                let obytes = cols[9].parse::<u64>().unwrap_or(0);
                link_bytes.insert(name.to_string(), (ibytes, obytes));
            } else if cols[3].contains('.') && !cols[3].starts_with("127.") {
                iface_ipv4
                    .entry(name.to_string())
                    .or_insert_with(|| cols[3].to_string());
            }
        } else if name.starts_with("utun")
            && !cols[2].starts_with("<Link#")
            && cols[3].contains('.')
            && vpn_ip.is_none()
        {
            vpn_ip = Some(cols[3].to_string());
        }
    }

    if !iface_ipv4.contains_key("en0") {
        if let Some((iface, _)) = iface_ipv4.iter().next() {
            best_iface = iface.clone();
        }
    }
    let local_ip = iface_ipv4
        .get(&best_iface)
        .cloned()
        .unwrap_or_else(|| "Offline".into());
    let (rx_bytes, tx_bytes) = link_bytes.get(&best_iface).copied().unwrap_or((0, 0));
    let vpn_active = vpn_ip.is_some();

    (best_iface, local_ip, vpn_active, vpn_ip, rx_bytes, tx_bytes)
}

pub fn parse_ping_summary(raw: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
    let mut loss = None;
    let mut avg = None;
    let mut jitter = None;

    for line in raw.lines() {
        if let Some((before_pct, _)) = line.split_once("% packet loss") {
            if let Some(tok) = before_pct.split_whitespace().last() {
                loss = tok.parse::<f64>().ok().map(round1);
            }
        } else if let Some((_, after_eq)) = line.split_once("round-trip min/avg/max/stddev =") {
            let nums: Vec<f64> = after_eq
                .trim()
                .trim_end_matches("ms")
                .trim()
                .split('/')
                .filter_map(|s| s.parse::<f64>().ok())
                .collect();
            if nums.len() >= 4 {
                avg = Some(round1(nums[1]));
                jitter = Some(round1(nums[3]));
            }
        }
    }
    (avg, jitter, loss)
}

/// Updates the eight basic counter/rate fields from one `netstat -ibn`
/// sample; both collectors funnel through here so pill never triggers the
/// diagnostic collectors below.
fn apply_network_counters(
    network: &mut NetworkSnapshot,
    best_iface: String,
    local_ip: String,
    vpn_ip: Option<String>,
    rx_bytes: u64,
    tx_bytes: u64,
) {
    let state = NET_SAMPLE_STATE.get_or_init(|| Mutex::new(None));
    let (mut rx_rate, mut tx_rate) = (0.0, 0.0);
    if let Ok(mut guard) = state.lock() {
        let now = Instant::now();
        if let Some((prev_time, prev_rx, prev_tx)) = *guard {
            let dt = now.duration_since(prev_time).as_secs_f64();
            if dt >= 0.2 {
                rx_rate = rx_bytes.saturating_sub(prev_rx) as f64 / dt;
                tx_rate = tx_bytes.saturating_sub(prev_tx) as f64 / dt;
            }
        }
        *guard = Some((now, rx_bytes, tx_bytes));
    }

    let gb = 1024.0 * 1024.0 * 1024.0;
    network.interface = best_iface;
    network.local_ip = local_ip;
    network.vpn_active = vpn_ip.is_some();
    network.vpn_ip = vpn_ip;
    network.rx_bytes_per_sec = rx_rate.round();
    network.tx_bytes_per_sec = tx_rate.round();
    network.total_rx_gb = round2(rx_bytes as f64 / gb);
    network.total_tx_gb = round2(tx_bytes as f64 / gb);
}

pub(super) fn sample_network_traffic(net_raw: &str, network: &mut NetworkSnapshot) {
    let (interface, local_ip, _vpn_active, vpn_ip, rx_bytes, tx_bytes) = parse_netstat_ibn(net_raw);
    apply_network_counters(network, interface, local_ip, vpn_ip, rx_bytes, tx_bytes);
}

#[cfg(target_os = "macos")]
fn read_ifmib_bytes(if_index: u16) -> Option<(u64, u64)> {
    #[repr(C)]
    struct IfMibData {
        ifmd_name: [u8; 16],
        ifmd_pcount: libc::c_int,
        ifmd_flags: libc::c_int,
        ifmd_snd_len: libc::c_int,
        ifmd_snd_maxlen: libc::c_int,
        ifmd_snd_drops: libc::c_int,
        ifmd_filler: [libc::c_int; 4],
        ifmd_data: libc::if_data64,
    }

    const NETLINK_GENERIC: libc::c_int = 0;
    const IFMIB_IFDATA: libc::c_int = 2;
    const IFDATA_GENERAL: libc::c_int = 1;

    let mut mib = [
        libc::CTL_NET,
        libc::PF_LINK,
        NETLINK_GENERIC,
        IFMIB_IFDATA,
        libc::c_int::from(if_index),
        IFDATA_GENERAL,
    ];
    let mut ifmd = std::mem::MaybeUninit::<IfMibData>::zeroed();
    let mut len = std::mem::size_of::<IfMibData>();
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            ifmd.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || len < std::mem::size_of::<IfMibData>()
    {
        return None;
    }
    let ifmd = unsafe { ifmd.assume_init() };
    Some((ifmd.ifmd_data.ifi_ibytes, ifmd.ifmd_data.ifi_obytes))
}

#[cfg(target_os = "macos")]
pub(super) fn sample_network_traffic_native(network: &mut NetworkSnapshot) -> bool {
    use std::ffi::CStr;
    use std::net::Ipv4Addr;

    let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut addrs) } != 0 || addrs.is_null() {
        return false;
    }

    let mut link_bytes: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut iface_ipv4: BTreeMap<String, String> = BTreeMap::new();
    let mut vpn_ip: Option<String> = None;

    let mut curr = addrs;
    while !curr.is_null() {
        let ifa = unsafe { &*curr };
        curr = ifa.ifa_next;
        if ifa.ifa_name.is_null() || ifa.ifa_addr.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
            .to_string_lossy()
            .into_owned();
        let family = unsafe { (*ifa.ifa_addr).sa_family } as i32;

        if family == libc::AF_LINK && name.starts_with("en") {
            let sdl = unsafe { std::ptr::read_unaligned(ifa.ifa_addr as *const libc::sockaddr_dl) };
            if let Some(pair) = read_ifmib_bytes(sdl.sdl_index) {
                link_bytes.insert(name, pair);
            }
        } else if family == libc::AF_INET {
            let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
            let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)).to_string();
            if name.starts_with("en") && !ip.starts_with("127.") {
                iface_ipv4.entry(name).or_insert(ip);
            } else if name.starts_with("utun") && vpn_ip.is_none() {
                vpn_ip = Some(ip);
            }
        }
    }
    unsafe { libc::freeifaddrs(addrs) };

    let mut best_iface = String::from("en0");
    if !iface_ipv4.contains_key("en0") {
        if let Some((iface, _)) = iface_ipv4.iter().next() {
            best_iface = iface.clone();
        }
    }
    let local_ip = iface_ipv4
        .get(&best_iface)
        .cloned()
        .unwrap_or_else(|| "Offline".into());
    let (rx_bytes, tx_bytes) = link_bytes.get(&best_iface).copied().unwrap_or((0, 0));
    apply_network_counters(network, best_iface, local_ip, vpn_ip, rx_bytes, tx_bytes);
    true
}

#[cfg(not(target_os = "macos"))]
pub(super) fn sample_network_traffic_native(_network: &mut NetworkSnapshot) -> bool {
    false
}

fn is_tailscale_process(name_lower: &str) -> bool {
    name_lower.starts_with("io.tailscale") || name_lower.starts_with("ipnextens")
}

pub(super) fn collect_network_snapshot() -> NetworkSnapshot {
    // Interface counters are sampled before the expensive diagnostics so the
    // rate window measures traffic, not the diagnostics' own latency.
    let mut snapshot = NetworkSnapshot::default();
    if !sample_network_traffic_native(&mut snapshot) {
        let net_raw = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
        sample_network_traffic(&net_raw, &mut snapshot);
    }
    let (latency_ms, jitter_ms, packet_loss_percent) = cached_ping();
    snapshot.latency_ms = latency_ms;
    snapshot.jitter_ms = jitter_ms;
    snapshot.packet_loss_percent = packet_loss_percent;
    snapshot.top_net_apps = collect_top_net_apps();
    snapshot.wifi = collect_wifi_diagnostics();
    snapshot.tailscale = collect_tailscale_status();
    snapshot.ssh_tunnels = collect_ssh_tunnels();
    snapshot.custom_hosts = read_custom_hosts();
    snapshot
}

pub fn parse_nettop_csv(raw: &str) -> HashMap<String, (u64, u64)> {
    let mut totals: HashMap<String, (u64, u64)> = HashMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(",bytes_in") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split(',').collect();
        if cols.len() < 3 {
            continue;
        }
        let proc_token = cols[0].trim();
        if proc_token.is_empty() {
            continue;
        }
        let base_name = match proc_token.rsplit_once('.') {
            Some((name, pid_part)) if pid_part.chars().all(|c| c.is_ascii_digit()) => name,
            _ => proc_token,
        };
        let normalized = base_name
            .split(" Helper")
            .next()
            .unwrap_or(base_name)
            .trim();
        let lower_norm = normalized.to_ascii_lowercase();
        let app_name = if is_tailscale_process(&lower_norm) {
            "Tailscale".to_string()
        } else {
            canonical_process_name(normalized)
        };
        let bytes_in = cols[1].trim().parse::<u64>().unwrap_or(0);
        let bytes_out = cols[2].trim().parse::<u64>().unwrap_or(0);
        let entry = totals.entry(app_name).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(bytes_in);
        entry.1 = entry.1.saturating_add(bytes_out);
    }
    totals
}

fn collect_top_net_apps() -> Vec<AppNetworkUsage> {
    TOP_NET_APPS_CACHE.get_or_refresh(Duration::from_secs(3), || {
        let Some(raw) = cmd_stdout(
            "/usr/bin/nettop",
            &["-P", "-L", "1", "-n", "-x", "-J", "bytes_in,bytes_out"],
        ) else {
            return Vec::new();
        };
        let current = parse_nettop_csv(&raw);
        let state = NETTOP_APP_STATE.get_or_init(|| Mutex::new(None));
        let mut results = Vec::new();

        if let Ok(mut guard) = state.lock() {
            let now = Instant::now();
            if let Some((prev_time, prev_map)) = guard.as_ref() {
                let dt = now.duration_since(*prev_time).as_secs_f64();
                if dt >= 0.3 {
                    for (name, (rx, tx)) in &current {
                        if let Some((prev_rx, prev_tx)) = prev_map.get(name) {
                            let drx = rx.saturating_sub(*prev_rx) as f64 / dt;
                            let dtx = tx.saturating_sub(*prev_tx) as f64 / dt;
                            if drx >= 128.0 || dtx >= 128.0 {
                                results.push(AppNetworkUsage {
                                    name: name.clone(),
                                    rx_bytes_per_sec: drx.round(),
                                    tx_bytes_per_sec: dtx.round(),
                                });
                            }
                        }
                    }
                }
            }
            *guard = Some((now, current));
        }

        results.sort_by(|a, b| {
            (b.rx_bytes_per_sec + b.tx_bytes_per_sec)
                .total_cmp(&(a.rx_bytes_per_sec + a.tx_bytes_per_sec))
        });
        results.truncate(6);
        results
    })
}

fn cached_ping() -> PingTuple {
    PING_CACHE.get_or_refresh(Duration::from_secs(20), || {
        cmd_stdout(
            "/sbin/ping",
            &["-n", "-q", "-c", "2", "-i", "0.15", "-W", "300", "1.1.1.1"],
        )
        .map(|s| parse_ping_summary(&s))
        .unwrap_or((None, None, None))
    })
}

pub fn run_network_quality() -> Result<SpeedTestResult, String> {
    let output = Command::new("/usr/bin/networkQuality")
        .arg("-c")
        .output()
        .map_err(|e| format!("Failed to run networkQuality: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "networkQuality failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let val: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse networkQuality JSON: {e}"))?;

    let dl_bps = val["dl_throughput"].as_f64().unwrap_or(0.0);
    let ul_bps = val["ul_throughput"].as_f64().unwrap_or(0.0);
    let responsiveness_rpm = val["responsiveness"].as_u64().unwrap_or(0) as u32;
    let interface = val["interface_name"].as_str().unwrap_or("en0").to_string();

    Ok(SpeedTestResult {
        download_mbps: round1(dl_bps / 1_000_000.0),
        upload_mbps: round1(ul_bps / 1_000_000.0),
        responsiveness_rpm,
        interface,
    })
}

pub fn parse_lsof_connections(raw: &str, target_app: &str) -> Vec<AppConnectionItem> {
    let target_lower = target_app.trim().to_ascii_lowercase();
    let prefix: String = target_lower.chars().take(7).collect();
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for line in raw.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let cmd_clean = cols[0].replace("\\x20", " ").to_ascii_lowercase();
        let matches = cmd_clean == target_lower
            || cmd_clean.starts_with(&target_lower)
            || (!prefix.is_empty() && target_lower.starts_with(&cmd_clean))
            || (target_lower == "tailscale" && is_tailscale_process(&cmd_clean));
        if !matches {
            continue;
        }
        let protocol = cols[7].to_string();
        let raw_endpoint = cols[8];
        let state = cols
            .get(9)
            .map(|s| s.trim_matches(|c| c == '(' || c == ')').to_string())
            .unwrap_or_else(|| "ACTIVE".into());
        let (local_endpoint, remote_endpoint) = match raw_endpoint.split_once("->") {
            Some((l, r)) => (l.to_string(), r.to_string()),
            None => (raw_endpoint.to_string(), "*".into()),
        };
        if seen.insert((
            protocol.clone(),
            local_endpoint.clone(),
            remote_endpoint.clone(),
        )) {
            items.push(AppConnectionItem {
                protocol,
                local_endpoint,
                remote_endpoint,
                state,
            });
            if items.len() >= 14 {
                break;
            }
        }
    }
    items
}

pub fn list_app_connections(process_name: &str) -> Result<Vec<AppConnectionItem>, String> {
    let out = Command::new("/usr/sbin/lsof")
        .args(["-i", "-n", "-P"])
        .output()
        .map_err(|e| format!("lsof failed to launch: {e}"))?;
    if out.status.success() {
        return Ok(parse_lsof_connections(
            &String::from_utf8_lossy(&out.stdout),
            process_name,
        ));
    }
    // lsof exits 1 with no output when nothing matched; a non-empty stderr or
    // any other status is an actual failure the drilldown must surface.
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.code() == Some(1) && out.stdout.is_empty() && stderr.trim().is_empty() {
        return Ok(Vec::new());
    }
    let detail = stderr.trim();
    Err(if detail.is_empty() {
        format!("lsof exited with status {}", out.status)
    } else {
        format!("lsof failed: {detail}")
    })
}

pub fn parse_wifi_profiler_json(raw: &str) -> WifiDiagnostics {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return WifiDiagnostics::default();
    };
    let Some(ifaces) = val["SPAirPortDataType"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["spairport_airport_interfaces"].as_array())
    else {
        return WifiDiagnostics::default();
    };

    for iface in ifaces {
        if iface["spairport_status_information"].as_str() != Some("spairport_status_connected") {
            continue;
        }
        let info = &iface["spairport_current_network_information"];
        if !info.is_object() {
            continue;
        }
        let channel = info["spairport_network_channel"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let phy_mode = info["spairport_network_phymode"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let security = info["spairport_security_mode"]
            .as_str()
            .unwrap_or("")
            .strip_prefix("spairport_security_mode_")
            .unwrap_or("")
            .replace('_', " ")
            .to_uppercase();
        let tx_rate_mbps = info["spairport_network_rate"].as_u64().map(|v| v as u32);
        let mcs_index = info["spairport_network_mcs"].as_u64().map(|v| v as u32);
        let (mut rssi_dbm, mut noise_dbm) = (None, None);
        if let Some(sn) = info["spairport_signal_noise"].as_str() {
            let parts: Vec<&str> = sn.split('/').collect();
            if parts.len() == 2 {
                rssi_dbm = parts[0]
                    .trim()
                    .trim_end_matches("dBm")
                    .trim()
                    .parse::<i32>()
                    .ok();
                noise_dbm = parts[1]
                    .trim()
                    .trim_end_matches("dBm")
                    .trim()
                    .parse::<i32>()
                    .ok();
            }
        }
        let snr_db = match (rssi_dbm, noise_dbm) {
            (Some(r), Some(n)) => Some(r - n),
            _ => None,
        };
        return WifiDiagnostics {
            channel,
            phy_mode,
            security,
            tx_rate_mbps,
            mcs_index,
            rssi_dbm,
            noise_dbm,
            snr_db,
        };
    }

    WifiDiagnostics::default()
}

fn collect_wifi_diagnostics() -> WifiDiagnostics {
    WIFI_CACHE.get_or_refresh(Duration::from_secs(10), || {
        if let Some(native) = crate::macos_bridge::read_native_wifi() {
            return WifiDiagnostics {
                channel: native.channel,
                phy_mode: native.phy_mode,
                security: native.security,
                tx_rate_mbps: Some(native.tx_rate_mbps),
                mcs_index: None,
                rssi_dbm: Some(native.rssi_dbm),
                noise_dbm: Some(native.noise_dbm),
                snr_db: Some(native.snr_db),
            };
        }
        let raw = cmd_stdout("/usr/sbin/system_profiler", &["SPAirPortDataType", "-json"])
            .unwrap_or_default();
        parse_wifi_profiler_json(&raw)
    })
}

pub fn parse_tailscale_json(raw: &str) -> TailscaleStatus {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return TailscaleStatus::default();
    };
    let running = val["BackendState"].as_str() == Some("Running");
    let host_name = val["Self"]["HostName"].as_str().unwrap_or("").to_string();
    let dns_name = val["Self"]["DNSName"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_string();
    let tailscale_ip = val["Self"]["TailscaleIPs"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut online_peers = Vec::new();
    if let Some(peers) = val["Peer"].as_object() {
        for peer in peers.values() {
            if peer["Online"].as_bool() == Some(true) {
                let raw_host = peer["HostName"].as_str().unwrap_or("").trim();
                let dns_label = peer["DNSName"]
                    .as_str()
                    .unwrap_or("")
                    .trim_end_matches('.')
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim();
                let name = if !raw_host.is_empty() && !raw_host.eq_ignore_ascii_case("localhost") {
                    raw_host
                } else if !dns_label.is_empty() {
                    dns_label
                } else if !raw_host.is_empty() {
                    raw_host
                } else {
                    "peer"
                };
                let ip = peer["TailscaleIPs"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !ip.is_empty() {
                    online_peers.push(format!("{name} ({ip})"));
                }
            }
        }
    }
    online_peers.sort();
    online_peers.truncate(32);
    TailscaleStatus {
        installed: true,
        running,
        host_name,
        dns_name,
        tailscale_ip,
        online_peers,
    }
}

fn find_tailscale_bin() -> Option<&'static str> {
    [
        "/opt/homebrew/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ]
    .into_iter()
    .find(|p| Path::new(p).exists())
}

fn collect_tailscale_status() -> TailscaleStatus {
    TAILSCALE_CACHE.get_or_refresh(Duration::from_secs(25), || {
        find_tailscale_bin()
            .and_then(|bin| cmd_stdout(bin, &["status", "--json"]))
            .map(|raw| parse_tailscale_json(&raw))
            .unwrap_or_default()
    })
}

pub fn parse_ssh_tunnels(ps_args_raw: &str) -> Vec<SshTunnelItem> {
    let mut tunnels = Vec::new();
    for line in ps_args_raw.lines() {
        let trimmed = line.trim();
        let Some((pid_s, args_s)) = split_token(trimmed) else {
            continue;
        };
        let Ok(pid) = pid_s.parse::<u32>() else {
            continue;
        };
        let toks: Vec<&str> = args_s.split_whitespace().collect();
        let Some(cmd0) = toks.first() else {
            continue;
        };
        let base = cmd0.rsplit('/').next().unwrap_or(cmd0);
        if base == "ssh" {
            let mut kind = None;
            let mut spec = String::new();
            let mut target = String::new();
            let mut i = 1;
            while i < toks.len() {
                let t = toks[i];
                if matches!(t, "-L" | "-R" | "-D") {
                    kind = Some(t.to_string());
                    if let Some(val) = toks.get(i + 1) {
                        spec = (*val).to_string();
                        i += 1;
                    }
                } else if !t.starts_with('-') && t != spec {
                    target = t.to_string();
                }
                i += 1;
            }
            if let Some(k) = kind {
                tunnels.push(SshTunnelItem {
                    pid,
                    kind: k,
                    spec,
                    target,
                });
            }
        } else if base == "cloudflared" && toks.contains(&"tunnel") {
            let spec = toks
                .windows(2)
                .find(|w| w[0] == "--url")
                .map(|w| w[1].to_string())
                .unwrap_or_else(|| "tunnel".into());
            let target = toks
                .iter()
                .rev()
                .find(|t| !t.starts_with('-') && **t != spec && **t != "run" && **t != "tunnel")
                .copied()
                .unwrap_or("Cloudflare")
                .to_string();
            tunnels.push(SshTunnelItem {
                pid,
                kind: "cloudflared".into(),
                spec,
                target,
            });
        } else if base == "ngrok" && toks.len() >= 3 {
            let proto = toks[1];
            if matches!(proto, "http" | "tcp" | "tls") {
                let spec = toks.get(2).copied().unwrap_or("").to_string();
                tunnels.push(SshTunnelItem {
                    pid,
                    kind: "ngrok".into(),
                    spec: format!("{proto} {spec}"),
                    target: "ngrok.io".into(),
                });
            }
        } else if base == "kubectl" && toks.contains(&"port-forward") {
            let target = toks
                .iter()
                .skip(1)
                .find(|t| !t.starts_with('-') && **t != "port-forward")
                .copied()
                .unwrap_or("k8s")
                .to_string();
            let spec = toks
                .iter()
                .rev()
                .find(|t| t.contains(':'))
                .copied()
                .unwrap_or("forward")
                .to_string();
            tunnels.push(SshTunnelItem {
                pid,
                kind: "k8s-fwd".into(),
                spec,
                target,
            });
        } else if base == "tailscale"
            && toks.get(1).is_some_and(|t| *t == "funnel" || *t == "serve")
        {
            let mode = toks[1];
            let spec = toks.get(2).copied().unwrap_or("").to_string();
            tunnels.push(SshTunnelItem {
                pid,
                kind: format!("ts-{mode}"),
                spec,
                target: "Tailnet".into(),
            });
        }
    }
    tunnels
}

fn collect_ssh_tunnels() -> Vec<SshTunnelItem> {
    SSH_TUNNELS_CACHE.get_or_refresh(Duration::from_secs(10), || {
        cmd_stdout("/bin/ps", &["-axo", "pid=,args="])
            .map(|raw| parse_ssh_tunnels(&raw))
            .unwrap_or_default()
    })
}

pub fn parse_etc_hosts(raw: &str) -> Vec<String> {
    let mut entries = Vec::new();
    for line in raw.lines() {
        let clean = line.split('#').next().unwrap_or("").trim();
        if clean.is_empty() {
            continue;
        }
        let cols: Vec<&str> = clean.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        let ip = cols[0];
        for &host in &cols[1..] {
            if matches!(host, "localhost" | "broadcasthost") || host.ends_with(".localdomain") {
                continue;
            }
            entries.push(format!("{host} → {ip}"));
            if entries.len() >= 10 {
                return entries;
            }
        }
    }
    entries
}

fn read_custom_hosts() -> Vec<String> {
    CUSTOM_HOSTS_CACHE.get_or_refresh(Duration::from_secs(30), || {
        std::fs::read_to_string("/etc/hosts")
            .map(|s| parse_etc_hosts(&s))
            .unwrap_or_default()
    })
}

pub fn parse_http_response_title(raw: &str) -> (Option<u16>, Option<String>) {
    let status_code = raw
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok());
    let lower = raw.to_ascii_lowercase();
    let title = if let (Some(start), Some(end)) = (lower.find("<title>"), lower.find("</title>")) {
        if end > start + 7 {
            let t = raw[start + 7..end].trim().replace(['\n', '\r'], " ");
            if t.is_empty() {
                None
            } else {
                Some(t.chars().take(48).collect())
            }
        } else {
            None
        }
    } else {
        None
    };
    (status_code, title)
}

pub fn probe_localhost_ports(ports: &[u16]) -> Vec<PortHttpProbe> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};

    let mut results = Vec::new();
    for &port in ports.iter().take(12) {
        let addr: SocketAddr = ([127, 0, 0, 1], port).into();
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(180)) else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(220)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(180)));
        if stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            continue;
        }
        let mut buf = [0_u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        if n > 0 {
            let text = String::from_utf8_lossy(&buf[..n]);
            let (status_code, title) = parse_http_response_title(&text);
            if status_code.is_some() || title.is_some() {
                results.push(PortHttpProbe {
                    port,
                    status_code,
                    title,
                });
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netstat_ibn() {
        let net = "\
Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
en0        1500  <Link#15>   02:00:00:00:00:00 100     0 5000000000  100     0 1500000000     0
en0        1500  192.168.100 192.168.100.135   100     - 5000000000  100     - 1500000000     -
utun6      1280  100.93.250.37 100.93.250.37   100     - 1000        100     - 1000           -
";
        let (iface, ip, vpn, vpn_ip, rx, tx) = parse_netstat_ibn(net);
        assert_eq!(iface, "en0");
        assert_eq!(ip, "192.168.100.135");
        assert!(vpn);
        assert_eq!(vpn_ip.as_deref(), Some("100.93.250.37"));
        assert_eq!(rx, 5_000_000_000);
        assert_eq!(tx, 1_500_000_000);
    }

    #[test]
    fn parses_ping_summary() {
        let ping = "\
2 packets transmitted, 2 packets received, 0.0% packet loss
round-trip min/avg/max/stddev = 14.2/16.8/19.4/2.6 ms
";
        let (avg, jitter, loss) = parse_ping_summary(ping);
        assert_eq!(avg, Some(16.8));
        assert_eq!(jitter, Some(2.6));
        assert_eq!(loss, Some(0.0));
    }

    #[test]
    fn parses_nettop_csv() {
        let nettop = "\
,bytes_in,bytes_out,
apsd.591,1000,200,
Cursor Helper (Renderer).112,5000,3000,
Cursor.100,2000,1000,
io.tailscale.ip.772,400,600,
Tailscale.770,100,200,
";
        let totals = parse_nettop_csv(nettop);
        assert_eq!(totals.get("apsd").copied(), Some((1000, 200)));
        assert_eq!(totals.get("Cursor").copied(), Some((7000, 4000)));
        assert_eq!(totals.get("Tailscale").copied(), Some((500, 800)));
    }

    #[test]
    fn parses_lsof_connections() {
        let lsof_net = "\
COMMAND   PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
Cursor  500 me 12u IPv4 0x1 0t0 TCP 192.168.1.5:51234->104.18.2.1:443 (ESTABLISHED)
node    501 me 14u IPv4 0x2 0t0 TCP 127.0.0.1:3000 (LISTEN)
IPNExtens 772 me 8u IPv4 0x3 0t0 UDP *:41641
";
        let conns = parse_lsof_connections(lsof_net, "Cursor");
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].remote_endpoint, "104.18.2.1:443");
        assert_eq!(conns[0].state, "ESTABLISHED");

        let ts_conns = parse_lsof_connections(lsof_net, "Tailscale");
        assert_eq!(ts_conns.len(), 1);
        assert_eq!(ts_conns[0].protocol, "UDP");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires live macOS host"]
    fn native_network_sampling_matches_64bit_netstat() {
        let mut snap = NetworkSnapshot::default();
        assert!(sample_network_traffic_native(&mut snap));
        let net_raw = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap();
        let (iface, ip, _vpn, _vpn_ip, rx_bytes, tx_bytes) = parse_netstat_ibn(&net_raw);
        assert_eq!(snap.interface, iface);
        assert_eq!(snap.local_ip, ip);
        let gb = 1024.0 * 1024.0 * 1024.0;
        let expected_rx_gb = round2(rx_bytes as f64 / gb);
        let expected_tx_gb = round2(tx_bytes as f64 / gb);
        assert!(
            (snap.total_rx_gb - expected_rx_gb).abs() <= 0.1,
            "native rx {} vs netstat {}",
            snap.total_rx_gb,
            expected_rx_gb
        );
        assert!(
            (snap.total_tx_gb - expected_tx_gb).abs() <= 0.1,
            "native tx {} vs netstat {}",
            snap.total_tx_gb,
            expected_tx_gb
        );
    }

    #[test]
    fn parses_wifi_tailscale_ssh_hosts_and_http_title() {
        let wifi_json = r#"{
            "SPAirPortDataType": [{
                "spairport_airport_interfaces": [{
                    "spairport_status_information": "spairport_status_connected",
                    "spairport_current_network_information": {
                        "spairport_network_channel": "48 (5GHz, 80MHz)",
                        "spairport_network_phymode": "802.11ac",
                        "spairport_network_rate": 866,
                        "spairport_network_mcs": 9,
                        "spairport_security_mode": "spairport_security_mode_wpa2_personal",
                        "spairport_signal_noise": "-43 dBm / -88 dBm"
                    }
                }]
            }]
        }"#;
        let wifi = parse_wifi_profiler_json(wifi_json);
        assert_eq!(wifi.tx_rate_mbps, Some(866));
        assert_eq!(wifi.rssi_dbm, Some(-43));
        assert_eq!(wifi.noise_dbm, Some(-88));
        assert_eq!(wifi.snr_db, Some(45));

        let ts_json = r#"{
            "BackendState": "Running",
            "Self": { "HostName": "beos-mac", "DNSName": "beos-mac.ts.net.", "TailscaleIPs": ["100.93.250.37"] },
            "Peer": {
                "k1": { "HostName": "prod-db", "Online": true, "TailscaleIPs": ["100.64.0.9"] },
                "k2": { "HostName": "localhost", "DNSName": "beos-phone.taila6e3f9.ts.net.", "Online": true, "TailscaleIPs": ["100.85.225.78"] }
            }
        }"#;
        let ts = parse_tailscale_json(ts_json);
        assert!(ts.running);
        assert_eq!(ts.tailscale_ip.as_deref(), Some("100.93.250.37"));
        assert_eq!(
            ts.online_peers,
            vec!["beos-phone (100.85.225.78)", "prod-db (100.64.0.9)"]
        );

        let ssh_raw = "\
41200 /usr/bin/ssh -N -L 5432:127.0.0.1:5432 bastion
41201 /usr/bin/zsh
41202 /opt/homebrew/bin/cloudflared tunnel --url http://localhost:3000
41203 /opt/homebrew/bin/ngrok http 8080
41204 /opt/homebrew/bin/kubectl port-forward svc/api 9000:80
";
        let tunnels = parse_ssh_tunnels(ssh_raw);
        assert_eq!(tunnels.len(), 4);
        assert_eq!(tunnels[0].pid, 41200);
        assert_eq!(tunnels[0].spec, "5432:127.0.0.1:5432");
        assert_eq!(tunnels[1].kind, "cloudflared");
        assert_eq!(tunnels[1].spec, "http://localhost:3000");
        assert_eq!(tunnels[2].kind, "ngrok");
        assert_eq!(tunnels[3].kind, "k8s-fwd");
        assert_eq!(tunnels[3].spec, "9000:80");

        let hosts = parse_etc_hosts("127.0.0.1 localhost app.local\n192.168.1.50 nas.home\n");
        assert_eq!(
            hosts,
            vec!["app.local → 127.0.0.1", "nas.home → 192.168.1.50"]
        );

        let (code, title) = parse_http_response_title(
            "HTTP/1.1 200 OK\r\n\r\n<html><head><title>Vite App</title></head></html>",
        );
        assert_eq!(code, Some(200));
        assert_eq!(title.as_deref(), Some("Vite App"));
    }
}
