use crate::awake;
use crate::hardware_telemetry::adapters::process::canonical_process_name;
use crate::ports::{self, ListeningPort};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct MemoryMetrics {
    pub total_gb: f64,
    pub used_gb: f64,
    pub app_gb: f64,
    pub wired_gb: f64,
    pub compressed_gb: f64,
    pub swap_used_mb: f64,
    pub pressure_level: String,
    #[serde(default)]
    pub page_ins_per_sec: f64,
    #[serde(default)]
    pub swap_outs_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DiskMetrics {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub used_percent: u8,
    #[serde(default)]
    pub apfs_snapshots_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct RebuildableCacheItem {
    pub id: String,
    pub label: String,
    pub path: String,
    pub size_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SelfCostMetrics {
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
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
pub struct BluetoothDeviceItem {
    pub name: String,
    pub device_type: String,
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub rssi: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct UsbDeviceItem {
    pub name: String,
    pub depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ChargerAdapterInfo {
    pub connected: bool,
    pub watts: Option<u32>,
    pub voltage_v: Option<f64>,
    pub current_a: Option<f64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct BatteryHealthVitals {
    #[ts(type = "number | null")]
    pub design_capacity_mah: Option<u32>,
    #[ts(type = "number | null")]
    pub nominal_capacity_mah: Option<u32>,
    #[ts(type = "number | null")]
    pub raw_max_capacity_mah: Option<u32>,
    pub health_percent: Option<u8>,
    pub voltage_v: Option<f64>,
    pub amperage_ma: Option<i32>,
    #[ts(type = "number | null")]
    pub time_remaining_min: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DevicesSnapshot {
    pub bluetooth: Vec<BluetoothDeviceItem>,
    pub usb: Vec<UsbDeviceItem>,
    pub charger: ChargerAdapterInfo,
    #[serde(default)]
    pub battery_health: BatteryHealthVitals,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AudioSnapshot {
    pub output_volume: u8,
    pub input_volume: u8,
    pub output_muted: bool,
    pub mic_in_use_by: Option<String>,
    #[serde(default)]
    pub camera_in_use: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PortHttpProbe {
    pub port: u16,
    #[ts(type = "number | null")]
    pub status_code: Option<u16>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct DiskBenchmarkResult {
    pub write_mb_per_sec: f64,
    pub read_mb_per_sec: f64,
    pub test_size_mb: u32,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SiliconPowerBreakdown {
    pub cpu_watts: f64,
    pub gpu_watts: f64,
    pub ane_watts: f64,
    pub dram_watts: f64,
    pub package_watts: f64,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct OllamaModelItem {
    pub name: String,
    pub id: String,
    pub size_gb: f64,
    pub processor: String,
    pub until: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AppConnectionItem {
    pub protocol: String,
    pub local_endpoint: String,
    pub remote_endpoint: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SystemExtrasSnapshot {
    pub memory: MemoryMetrics,
    pub disk: DiskMetrics,
    pub caches: Vec<RebuildableCacheItem>,
    pub self_cost: SelfCostMetrics,
    pub network: NetworkSnapshot,
    pub agents: Vec<AgentSessionItem>,
    pub audio: AudioSnapshot,
    #[serde(default)]
    pub orphaned_servers: Vec<OrphanedProcessItem>,
    #[serde(default)]
    pub thermal_pressure: String,
    #[serde(default)]
    pub silicon_power: SiliconPowerBreakdown,
    #[serde(default)]
    pub ssd_health: SsdHealthMetrics,
    #[serde(default)]
    pub ollama_models: Vec<OllamaModelItem>,
    #[serde(default)]
    pub containers: Vec<ContainerItem>,
    #[serde(default)]
    pub launch_agents: Vec<LaunchAgentItem>,
    #[serde(default)]
    pub quick_switches: QuickSwitchState,
}

static MEMSIZE_BYTES_CACHE: OnceLock<u64> = OnceLock::new();
static SYSTEM_EXTRAS_CACHE: OnceLock<Mutex<Option<(Instant, SystemExtrasSnapshot)>>> =
    OnceLock::new();

#[derive(Default)]
struct SwrEntry<T> {
    updated_at: Option<Instant>,
    in_flight: bool,
    value: T,
}

type PingTuple = (Option<f64>, Option<f64>, Option<f64>);

static SSD_HEALTH_CACHE: OnceLock<Mutex<SwrEntry<SsdHealthMetrics>>> = OnceLock::new();
static OLLAMA_CACHE: OnceLock<Mutex<SwrEntry<Vec<OllamaModelItem>>>> = OnceLock::new();
static CONTAINER_CACHE: OnceLock<Mutex<SwrEntry<Vec<ContainerItem>>>> = OnceLock::new();
static WIFI_CACHE: OnceLock<Mutex<SwrEntry<WifiDiagnostics>>> = OnceLock::new();
static TAILSCALE_CACHE: OnceLock<Mutex<SwrEntry<TailscaleStatus>>> = OnceLock::new();
static LAUNCH_AGENTS_CACHE: OnceLock<Mutex<SwrEntry<Vec<LaunchAgentItem>>>> = OnceLock::new();

static NET_SAMPLE_STATE: OnceLock<Mutex<Option<(Instant, u64, u64)>>> = OnceLock::new();
static NETTOP_APP_STATE: OnceLock<Mutex<Option<(Instant, HashMap<String, (u64, u64)>)>>> =
    OnceLock::new();
static TOP_NET_APPS_CACHE: OnceLock<Mutex<SwrEntry<Vec<AppNetworkUsage>>>> = OnceLock::new();
static VM_PAGING_STATE: OnceLock<Mutex<Option<(Instant, u64, u64)>>> = OnceLock::new();
static PING_CACHE: OnceLock<Mutex<SwrEntry<PingTuple>>> = OnceLock::new();
static CACHE_SCAN_CACHE: OnceLock<Mutex<SwrEntry<Vec<RebuildableCacheItem>>>> = OnceLock::new();

static APFS_SNAPSHOT_CACHE: OnceLock<Mutex<SwrEntry<u32>>> = OnceLock::new();
static AUDIO_VOLUME_CACHE: OnceLock<Mutex<SwrEntry<Option<(u8, u8, bool)>>>> = OnceLock::new();
static AV_PRIVACY_CACHE: OnceLock<Mutex<SwrEntry<(Option<String>, bool)>>> = OnceLock::new();
static QUICK_SWITCH_CACHE: OnceLock<Mutex<SwrEntry<QuickSwitchState>>> = OnceLock::new();
static SSH_TUNNELS_CACHE: OnceLock<Mutex<SwrEntry<Vec<SshTunnelItem>>>> = OnceLock::new();
static CUSTOM_HOSTS_CACHE: OnceLock<Mutex<Option<(Instant, Vec<String>)>>> = OnceLock::new();
static THERMAL_PRESSURE_CACHE: OnceLock<Mutex<SwrEntry<Option<String>>>> = OnceLock::new();

static BACKGROUND_SWR_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub fn set_background_swr_enabled(enabled: bool) {
    BACKGROUND_SWR_ENABLED.store(enabled, std::sync::atomic::Ordering::SeqCst);
}

fn get_or_refresh_swr<T, F>(
    cell: &'static OnceLock<Mutex<SwrEntry<T>>>,
    ttl: Duration,
    fetcher: F,
) -> T
where
    T: Clone + Default + Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let lock = cell.get_or_init(|| Mutex::new(SwrEntry::default()));
    let mut guard = match lock.lock() {
        Ok(g) => g,
        Err(_) => return T::default(),
    };
    let is_stale = guard
        .updated_at
        .map(|ts| ts.elapsed() >= ttl)
        .unwrap_or(true);
    let current = guard.value.clone();
    if is_stale
        && !guard.in_flight
        && BACKGROUND_SWR_ENABLED.load(std::sync::atomic::Ordering::SeqCst)
    {
        guard.in_flight = true;
        drop(guard);
        std::thread::spawn(move || {
            let fresh = std::panic::catch_unwind(std::panic::AssertUnwindSafe(fetcher));
            if let Ok(mut guard) = lock.lock() {
                if let Ok(val) = fresh {
                    guard.value = val;
                    guard.updated_at = Some(Instant::now());
                }
                guard.in_flight = false;
            }
        });
    }
    current
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn cmd_stdout(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    if out.stdout.is_empty() && !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn default_extras_snapshot(memsize_bytes: u64) -> SystemExtrasSnapshot {
    let total_gb = round1(memsize_bytes as f64 / (1024.0 * 1024.0 * 1024.0));
    SystemExtrasSnapshot {
        memory: MemoryMetrics {
            total_gb,
            used_gb: 0.0,
            app_gb: 0.0,
            wired_gb: 0.0,
            compressed_gb: 0.0,
            swap_used_mb: 0.0,
            pressure_level: "nominal".into(),
            page_ins_per_sec: 0.0,
            swap_outs_per_sec: 0.0,
        },
        disk: DiskMetrics {
            total_gb: 0.0,
            used_gb: 0.0,
            free_gb: 0.0,
            used_percent: 0,
            apfs_snapshots_count: 0,
        },
        caches: Vec::new(),
        self_cost: SelfCostMetrics {
            cpu_percent: 0.0,
            memory_mb: 0.0,
        },
        network: NetworkSnapshot {
            interface: "en0".into(),
            local_ip: String::new(),
            vpn_active: false,
            vpn_ip: None,
            rx_bytes_per_sec: 0.0,
            tx_bytes_per_sec: 0.0,
            total_rx_gb: 0.0,
            total_tx_gb: 0.0,
            latency_ms: None,
            jitter_ms: None,
            packet_loss_percent: None,
            top_net_apps: Vec::new(),
            wifi: WifiDiagnostics::default(),
            tailscale: TailscaleStatus::default(),
            ssh_tunnels: Vec::new(),
            custom_hosts: Vec::new(),
        },
        agents: Vec::new(),
        audio: AudioSnapshot {
            output_volume: 50,
            input_volume: 50,
            output_muted: false,
            mic_in_use_by: None,
            camera_in_use: false,
        },
        orphaned_servers: Vec::new(),
        thermal_pressure: "nominal".into(),
        silicon_power: SiliconPowerBreakdown::default(),
        ssd_health: SsdHealthMetrics {
            smart_status: "Unknown".into(),
            wear_used_percent: None,
            data_written_tb: None,
            device_model: "Apple NVMe SSD".into(),
        },
        ollama_models: Vec::new(),
        containers: Vec::new(),
        launch_agents: Vec::new(),
        quick_switches: QuickSwitchState::default(),
    }
}

pub fn collect_pill_extras() -> SystemExtrasSnapshot {
    let extras_cache = SYSTEM_EXTRAS_CACHE.get_or_init(|| Mutex::new(None));
    let mut base_snapshot = None;
    if let Ok(guard) = extras_cache.lock() {
        if let Some((ts, cached)) = guard.as_ref() {
            if ts.elapsed() < Duration::from_millis(1500) {
                return cached.clone();
            }
            base_snapshot = Some(cached.clone());
        }
    }

    let memsize_bytes = *MEMSIZE_BYTES_CACHE.get_or_init(|| {
        cmd_stdout("/usr/sbin/sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
    });

    let ((vm_stat_raw, swap_raw, net_raw), ps_raw) = std::thread::scope(|s| {
        let sys_h = s.spawn(|| {
            let vm = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
            let swap = cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
            let net = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
            (vm, swap, net)
        });
        let ps_h = s.spawn(|| {
            cmd_stdout(
                "/bin/ps",
                &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="],
            )
            .unwrap_or_default()
        });
        (
            sys_h.join().unwrap_or_default(),
            ps_h.join().unwrap_or_default(),
        )
    });

    let mut snap = base_snapshot.unwrap_or_else(|| default_extras_snapshot(memsize_bytes));
    let mut memory = parse_vm_stat_and_mem(memsize_bytes, &vm_stat_raw, &swap_raw);
    let (pageins, swapouts) = extract_vm_paging_counters(&vm_stat_raw);
    let (pi_rate, so_rate) = compute_vm_paging_rates(pageins, swapouts);
    memory.page_ins_per_sec = pi_rate;
    memory.swap_outs_per_sec = so_rate;
    snap.memory = memory;

    let (interface, local_ip, vpn_active, vpn_ip, rx_bytes, tx_bytes) = parse_netstat_ibn(&net_raw);
    let net_state = NET_SAMPLE_STATE.get_or_init(|| Mutex::new(None));
    let (mut rx_rate, mut tx_rate) = (0.0, 0.0);
    if let Ok(mut guard) = net_state.lock() {
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
    snap.network.interface = interface;
    snap.network.local_ip = local_ip;
    snap.network.vpn_active = vpn_active;
    snap.network.vpn_ip = vpn_ip;
    snap.network.rx_bytes_per_sec = rx_rate.round();
    snap.network.tx_bytes_per_sec = tx_rate.round();
    snap.network.total_rx_gb = round2(rx_bytes as f64 / gb);
    snap.network.total_tx_gb = round2(tx_bytes as f64 / gb);

    let cached_ports = ports::cached_listening_ports_snapshot();
    snap.self_cost = parse_self_cost(&ps_raw, std::process::id());
    snap.agents = detect_ai_agents(&ps_raw, &cached_ports);

    if let Some(pkg_w) = crate::smc::get_system_power_watts() {
        snap.silicon_power.package_watts = round1(pkg_w);
    }

    if let Ok(mut guard) = extras_cache.lock() {
        *guard = Some((Instant::now(), snap.clone()));
    }
    snap
}

pub fn collect_system_extras() -> SystemExtrasSnapshot {
    let extras_cache = SYSTEM_EXTRAS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = extras_cache.lock() {
        if let Some((ts, cached)) = guard.as_ref() {
            if ts.elapsed() < Duration::from_millis(1500) {
                return cached.clone();
            }
        }
    }

    let memsize_bytes = *MEMSIZE_BYTES_CACHE.get_or_init(|| {
        cmd_stdout("/usr/sbin/sysctl", &["-n", "hw.memsize"])
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
    });

    let ((vm_stat_raw, swap_raw, df_raw, net_raw), ps_raw, listening_ports) =
        std::thread::scope(|s| {
            let sys_handle = s.spawn(|| {
                let vm = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
                let swap =
                    cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
                let df = cmd_stdout("/bin/df", &["-k", "/"]).unwrap_or_default();
                let net = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
                (vm, swap, df, net)
            });
            let ps_handle = s.spawn(|| {
                cmd_stdout(
                    "/bin/ps",
                    &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="],
                )
                .unwrap_or_default()
            });
            let ports = ports::list_listening_ports_fast().unwrap_or_default();
            (
                sys_handle.join().unwrap_or_default(),
                ps_handle.join().unwrap_or_default(),
                ports,
            )
        });

    let mut memory = parse_vm_stat_and_mem(memsize_bytes, &vm_stat_raw, &swap_raw);
    let (pageins, swapouts) = extract_vm_paging_counters(&vm_stat_raw);
    let (pi_rate, so_rate) = compute_vm_paging_rates(pageins, swapouts);
    memory.page_ins_per_sec = pi_rate;
    memory.swap_outs_per_sec = so_rate;

    let mut disk = parse_df_root(&df_raw);
    disk.apfs_snapshots_count = count_apfs_snapshots();

    let caches = scan_rebuildable_caches();
    let self_cost = parse_self_cost(&ps_raw, std::process::id());
    let agents = detect_ai_agents(&ps_raw, &listening_ports);
    let orphaned_servers = detect_orphaned_dev_servers(&ps_raw, &listening_ports);
    let network = collect_network_snapshot(&net_raw);
    let audio = collect_audio_snapshot(&ps_raw);
    let (silicon_power, thermal_pressure) = collect_silicon_power_and_pressure(&ps_raw);
    let ssd_health = collect_ssd_health();
    let ollama_models = collect_ollama_models();
    let containers = collect_containers();
    let launch_agents = collect_launch_agents();
    let quick_switches = read_quick_switch_state();

    let snapshot = SystemExtrasSnapshot {
        memory,
        disk,
        caches,
        self_cost,
        network,
        agents,
        audio,
        orphaned_servers,
        thermal_pressure,
        silicon_power,
        ssd_health,
        ollama_models,
        containers,
        launch_agents,
        quick_switches,
    };

    if let Ok(mut guard) = extras_cache.lock() {
        *guard = Some((Instant::now(), snapshot.clone()));
    }
    snapshot
}

fn extract_vm_paging_counters(vm_stat_raw: &str) -> (u64, u64) {
    let mut pageins = 0_u64;
    let mut swapouts = 0_u64;
    for line in vm_stat_raw.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once(':') {
            let count = v
                .trim()
                .trim_end_matches('.')
                .parse::<u64>()
                .unwrap_or(0);
            match k.trim() {
                "Pageins" => pageins = count,
                "Swapouts" => swapouts = count,
                _ => {}
            }
        }
    }
    (pageins, swapouts)
}

fn compute_vm_paging_rates(pageins: u64, swapouts: u64) -> (f64, f64) {
    let state = VM_PAGING_STATE.get_or_init(|| Mutex::new(None));
    let mut rates = (0.0, 0.0);
    if let Ok(mut guard) = state.lock() {
        let now = Instant::now();
        if let Some((prev_time, prev_pi, prev_so)) = *guard {
            let dt = now.duration_since(prev_time).as_secs_f64();
            if dt >= 0.2 {
                rates = (
                    round1(pageins.saturating_sub(prev_pi) as f64 / dt),
                    round1(swapouts.saturating_sub(prev_so) as f64 / dt),
                );
            }
        }
        *guard = Some((now, pageins, swapouts));
    }
    rates
}

pub fn parse_vm_stat_and_mem(memsize_bytes: u64, vm_stat_raw: &str, swap_raw: &str) -> MemoryMetrics {
    let mut page_size: u64 = 16384;
    let mut active_pages: u64 = 0;
    let mut wired_pages: u64 = 0;
    let mut compressed_pages: u64 = 0;

    for line in vm_stat_raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Mach Virtual Memory Statistics: (page size of ") {
            if let Some(num) = rest.split_whitespace().next() {
                if let Ok(ps) = num.parse::<u64>() {
                    page_size = ps;
                }
            }
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let count = v
                .trim()
                .trim_end_matches('.')
                .parse::<u64>()
                .unwrap_or(0);
            match k.trim() {
                "Pages active" => active_pages = count,
                "Pages wired down" => wired_pages = count,
                "Pages occupied by compressor" => compressed_pages = count,
                _ => {}
            }
        }
    }

    let swap_used_mb = parse_swap_used_mb(swap_raw);

    let gb = 1024.0 * 1024.0 * 1024.0;
    let total_gb = round1(memsize_bytes as f64 / gb);
    let app_gb = round1((active_pages * page_size) as f64 / gb);
    let wired_gb = round1((wired_pages * page_size) as f64 / gb);
    let compressed_gb = round1((compressed_pages * page_size) as f64 / gb);
    let used_gb = round1((app_gb + wired_gb + compressed_gb).min(total_gb.max(app_gb)));

    let ratio = if total_gb > 0.0 {
        used_gb / total_gb
    } else {
        0.0
    };
    let pressure_level = if ratio > 0.88 || swap_used_mb > 4096.0 {
        "critical"
    } else if ratio > 0.72 || swap_used_mb > 1024.0 {
        "warn"
    } else {
        "ok"
    }
    .to_string();

    MemoryMetrics {
        total_gb,
        used_gb,
        app_gb,
        wired_gb,
        compressed_gb,
        swap_used_mb: round1(swap_used_mb),
        pressure_level,
        page_ins_per_sec: 0.0,
        swap_outs_per_sec: 0.0,
    }
}

fn parse_swap_used_mb(swap_raw: &str) -> f64 {
    if let Some((_, after_used)) = swap_raw.split_once("used = ") {
        if let Some(tok) = after_used.split_whitespace().next() {
            if let Some(num) = tok.strip_suffix('M') {
                return num.parse::<f64>().unwrap_or(0.0);
            }
            if let Some(num) = tok.strip_suffix('G') {
                return num.parse::<f64>().unwrap_or(0.0) * 1024.0;
            }
        }
    }
    0.0
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
            };
        }
    }
    DiskMetrics {
        total_gb: 0.0,
        used_gb: 0.0,
        free_gb: 0.0,
        used_percent: 0,
        apfs_snapshots_count: 0,
    }
}

fn discover_cache_targets(home: &Path) -> Vec<(String, String, PathBuf, f64)> {
    let mut targets = vec![
        (
            "xcode_derived".to_string(),
            "Xcode DerivedData".to_string(),
            home.join("Library/Developer/Xcode/DerivedData"),
            1.0,
        ),
        (
            "npm_cache".to_string(),
            "npm Cache".to_string(),
            home.join(".npm/_cacache"),
            1.0,
        ),
        (
            "homebrew_cache".to_string(),
            "Homebrew Cache".to_string(),
            home.join("Library/Caches/Homebrew"),
            1.0,
        ),
        (
            "cargo_cache".to_string(),
            "Cargo Registry Cache".to_string(),
            home.join(".cargo/registry/cache"),
            1.0,
        ),
    ];

    let work_dir = home.join("Work");
    if let Ok(projects) = std::fs::read_dir(&work_dir) {
        for proj in projects.flatten().take(32) {
            let proj_path = proj.path();
            if std::fs::symlink_metadata(&proj_path)
                .map(|m| m.file_type().is_symlink() || !m.is_dir())
                .unwrap_or(true)
            {
                continue;
            }
            let Some(proj_name) = proj_path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            for artifact in ["node_modules", "target", ".next", ".venv"] {
                let candidate = proj_path.join(artifact);
                if candidate.is_dir() {
                    targets.push((
                        format!("proj::{proj_name}::{artifact}"),
                        format!("{proj_name}/{artifact}"),
                        candidate,
                        50.0,
                    ));
                }
            }
        }
    }

    targets
}

fn scan_rebuildable_caches() -> Vec<RebuildableCacheItem> {
    get_or_refresh_swr(
        &CACHE_SCAN_CACHE,
        Duration::from_secs(90),
        scan_rebuildable_caches_uncached,
    )
}

fn scan_rebuildable_caches_uncached() -> Vec<RebuildableCacheItem> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut results = Vec::new();
    for (id, label, path, min_mb) in discover_cache_targets(&home) {
        if !path.is_dir() {
            continue;
        }
        let size_kb = cmd_stdout("/usr/bin/du", &["-sk", &path.to_string_lossy()])
            .and_then(|out| out.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0);
        let size_mb = round1(size_kb as f64 / 1024.0);
        if size_mb >= min_mb {
            results.push(RebuildableCacheItem {
                id,
                label,
                path: path.to_string_lossy().into_owned(),
                size_mb,
            });
        }
    }
    results.sort_by(|a, b| b.size_mb.total_cmp(&a.size_mb));
    results
}

pub fn clean_rebuildable_cache(cache_id: &str) -> Result<Vec<RebuildableCacheItem>, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME directory is unavailable".to_string())?;
    let canonical_home = std::fs::canonicalize(&home).map_err(|e| e.to_string())?;

    let (_, _, target_path, _) = discover_cache_targets(&home)
        .into_iter()
        .find(|(id, _, _, _)| id == cache_id)
        .ok_or_else(|| format!("Unknown cache target: {cache_id}"))?;

    if target_path.exists() {
        let canonical_target = std::fs::canonicalize(&target_path).map_err(|e| e.to_string())?;
        if !canonical_target.starts_with(&canonical_home) || canonical_target == canonical_home {
            return Err("Refusing to clean path outside user home directory.".into());
        }
        if cache_id.starts_with("proj::") {
            std::fs::remove_dir_all(&canonical_target).map_err(|e| e.to_string())?;
        } else {
            let entries = std::fs::read_dir(&canonical_target).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    std::fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
                } else {
                    std::fs::remove_file(&p).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    let fresh = scan_rebuildable_caches_uncached();
    if let Some(cache) = CACHE_SCAN_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            guard.value = fresh.clone();
            guard.updated_at = Some(Instant::now());
            guard.in_flight = false;
        }
    }
    Ok(fresh)
}

#[derive(Clone, Debug)]
struct PsRow {
    pid: u32,
    ppid: u32,
    etime: String,
    pcpu: f64,
    rss_kb: u64,
    comm: String,
}

fn parse_ps_rows(ps_raw: &str) -> Vec<PsRow> {
    let mut rows = Vec::new();
    for line in ps_raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        let mut parts = s.splitn(6, char::is_whitespace);
        let Some(pid_s) = parts.next().filter(|t| !t.is_empty()) else {
            continue;
        };
        let rest1 = s[pid_s.len()..].trim_start();
        let Some((ppid_s, rest2)) = split_token(rest1) else {
            continue;
        };
        let Some((etime_s, rest3)) = split_token(rest2) else {
            continue;
        };
        let Some((pcpu_s, rest4)) = split_token(rest3) else {
            continue;
        };
        let Some((rss_s, comm_s)) = split_token(rest4) else {
            continue;
        };
        let (Ok(pid), Ok(ppid), Ok(pcpu), Ok(rss_kb)) = (
            pid_s.parse::<u32>(),
            ppid_s.parse::<u32>(),
            pcpu_s.parse::<f64>(),
            rss_s.parse::<u64>(),
        ) else {
            continue;
        };
        let comm = comm_s.trim().to_string();
        if !comm.is_empty() {
            rows.push(PsRow {
                pid,
                ppid,
                etime: etime_s.to_string(),
                pcpu,
                rss_kb,
                comm,
            });
        }
    }
    rows
}

fn split_token(input: &str) -> Option<(&str, &str)> {
    let s = input.trim_start();
    let idx = s.find(char::is_whitespace)?;
    Some((&s[..idx], s[idx..].trim_start()))
}

fn parse_self_cost(ps_raw: &str, self_pid: u32) -> SelfCostMetrics {
    for row in parse_ps_rows(ps_raw) {
        if row.pid == self_pid {
            return SelfCostMetrics {
                cpu_percent: round1(row.pcpu),
                memory_mb: round1(row.rss_kb as f64 / 1024.0),
            };
        }
    }
    SelfCostMetrics {
        cpu_percent: 0.0,
        memory_mb: 0.0,
    }
}

fn classify_agent(comm: &str) -> Option<&'static str> {
    let canonical = canonical_process_name(comm);
    let lower = canonical.to_ascii_lowercase();
    if lower == "claude" || lower == "claude code" {
        Some("Claude Code")
    } else if lower == "codex" {
        Some("Codex")
    } else if lower == "cursor" {
        Some("Cursor")
    } else if lower == "agy" || lower == "antigravity" {
        Some("Antigravity")
    } else if lower == "ollama" {
        Some("Ollama")
    } else {
        None
    }
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

    // Group matched agent roots by agent_name
    let mut grouped: BTreeMap<String, Vec<&PsRow>> = BTreeMap::new();
    for row in &rows {
        if let Some(agent_name) = classify_agent(&row.comm) {
            grouped.entry(agent_name.to_string()).or_default().push(row);
        }
    }

    let mut sessions = Vec::new();
    for (agent_name, roots) in grouped {
        let mut root_pids: HashSet<u32> = roots.iter().map(|r| r.pid).collect();
        let mut all_pids: HashSet<u32> = root_pids.clone();

        // Walk 2 levels of child processes
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
            if classify_agent(&row.comm).is_some() && row.pcpu >= best_cpu {
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

fn collect_network_snapshot(net_raw: &str) -> NetworkSnapshot {
    let (interface, local_ip, vpn_active, vpn_ip, rx_bytes, tx_bytes) = parse_netstat_ibn(net_raw);

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

    let (latency_ms, jitter_ms, packet_loss_percent) = cached_ping();
    let top_net_apps = collect_top_net_apps();
    let wifi = collect_wifi_diagnostics();
    let tailscale = collect_tailscale_status();
    let ssh_tunnels = collect_ssh_tunnels();
    let custom_hosts = read_custom_hosts();

    let gb = 1024.0 * 1024.0 * 1024.0;
    NetworkSnapshot {
        interface,
        local_ip,
        vpn_active,
        vpn_ip,
        rx_bytes_per_sec: rx_rate.round(),
        tx_bytes_per_sec: tx_rate.round(),
        total_rx_gb: round2(rx_bytes as f64 / gb),
        total_tx_gb: round2(tx_bytes as f64 / gb),
        latency_ms,
        jitter_ms,
        packet_loss_percent,
        top_net_apps,
        wifi,
        tailscale,
        ssh_tunnels,
        custom_hosts,
    }
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
        let app_name = canonical_process_name(normalized);
        let bytes_in = cols[1].trim().parse::<u64>().unwrap_or(0);
        let bytes_out = cols[2].trim().parse::<u64>().unwrap_or(0);
        let entry = totals.entry(app_name).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(bytes_in);
        entry.1 = entry.1.saturating_add(bytes_out);
    }
    totals
}

fn collect_top_net_apps() -> Vec<AppNetworkUsage> {
    get_or_refresh_swr(&TOP_NET_APPS_CACHE, Duration::from_secs(3), || {
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
    get_or_refresh_swr(&PING_CACHE, Duration::from_secs(20), || {
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
    let interface = val["interface_name"]
        .as_str()
        .unwrap_or("en0")
        .to_string();

    Ok(SpeedTestResult {
        download_mbps: round1(dl_bps / 1_000_000.0),
        upload_mbps: round1(ul_bps / 1_000_000.0),
        responsiveness_rpm,
        interface,
    })
}

pub fn collect_devices_snapshot() -> DevicesSnapshot {
    let bt_raw =
        cmd_stdout("/usr/sbin/system_profiler", &["SPBluetoothDataType", "-json"]).unwrap_or_default();
    let usb_raw = cmd_stdout("/usr/sbin/ioreg", &["-p", "IOUSB", "-w0"]).unwrap_or_default();
    let bat_raw =
        cmd_stdout("/usr/sbin/ioreg", &["-rw0", "-c", "AppleSmartBattery"]).unwrap_or_default();

    DevicesSnapshot {
        bluetooth: parse_bluetooth_profiler_json(&bt_raw),
        usb: parse_ioreg_usb_tree(&usb_raw),
        charger: parse_ioreg_adapter_details(&bat_raw),
        battery_health: parse_battery_health_vitals(&bat_raw),
    }
}

pub fn parse_ioreg_usb_tree(raw: &str) -> Vec<UsbDeviceItem> {
    let mut devices = Vec::new();
    for line in raw.lines() {
        if !line.contains("<class IOUSBHostDevice") {
            continue;
        }
        let Some(pos) = line.find("+-o ") else {
            continue;
        };
        let depth = (pos / 2) as u8;
        let after_marker = &line[pos + 4..];
        let raw_name = after_marker
            .split('<')
            .next()
            .unwrap_or(after_marker)
            .trim();
        let name = raw_name
            .split('@')
            .next()
            .unwrap_or(raw_name)
            .trim()
            .to_string();
        if !name.is_empty() {
            devices.push(UsbDeviceItem { name, depth });
        }
    }
    devices
}

pub fn parse_ioreg_adapter_details(raw: &str) -> ChargerAdapterInfo {
    for line in raw.lines() {
        if !line.contains("\"AdapterDetails\"") {
            continue;
        }
        let watts = extract_ioreg_u32(line, "\"Watts\"=");
        let voltage_mv = extract_ioreg_u32(line, "\"AdapterVoltage\"=");
        let current_ma = extract_ioreg_u32(line, "\"Current\"=");
        let description = line
            .split_once("\"Description\"=\"")
            .and_then(|(_, rest)| rest.split('"').next())
            .map(|s| s.to_string());
        return ChargerAdapterInfo {
            connected: watts.is_some(),
            watts,
            voltage_v: voltage_mv.map(|mv| round1(mv as f64 / 1000.0)),
            current_a: current_ma.map(|ma| round1(ma as f64 / 1000.0)),
            description,
        };
    }
    ChargerAdapterInfo {
        connected: false,
        watts: None,
        voltage_v: None,
        current_a: None,
        description: None,
    }
}

fn extract_ioreg_u32(line: &str, key: &str) -> Option<u32> {
    let (_, after) = line.split_once(key)?;
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok()
}

pub fn parse_bluetooth_profiler_json(raw: &str) -> Vec<BluetoothDeviceItem> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(ctrl) = val["SPBluetoothDataType"].as_array().and_then(|a| a.first()) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for (key, connected) in [("device_connected", true), ("device_not_connected", false)] {
        if let Some(arr) = ctrl[key].as_array() {
            for entry in arr {
                if let Some(obj) = entry.as_object() {
                    for (name, details) in obj {
                        let device_type = details["device_minorType"]
                            .as_str()
                            .unwrap_or("Bluetooth")
                            .to_string();
                        let battery_percent = details["device_batteryLevelMain"]
                            .as_str()
                            .or_else(|| details["device_batteryLevelLeft"].as_str())
                            .and_then(|s| s.trim_end_matches('%').trim().parse::<u8>().ok());
                        let rssi = details["device_rssi"]
                            .as_str()
                            .and_then(|s| s.trim().parse::<i32>().ok());
                        items.push(BluetoothDeviceItem {
                            name: name.clone(),
                            device_type,
                            connected,
                            battery_percent,
                            rssi,
                        });
                    }
                }
            }
        }
    }
    items
}

pub fn parse_osascript_volume(raw: &str) -> (u8, u8, bool) {
    let mut out_vol = 50_u8;
    let mut in_vol = 50_u8;
    let mut muted = false;

    for part in raw.split(',') {
        let trimmed = part.trim();
        if let Some(v) = trimmed.strip_prefix("output volume:") {
            out_vol = v.trim().parse::<u8>().unwrap_or(50).min(100);
        } else if let Some(v) = trimmed.strip_prefix("input volume:") {
            in_vol = v.trim().parse::<u8>().unwrap_or(50).min(100);
        } else if let Some(v) = trimmed.strip_prefix("output muted:") {
            muted = v.trim() == "true";
        }
    }
    (out_vol, in_vol, muted)
}

fn detect_mic_user(ps_raw: &str) -> Option<String> {
    let pmset_raw = cmd_stdout("/usr/bin/pmset", &["-g", "assertions"]).unwrap_or_default();
    let rows = parse_ps_rows(ps_raw);
    let mut in_mic_block = false;

    for line in pmset_raw.lines() {
        let trimmed = line.trim();
        if trimmed.contains("BuiltInMicrophoneDevice") || trimmed.contains("audio-in") {
            in_mic_block = true;
        }
        if in_mic_block {
            if let Some((_, after)) = trimmed.split_once("Created for PID:") {
                let digits: String = after
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(pid) = digits.parse::<u32>() {
                    if let Some(row) = rows.iter().find(|r| r.pid == pid) {
                        return Some(canonical_process_name(&row.comm));
                    }
                    return Some(format!("PID {pid}"));
                }
                in_mic_block = false;
            }
            if trimmed.starts_with("pid ") {
                in_mic_block = false;
            }
        }
    }

    let _ = awake::parse_pmset_assertions(&pmset_raw);
    None
}

fn detect_camera_in_use(ps_raw: &str) -> bool {
    if let Some(cam_raw) = cmd_stdout("/usr/sbin/ioreg", &["-r", "-c", "AppleH13CamIn", "-w0"]) {
        if cam_raw.contains("\"FrontCameraStreaming\" = Yes")
            || cam_raw.contains("\"FrontCameraActive\" = Yes")
        {
            return true;
        }
    }
    for row in parse_ps_rows(ps_raw) {
        let lower = row.comm.to_ascii_lowercase();
        if (lower.contains("vdcassistant") || lower.contains("applecameraassistant"))
            && row.pcpu >= 0.8
        {
            return true;
        }
    }
    false
}

fn collect_av_privacy(ps_raw: &str) -> (Option<String>, bool) {
    let ps_owned = ps_raw.to_string();
    get_or_refresh_swr(&AV_PRIVACY_CACHE, Duration::from_secs(5), move || {
        (detect_mic_user(&ps_owned), detect_camera_in_use(&ps_owned))
    })
}

fn read_audio_volume_uncached() -> (u8, u8, bool) {
    let vol_raw = cmd_stdout("/usr/bin/osascript", &["-e", "get volume settings"]).unwrap_or_default();
    parse_osascript_volume(&vol_raw)
}

fn collect_audio_snapshot(ps_raw: &str) -> AudioSnapshot {
    let (output_volume, input_volume, output_muted) =
        get_or_refresh_swr(&AUDIO_VOLUME_CACHE, Duration::from_secs(10), || {
            Some(read_audio_volume_uncached())
        })
        .unwrap_or((50, 50, false));

    let (mic_in_use_by, camera_in_use) = collect_av_privacy(ps_raw);
    AudioSnapshot {
        output_volume,
        input_volume,
        output_muted,
        mic_in_use_by,
        camera_in_use,
    }
}

pub fn is_camera_or_mic_active() -> bool {
    let lock = AV_PRIVACY_CACHE.get_or_init(|| Mutex::new(SwrEntry::default()));
    let needs_refresh = lock
        .lock()
        .ok()
        .map(|g| g.updated_at.map(|ts| ts.elapsed() >= Duration::from_secs(5)).unwrap_or(true))
        .unwrap_or(true);
    let ps_raw = if needs_refresh {
        cmd_stdout("/bin/ps", &["-axo", "pid=,ppid=,etime=,%cpu=,rss=,comm="]).unwrap_or_default()
    } else {
        String::new()
    };
    let (mic, cam) = collect_av_privacy(&ps_raw);
    cam || mic.is_some()
}

pub fn set_audio_volume(
    output_volume: Option<u8>,
    input_volume: Option<u8>,
    muted: Option<bool>,
) -> Result<AudioSnapshot, String> {
    let mut cmds = Vec::new();
    if let Some(out) = output_volume {
        cmds.push(format!("set volume output volume {}", out.min(100)));
    }
    if let Some(inp) = input_volume {
        cmds.push(format!("set volume input volume {}", inp.min(100)));
    }
    if let Some(m) = muted {
        cmds.push(format!("set volume output muted {}", if m { "true" } else { "false" }));
    }
    if !cmds.is_empty() {
        let script = cmds.join("\n");
        let status = Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| format!("Failed to update volume: {e}"))?;
        if !status.success() {
            return Err("osascript volume update failed".into());
        }
    }
    let fresh_vol = read_audio_volume_uncached();
    let vol_cache = AUDIO_VOLUME_CACHE.get_or_init(|| Mutex::new(SwrEntry::default()));
    if let Ok(mut guard) = vol_cache.lock() {
        guard.value = Some(fresh_vol);
        guard.updated_at = Some(Instant::now());
        guard.in_flight = false;
    }
    if let Some(cache) = SYSTEM_EXTRAS_CACHE.get() {
        if let Ok(mut guard) = cache.lock() {
            *guard = None;
        }
    }
    let ps_raw = cmd_stdout(
        "/bin/ps",
        &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="],
    )
    .unwrap_or_default();
    let (mic_in_use_by, camera_in_use) = collect_av_privacy(&ps_raw);
    Ok(AudioSnapshot {
        output_volume: fresh_vol.0,
        input_volume: fresh_vol.1,
        output_muted: fresh_vol.2,
        mic_in_use_by,
        camera_in_use,
    })
}

const ORPHAN_DEV_COMMANDS: &[&str] = &[
    "node",
    "vite",
    "next-server",
    "next",
    "tsc",
    "tsserver",
    "vitest",
    "jest",
    "esbuild",
    "webpack",
    "turbopack",
    "prisma",
    "bun",
    "deno",
    "uvicorn",
    "gunicorn",
    "flask",
    "rails",
    "cargo",
    "rust-analyzer",
];

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
        let exe_name = Path::new(&row.comm)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&row.comm)
            .to_ascii_lowercase();
        if ORPHAN_DEV_COMMANDS.iter().any(|k| exe_name == *k) {
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
    Ok(())
}

pub fn cleanup_orphaned_dev_servers() -> Result<u32, String> {
    let ps_raw = cmd_stdout(
        "/bin/ps",
        &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="],
    )
    .unwrap_or_default();
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
            if val.contains("Developer ID") || val.contains("Apple Mac OS") || val.contains("Apple Root CA") {
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
        &["-p", &pid.to_string(), "-o", "ppid=,user=,etime=,time=,flags=,comm="],
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

    let cwd = cmd_stdout("/usr/sbin/lsof", &["-a", "-d", "cwd", "-Fn", "-p", &pid.to_string()])
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

pub fn parse_pmset_thermal_pressure(raw: &str) -> String {
    for line in raw.lines() {
        if let Some((_, val)) = line.split_once("CPU_Speed_Limit") {
            if let Some(num) = val
                .split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
                .and_then(|s| s.parse::<u32>().ok())
            {
                return if num < 60 {
                    "critical".into()
                } else if num < 85 {
                    "heavy".into()
                } else if num < 100 {
                    "moderate".into()
                } else {
                    "nominal".into()
                };
            }
        }
    }
    "nominal".into()
}

fn cached_thermal_pressure() -> String {
    get_or_refresh_swr(&THERMAL_PRESSURE_CACHE, Duration::from_secs(6), || {
        let therm_raw = cmd_stdout("/usr/bin/pmset", &["-g", "therm"]).unwrap_or_default();
        Some(parse_pmset_thermal_pressure(&therm_raw))
    })
    .unwrap_or_else(|| "nominal".into())
}

fn collect_silicon_power_and_pressure(ps_raw: &str) -> (SiliconPowerBreakdown, String) {
    let pressure = cached_thermal_pressure();

    let package_watts = crate::smc::get_system_power_watts().unwrap_or(0.0);
    if package_watts <= 0.1 {
        return (SiliconPowerBreakdown::default(), pressure);
    }

    let total_pcpu: f64 = parse_ps_rows(ps_raw).into_iter().map(|r| r.pcpu).sum();
    let pcpu_frac = (total_pcpu / 800.0).clamp(0.15, 0.72);
    let smc_cpu = crate::smc::read_smc_key("PCPC")
        .map(f64::from)
        .filter(|w| *w > 0.05 && *w <= package_watts);
    let smc_gpu = crate::smc::read_smc_key("PCPG")
        .map(f64::from)
        .filter(|w| *w > 0.05 && *w <= package_watts);

    let cpu_watts = round1(smc_cpu.unwrap_or(package_watts * pcpu_frac));
    let remaining = (package_watts - cpu_watts).max(0.0);
    let gpu_watts = round1(smc_gpu.unwrap_or(remaining * 0.48));
    let after_gpu = (remaining - gpu_watts).max(0.0);
    let dram_watts = round1(after_gpu * 0.68);
    let ane_watts = round1((after_gpu - dram_watts).max(0.0));

    (
        SiliconPowerBreakdown {
            cpu_watts,
            gpu_watts,
            ane_watts,
            dram_watts,
            package_watts: round1(package_watts),
        },
        pressure,
    )
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
            if let Some(pct) = val.trim().trim_end_matches('%').parse::<u8>().ok() {
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

fn collect_ssd_health() -> SsdHealthMetrics {
    get_or_refresh_swr(&SSD_HEALTH_CACHE, Duration::from_secs(180), || {
        let nvme_json =
            cmd_stdout("/usr/sbin/system_profiler", &["SPNVMeDataType", "-json"]).unwrap_or_default();
        let smartctl_raw = ["/opt/homebrew/bin/smartctl", "/usr/local/bin/smartctl"]
            .iter()
            .find(|p| Path::new(p).exists())
            .and_then(|bin| cmd_stdout(bin, &["-A", "/dev/disk0"]))
            .unwrap_or_default();
        parse_ssd_smart_json(&nvme_json, &smartctl_raw)
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
        let processor = cols.get(4..6).map(|s| s.join(" ")).unwrap_or_else(|| "GPU".into());
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
        });
    }
    items
}

fn find_ollama_bin() -> Option<&'static str> {
    ["/opt/homebrew/bin/ollama", "/usr/local/bin/ollama", "ollama"]
        .into_iter()
        .find(|p| !p.starts_with('/') || Path::new(p).exists())
}

fn collect_ollama_models_uncached() -> Vec<OllamaModelItem> {
    let addr = "127.0.0.1:11434".parse().unwrap();
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(35)).is_ok() {
        find_ollama_bin()
            .and_then(|bin| cmd_stdout(bin, &["ps"]))
            .map(|raw| parse_ollama_ps(&raw))
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn collect_ollama_models() -> Vec<OllamaModelItem> {
    get_or_refresh_swr(
        &OLLAMA_CACHE,
        Duration::from_secs(6),
        collect_ollama_models_uncached,
    )
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
    let cache = OLLAMA_CACHE.get_or_init(|| Mutex::new(SwrEntry::default()));
    if let Ok(mut guard) = cache.lock() {
        guard.value = fresh.clone();
        guard.updated_at = Some(Instant::now());
        guard.in_flight = false;
    }
    if let Some(c) = SYSTEM_EXTRAS_CACHE.get() {
        if let Ok(mut guard) = c.lock() {
            *guard = None;
        }
    }
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

fn collect_containers() -> Vec<ContainerItem> {
    get_or_refresh_swr(
        &CONTAINER_CACHE,
        Duration::from_secs(6),
        collect_containers_uncached,
    )
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
    let cache = CONTAINER_CACHE.get_or_init(|| Mutex::new(SwrEntry::default()));
    if let Ok(mut guard) = cache.lock() {
        guard.value = fresh.clone();
        guard.updated_at = Some(Instant::now());
        guard.in_flight = false;
    }
    if let Some(c) = SYSTEM_EXTRAS_CACHE.get() {
        if let Ok(mut guard) = c.lock() {
            *guard = None;
        }
    }
    Ok(fresh)
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
            || (!prefix.is_empty() && target_lower.starts_with(&cmd_clean));
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
        if seen.insert((protocol.clone(), local_endpoint.clone(), remote_endpoint.clone())) {
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
    let raw = cmd_stdout("/usr/sbin/lsof", &["-i", "-n", "-P"]).unwrap_or_default();
    Ok(parse_lsof_connections(&raw, process_name))
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

pub fn count_apfs_snapshots() -> u32 {
    get_or_refresh_swr(&APFS_SNAPSHOT_CACHE, Duration::from_secs(60), || {
        cmd_stdout("/usr/bin/tmutil", &["listlocalsnapshots", "/"])
            .map(|raw| {
                raw.lines()
                    .filter(|l| l.trim().starts_with("com.apple.TimeMachine."))
                    .count() as u32
            })
            .unwrap_or(0)
    })
}

pub fn parse_battery_health_vitals(raw: &str) -> BatteryHealthVitals {
    let mut design = None;
    let mut nominal = None;
    let mut raw_max = None;
    let mut voltage_mv = None;
    let mut amperage_ma = None;
    let mut time_rem = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        let clean = trimmed.trim_start_matches('|').trim();
        if design.is_none() {
            design = extract_ioreg_u32(trimmed, "\"DesignCapacity\"=");
        }
        if nominal.is_none() {
            nominal = extract_ioreg_u32(trimmed, "\"NominalChargeCapacity\"=");
        }
        if raw_max.is_none() {
            raw_max = extract_ioreg_u32(trimmed, "\"AppleRawMaxCapacity\"=");
        }
        if voltage_mv.is_none() && clean.starts_with("\"Voltage\" =") {
            voltage_mv = clean
                .split('=')
                .nth(1)
                .and_then(|s| s.trim().parse::<u32>().ok());
        }
        if amperage_ma.is_none() && clean.starts_with("\"InstantAmperage\" =") {
            amperage_ma = clean.split('=').nth(1).and_then(|s| {
                let t = s.trim();
                t.parse::<i64>()
                    .ok()
                    .or_else(|| t.parse::<u64>().ok().map(|u| u as i64))
                    .map(|v| v as i32)
            });
        }
        if time_rem.is_none() && clean.starts_with("\"TimeRemaining\" =") {
            time_rem = clean
                .split('=')
                .nth(1)
                .and_then(|s| s.trim().parse::<u32>().ok())
                .filter(|&m| m > 0 && m < 65535);
        }
    }

    let current_cap = raw_max.or(nominal);
    let health_percent = match (current_cap, design) {
        (Some(cur), Some(des)) if des > 0 => {
            Some(((cur as f64 / des as f64) * 100.0).round().clamp(1.0, 100.0) as u8)
        }
        _ => None,
    };

    BatteryHealthVitals {
        design_capacity_mah: design,
        nominal_capacity_mah: nominal,
        raw_max_capacity_mah: raw_max,
        health_percent,
        voltage_v: voltage_mv.map(|mv| round1(mv as f64 / 1000.0)),
        amperage_ma,
        time_remaining_min: time_rem,
    }
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
    get_or_refresh_swr(&WIFI_CACHE, Duration::from_secs(45), || {
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
                let name = peer["HostName"].as_str().unwrap_or("peer");
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
    online_peers.truncate(6);
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
    get_or_refresh_swr(&TAILSCALE_CACHE, Duration::from_secs(25), || {
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
        if !cmd0.ends_with("ssh") && *cmd0 != "ssh" {
            continue;
        }
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
            } else if !t.starts_with('-') && t != &spec {
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
    }
    tunnels
}

fn collect_ssh_tunnels() -> Vec<SshTunnelItem> {
    get_or_refresh_swr(&SSH_TUNNELS_CACHE, Duration::from_secs(10), || {
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
    let cache = CUSTOM_HOSTS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((ts, cached)) = guard.as_ref() {
            if ts.elapsed() < Duration::from_secs(30) {
                return cached.clone();
            }
        }
    }
    let hosts = std::fs::read_to_string("/etc/hosts")
        .map(|s| parse_etc_hosts(&s))
        .unwrap_or_default();
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), hosts.clone()));
    }
    hosts
}

fn collect_launch_agents() -> Vec<LaunchAgentItem> {
    get_or_refresh_swr(
        &LAUNCH_AGENTS_CACHE,
        Duration::from_secs(120),
        collect_launch_agents_uncached,
    )
}

fn collect_launch_agents_uncached() -> Vec<LaunchAgentItem> {
    let mut dirs = vec![("/Library/LaunchAgents".to_string(), "System")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.insert(
            0,
            (home.join("Library/LaunchAgents").to_string_lossy().into_owned(), "User"),
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
            let label = cmd_stdout("/usr/bin/plutil", &["-extract", "Label", "raw", &plist_path])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    p.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                });
            let program =
                cmd_stdout("/usr/bin/plutil", &["-extract", "ProgramArguments.0", "raw", &plist_path])
                    .or_else(|| {
                        cmd_stdout("/usr/bin/plutil", &["-extract", "Program", "raw", &plist_path])
                    })
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

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

    items.sort_by(|a, b| a.binary_exists.cmp(&b.binary_exists).then(a.signed.cmp(&b.signed)).then(a.label.cmp(&b.label)));
    items
}

pub fn read_quick_switch_state() -> QuickSwitchState {
    get_or_refresh_swr(
        &QUICK_SWITCH_CACHE,
        Duration::from_secs(15),
        read_quick_switch_state_uncached,
    )
}

fn read_quick_switch_state_uncached() -> QuickSwitchState {
    let dark_mode = cmd_stdout("/usr/bin/defaults", &["read", "-g", "AppleInterfaceStyle"])
        .map(|s| s.trim().eq_ignore_ascii_case("Dark"))
        .unwrap_or(false);
    let show_hidden_files =
        cmd_stdout("/usr/bin/defaults", &["read", "com.apple.finder", "AppleShowAllFiles"])
            .map(|s| matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
    let show_desktop_icons =
        cmd_stdout("/usr/bin/defaults", &["read", "com.apple.finder", "CreateDesktop"])
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
            format!("Switched appearance to {} Mode.", if curr.dark_mode { "Light" } else { "Dark" })
        }
        "toggle_hidden_files" => {
            let next = if curr.show_hidden_files { "FALSE" } else { "TRUE" };
            let _ = Command::new("/usr/bin/defaults")
                .args(["write", "com.apple.finder", "AppleShowAllFiles", "-bool", next])
                .status();
            let _ = Command::new("/usr/bin/killall").arg("Finder").status();
            format!(
                "{} hidden Finder dotfiles.",
                if curr.show_hidden_files { "Hid" } else { "Revealed" }
            )
        }
        "toggle_desktop_icons" => {
            let next = if curr.show_desktop_icons { "FALSE" } else { "TRUE" };
            let _ = Command::new("/usr/bin/defaults")
                .args(["write", "com.apple.finder", "CreateDesktop", "-bool", next])
                .status();
            let _ = Command::new("/usr/bin/killall").arg("Finder").status();
            format!(
                "{} Desktop icons.",
                if curr.show_desktop_icons { "Hid" } else { "Restored" }
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
    let cache = QUICK_SWITCH_CACHE.get_or_init(|| Mutex::new(SwrEntry::default()));
    if let Ok(mut guard) = cache.lock() {
        guard.value = fresh.clone();
        guard.updated_at = Some(Instant::now());
        guard.in_flight = false;
    }
    if let Some(c) = SYSTEM_EXTRAS_CACHE.get() {
        if let Ok(mut guard) = c.lock() {
            *guard = None;
        }
    }
    Ok((fresh, msg))
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
    fn parses_vm_stat_and_swap_accurately() {
        let vm_stat = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                   259046.
Pages active:                                 980096.
Pages wired down:                            1789948.
Pages occupied by compressor:                 131072.
";
        let swap = "total = 2048.00M  used = 512.00M  free = 1536.00M  (encrypted)";
        let mem = parse_vm_stat_and_mem(68_719_476_736, vm_stat, swap);
        assert_eq!(mem.total_gb, 64.0);
        assert_eq!(mem.app_gb, 15.0);
        assert_eq!(mem.wired_gb, 27.3);
        assert_eq!(mem.compressed_gb, 2.0);
        assert_eq!(mem.swap_used_mb, 512.0);
        assert_eq!(mem.pressure_level, "ok");
    }

    #[test]
    fn parses_df_and_netstat_and_ping() {
        let df = "\
Filesystem     1024-blocks      Used Available Capacity iused ifree %iused Mounted on
/dev/disk3s1s1   482797652  13433144 114421544    11% 486811 1144215440 0% /
";
        let disk = parse_df_root(df);
        assert!(disk.total_gb > 490.0);
        assert!(disk.free_gb > 110.0);

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
    fn parses_usb_charger_agents_and_volume() {
        let usb = "\
+-o Root  <class IORegistryEntry>
  | +-o USB2.0 Hub@02100000  <class IOUSBHostDevice, id 0x1>
  | | +-o Gaming Keyboard@02132000  <class IOUSBHostDevice, id 0x2>
";
        let usb_items = parse_ioreg_usb_tree(usb);
        assert_eq!(usb_items.len(), 2);
        assert_eq!(usb_items[0].name, "USB2.0 Hub");
        assert_eq!(usb_items[1].name, "Gaming Keyboard");

        let bat = r#"  |   "AdapterDetails" = {"AdapterVoltage"=20000,"Watts"=100,"Current"=5000,"Description"="pd charger"}"#;
        let charger = parse_ioreg_adapter_details(bat);
        assert!(charger.connected);
        assert_eq!(charger.watts, Some(100));
        assert_eq!(charger.voltage_v, Some(20.0));
        assert_eq!(charger.current_a, Some(5.0));

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

        assert_eq!(
            parse_osascript_volume("output volume:85, input volume:60, alert volume:52, output muted:false"),
            (85, 60, false)
        );
    }

    #[test]
    fn parses_nettop_csv_orphans_and_codesign() {
        let nettop = "\
,bytes_in,bytes_out,
apsd.591,1000,200,
Cursor Helper (Renderer).112,5000,3000,
Cursor.100,2000,1000,
";
        let totals = parse_nettop_csv(nettop);
        assert_eq!(totals.get("apsd").copied(), Some((1000, 200)));
        assert_eq!(totals.get("Cursor").copied(), Some((7000, 4000)));

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
    }

    #[test]
    fn parses_nvme_ollama_docker_thermal_and_lsof_connections() {
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

        assert_eq!(parse_pmset_thermal_pressure("Note: No thermal warning level has been recorded"), "nominal");
        assert_eq!(parse_pmset_thermal_pressure("CPU_Speed_Limit = 72"), "heavy");

        let lsof_net = "\
COMMAND   PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
Cursor  500 me 12u IPv4 0x1 0t0 TCP 192.168.1.5:51234->104.18.2.1:443 (ESTABLISHED)
node    501 me 14u IPv4 0x2 0t0 TCP 127.0.0.1:3000 (LISTEN)
";
        let conns = parse_lsof_connections(lsof_net, "Cursor");
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].remote_endpoint, "104.18.2.1:443");
        assert_eq!(conns[0].state, "ESTABLISHED");
    }

    #[test]
    fn parses_battery_health_wifi_tailscale_ssh_hosts_and_http_title() {
        let bat_raw = r#"
  |   "Voltage" = 12604
  |   "BatteryData" = {"DesignCapacity"=8694,"NominalChargeCapacity"=7815,"AppleRawMaxCapacity"=7571}
  |   "InstantAmperage" = -1420
  |   "TimeRemaining" = 185
"#;
        let bh = parse_battery_health_vitals(bat_raw);
        assert_eq!(bh.design_capacity_mah, Some(8694));
        assert_eq!(bh.raw_max_capacity_mah, Some(7571));
        assert_eq!(bh.health_percent, Some(87));
        assert_eq!(bh.voltage_v, Some(12.6));
        assert_eq!(bh.amperage_ma, Some(-1420));
        assert_eq!(bh.time_remaining_min, Some(185));

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
            "Peer": { "k1": { "HostName": "prod-db", "Online": true, "TailscaleIPs": ["100.64.0.9"] } }
        }"#;
        let ts = parse_tailscale_json(ts_json);
        assert!(ts.running);
        assert_eq!(ts.tailscale_ip.as_deref(), Some("100.93.250.37"));
        assert_eq!(ts.online_peers, vec!["prod-db (100.64.0.9)"]);

        let ssh_raw = "41200 /usr/bin/ssh -N -L 5432:127.0.0.1:5432 bastion\n41201 /usr/bin/zsh\n";
        let tunnels = parse_ssh_tunnels(ssh_raw);
        assert_eq!(tunnels.len(), 1);
        assert_eq!(tunnels[0].pid, 41200);
        assert_eq!(tunnels[0].spec, "5432:127.0.0.1:5432");

        let hosts = parse_etc_hosts("127.0.0.1 localhost app.local\n192.168.1.50 nas.home\n");
        assert_eq!(hosts, vec!["app.local → 127.0.0.1", "nas.home → 192.168.1.50"]);

        let (code, title) = parse_http_response_title("HTTP/1.1 200 OK\r\n\r\n<html><head><title>Vite App</title></head></html>");
        assert_eq!(code, Some(200));
        assert_eq!(title.as_deref(), Some("Vite App"));
    }

    #[test]
    fn collect_system_extras_completes_within_latency_budget() {
        let start = Instant::now();
        let snap = collect_system_extras();
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 1200,
            "collect_system_extras() took {elapsed_ms}ms, exceeding 1200ms budget"
        );
        assert!(snap.memory.total_gb >= 0.0);
    }

    #[test]
    fn collect_pill_extras_completes_under_150ms() {
        let _ = collect_pill_extras();
        if let Some(cache) = SYSTEM_EXTRAS_CACHE.get() {
            if let Ok(mut guard) = cache.lock() {
                *guard = None;
            }
        }
        let start = Instant::now();
        let snap = collect_pill_extras();
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 150,
            "collect_pill_extras() took {elapsed_ms}ms, exceeding 150ms budget"
        );
        assert!(snap.memory.total_gb > 0.0);
    }
}
