mod audio;
mod cache_cleanup;
#[cfg(test)]
mod cache_cost_fixture;
mod developer_tools;
mod devices;
mod network;
mod storage;

use crate::ports;
use crate::proc_info::{cmd_stdout, round1};
use developer_tools::{AiTokenUsageSummary, LaunchAgentItem, OrphanedProcessItem};
use devices::SiliconPowerBreakdown;
use network::{NetworkSnapshot, TailscaleStatus, WifiDiagnostics};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};
use storage::{count_apfs_snapshots, sample_root_disk_metrics, DiskMetrics, SsdHealthMetrics};
use ts_rs::TS;

pub use audio::{
    control_media_playback, is_camera_or_mic_active, list_audio_devices, set_audio_device,
    set_audio_volume, toggle_mic_mute, AudioSnapshot, NowPlayingItem,
};
pub use developer_tools::{
    ai_agent_active, cached_containers_snapshot, cleanup_orphaned_dev_servers, collect_containers,
    control_container, detect_ai_agents_fast, inspect_process_details, list_user_scripts,
    open_in_editor, parse_container_port_map, reveal_in_finder, run_maintenance_action,
    run_quick_switch, run_user_script, set_process_paused, terminate_user_process,
    unload_ollama_model, AgentSessionItem, ContainerItem, OllamaModelItem, ProcessInspection,
    QuickSwitchState, UserScriptCommand,
};
pub use devices::{collect_devices_snapshot, DevicesSnapshot};
pub use network::{
    list_app_connections, probe_localhost_ports, run_network_quality, AppConnectionItem,
    PortHttpProbe, SpeedTestResult,
};
pub use storage::{
    clean_rebuildable_cache, clean_stale_rebuildable_caches, run_nvme_benchmark,
    set_workspace_roots, DiskBenchmarkResult, RebuildableCacheItem,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct SelfCostMetrics {
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
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
    #[serde(default)]
    pub ai_token_usage: AiTokenUsageSummary,
}

static MEMSIZE_BYTES_CACHE: OnceLock<u64> = OnceLock::new();

fn physical_memory_bytes() -> u64 {
    *MEMSIZE_BYTES_CACHE.get_or_init(|| {
        native_physical_memory_bytes().unwrap_or_else(|| {
            cmd_stdout("/usr/sbin/sysctl", &["-n", "hw.memsize"])
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(0)
        })
    })
}

#[cfg(target_os = "macos")]
fn native_physical_memory_bytes() -> Option<u64> {
    let mut memsize: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            &mut memsize as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 && memsize > 0 {
        Some(memsize)
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
fn native_physical_memory_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn native_swap_used_mb() -> Option<f64> {
    #[repr(C)]
    struct NativeXswUsage {
        xsu_total: u64,
        xsu_avail: u64,
        xsu_used: u64,
        xsu_pagesize: u32,
        xsu_encrypted: bool,
    }
    let mut xsw = std::mem::MaybeUninit::<NativeXswUsage>::zeroed();
    let mut len = std::mem::size_of::<NativeXswUsage>();
    let rc = unsafe {
        libc::sysctlbyname(
            c"vm.swapusage".as_ptr(),
            xsw.as_mut_ptr() as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 && len >= std::mem::size_of::<NativeXswUsage>() {
        let xsw = unsafe { xsw.assume_init() };
        Some(xsw.xsu_used as f64 / (1024.0 * 1024.0))
    } else {
        None
    }
}

fn build_memory_metrics(
    memsize_bytes: u64,
    active_bytes: u64,
    wired_bytes: u64,
    compressed_bytes: u64,
    swap_used_mb: f64,
    (page_ins_per_sec, swap_outs_per_sec): (f64, f64),
) -> MemoryMetrics {
    let gb = 1024.0 * 1024.0 * 1024.0;
    let total_gb = round1(memsize_bytes as f64 / gb);
    let app_gb = round1(active_bytes as f64 / gb);
    let wired_gb = round1(wired_bytes as f64 / gb);
    let compressed_gb = round1(compressed_bytes as f64 / gb);
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
        page_ins_per_sec,
        swap_outs_per_sec,
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn native_sample_memory(memsize_bytes: u64) -> Option<MemoryMetrics> {
    let host = unsafe { libc::mach_host_self() };
    let ps = unsafe { libc::vm_page_size as u64 };
    if ps == 0 {
        return None;
    }
    let mut stat = std::mem::MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let rc = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            stat.as_mut_ptr() as libc::host_info64_t,
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let swap_used_mb = native_swap_used_mb()?;
    let rates = compute_vm_paging_rates(stat.pageins, stat.swapouts);
    Some(build_memory_metrics(
        memsize_bytes,
        stat.active_count as u64 * ps,
        stat.wire_count as u64 * ps,
        stat.compressor_page_count as u64 * ps,
        swap_used_mb,
        rates,
    ))
}

#[cfg(not(target_os = "macos"))]
fn native_sample_memory(_memsize_bytes: u64) -> Option<MemoryMetrics> {
    None
}

fn sample_memory(vm_stat_raw: &str, swap_raw: &str) -> MemoryMetrics {
    let (mut memory, pageins, swapouts) =
        parse_vm_stat_with_paging(physical_memory_bytes(), vm_stat_raw, swap_raw);
    let (pi_rate, so_rate) = compute_vm_paging_rates(pageins, swapouts);
    memory.page_ins_per_sec = pi_rate;
    memory.swap_outs_per_sec = so_rate;
    memory
}

// One coalescing cache for both collectors: a single mutex serializes the
// lookup, the collection, and the publish, so concurrent callers can never
// run two cold collectors at once. A Pill entry never satisfies a Full
// request; a Full entry satisfies either.
#[derive(Clone, Copy, PartialEq)]
enum ExtrasScope {
    Pill,
    Full,
}

pub(super) struct ExtrasCache {
    // Serializes collectors so concurrent callers coalesce into one fetch.
    // `entry` is never held while a fetch runs, so readers such as
    // `latest_cached_snapshot` never wait behind a collection.
    fetch_gate: Mutex<()>,
    entry: RwLock<Option<(Instant, ExtrasScope, SystemExtrasSnapshot)>>,
    // Bumped by `invalidate`; a fetch that straddles an invalidation returns
    // its result but does not publish it.
    generation: AtomicU64,
}

impl ExtrasCache {
    const TTL: Duration = Duration::from_millis(1500);

    const fn new() -> Self {
        Self {
            fetch_gate: Mutex::new(()),
            entry: RwLock::new(None),
            generation: AtomicU64::new(0),
        }
    }

    fn fresh(&self, scope: ExtrasScope) -> Option<SystemExtrasSnapshot> {
        let guard = self.entry.read().unwrap_or_else(|e| e.into_inner());
        let (ts, saved_scope, cached) = guard.as_ref()?;
        (ts.elapsed() < Self::TTL && (*saved_scope == ExtrasScope::Full || *saved_scope == scope))
            .then(|| cached.clone())
    }

    // Callers queue behind an in-flight collection and reuse its result.
    // Fetch must never call back into this cache.
    fn get(
        &self,
        scope: ExtrasScope,
        fetch: impl FnOnce(Option<&SystemExtrasSnapshot>) -> SystemExtrasSnapshot,
    ) -> SystemExtrasSnapshot {
        if let Some(cached) = self.fresh(scope) {
            return cached;
        }
        let _gate = self.fetch_gate.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = self.fresh(scope) {
            return cached;
        }
        let generation = self.generation.load(AtomicOrdering::SeqCst);
        let prior = self.peek_latest();
        let snapshot = fetch(prior.as_ref());
        let mut entry = self.entry.write().unwrap_or_else(|e| e.into_inner());
        if self.generation.load(AtomicOrdering::SeqCst) == generation {
            *entry = Some((Instant::now(), scope, snapshot.clone()));
        }
        snapshot
    }

    pub(super) fn invalidate(&self) {
        let mut entry = self.entry.write().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, AtomicOrdering::SeqCst);
        *entry = None;
    }

    fn peek_latest(&self) -> Option<SystemExtrasSnapshot> {
        self.entry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(_, _, snapshot)| snapshot.clone())
    }
}

pub(super) static EXTRAS_CACHE: ExtrasCache = ExtrasCache::new();

// Process-table derived pill data (self cost + agent sessions). The pill path
// refreshes it in the background instead of spawning `ps` on every poll.
static PILL_PROC_CACHE: SwrCache<(SelfCostMetrics, Vec<AgentSessionItem>)> = SwrCache::new();

fn sample_pill_processes() -> (SelfCostMetrics, Vec<AgentSessionItem>) {
    let ps = crate::proc_info::shared_ps_snapshot().unwrap_or_default();
    let cached_ports = ports::cached_listening_ports_snapshot();
    (
        parse_self_cost(&ps, std::process::id()),
        detect_ai_agents_fast(&ps, &cached_ports),
    )
}

pub(super) struct SwrCache<T> {
    cell: OnceLock<Mutex<SwrEntry<T>>>,
}

impl<T> SwrCache<T> {
    pub(super) const fn new() -> Self {
        Self {
            cell: OnceLock::new(),
        }
    }
}

impl<T: Clone + Default + Send + 'static> SwrCache<T> {
    pub(super) fn get_or_refresh<F>(&'static self, ttl: Duration, fetcher: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
    {
        get_or_refresh_swr(&self.cell, ttl, fetcher)
    }

    pub(super) fn store(&'static self, value: T) {
        let lock = self.cell.get_or_init(|| Mutex::new(SwrEntry::default()));
        if let Ok(mut guard) = lock.lock() {
            guard.value = value;
            guard.updated_at = Some(Instant::now());
            guard.in_flight = false;
        }
    }

    pub(super) fn peek(&'static self) -> T {
        self.cell
            .get()
            .and_then(|lock| lock.lock().ok())
            .map(|guard| guard.value.clone())
            .unwrap_or_default()
    }
}

#[derive(Default)]
pub(super) struct SwrEntry<T> {
    pub(super) updated_at: Option<Instant>,
    pub(super) in_flight: bool,
    pub(super) value: T,
}

static VM_PAGING_STATE: OnceLock<Mutex<Option<(Instant, u64, u64)>>> = OnceLock::new();

static SWR_BLOCKING_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_swr_blocking_mode(blocking: bool) {
    SWR_BLOCKING_MODE.store(blocking, std::sync::atomic::Ordering::SeqCst);
}

pub(super) fn is_swr_blocking_mode() -> bool {
    SWR_BLOCKING_MODE.load(std::sync::atomic::Ordering::SeqCst)
}

pub(super) fn get_or_refresh_swr<T, F>(
    cell: &'static OnceLock<Mutex<SwrEntry<T>>>,
    ttl: Duration,
    fetcher: F,
) -> T
where
    T: Clone + Default + Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    get_or_refresh_swr_with_mode(
        cell,
        ttl,
        SWR_BLOCKING_MODE.load(std::sync::atomic::Ordering::SeqCst),
        fetcher,
    )
}

fn get_or_refresh_swr_with_mode<T, F>(
    cell: &'static OnceLock<Mutex<SwrEntry<T>>>,
    ttl: Duration,
    blocking: bool,
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
    if !is_stale {
        return current;
    }
    if blocking {
        if guard.in_flight {
            return current;
        }
        guard.in_flight = true;
        drop(guard);
        let fresh = std::panic::catch_unwind(std::panic::AssertUnwindSafe(fetcher));
        if let Ok(mut guard) = lock.lock() {
            guard.in_flight = false;
            if let Ok(val) = fresh {
                guard.value = val.clone();
                guard.updated_at = Some(Instant::now());
                return val;
            }
            return guard.value.clone();
        }
        return current;
    }
    if !guard.in_flight {
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
            pressure_level: "ok".into(),
            page_ins_per_sec: 0.0,
            swap_outs_per_sec: 0.0,
        },
        disk: DiskMetrics {
            total_gb: 0.0,
            used_gb: 0.0,
            free_gb: 0.0,
            used_percent: 0,
            apfs_snapshots_count: 0,
            strict_free_gb: 0.0,
            purgeable_gb: 0.0,
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
            input_muted: false,
            mic_in_use_by: None,
            camera_in_use: false,
            now_playing: None,
            output_devices: Vec::new(),
            input_devices: Vec::new(),
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
        ai_token_usage: AiTokenUsageSummary::default(),
    }
}

pub fn collect_pill_extras() -> SystemExtrasSnapshot {
    EXTRAS_CACHE.get(ExtrasScope::Pill, |prior| {
        collect_pill_extras_uncached(prior)
    })
}

pub fn latest_cached_snapshot() -> Option<SystemExtrasSnapshot> {
    EXTRAS_CACHE.peek_latest()
}

fn collect_pill_extras_uncached(
    base_snapshot: Option<&SystemExtrasSnapshot>,
) -> SystemExtrasSnapshot {
    let memsize_bytes = physical_memory_bytes();

    let mut snap = base_snapshot
        .cloned()
        .unwrap_or_else(|| default_extras_snapshot(memsize_bytes));

    if cfg!(target_os = "macos") {
        snap.memory = native_sample_memory(memsize_bytes).unwrap_or_else(|| {
            let vm = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
            let swap = cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
            sample_memory(&vm, &swap)
        });
        if !network::sample_network_traffic_native(&mut snap.network) {
            let net = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
            network::sample_network_traffic(&net, &mut snap.network);
        }
    } else {
        let vm_stat_raw = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
        let swap_raw = cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
        let net_raw = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
        snap.memory = sample_memory(&vm_stat_raw, &swap_raw);
        network::sample_network_traffic(&net_raw, &mut snap.network);
    }

    let (self_cost, agents) =
        PILL_PROC_CACHE.get_or_refresh(Duration::from_secs(6), sample_pill_processes);
    snap.self_cost = self_cost;
    snap.agents = agents;

    if let Some(pkg_w) = crate::smc::get_system_power_watts() {
        snap.silicon_power.package_watts = round1(pkg_w);
    }

    snap
}

pub fn collect_system_extras() -> SystemExtrasSnapshot {
    EXTRAS_CACHE.get(ExtrasScope::Full, |_| collect_system_extras_uncached())
}

/// Just the fields the Prometheus / CSV exporters read. A fresh full snapshot
/// is reused; otherwise a partial one is collected without touching the shared
/// cache, so partial data can never satisfy a full request.
pub fn collect_metrics_extras(include_agents_and_tokens: bool) -> SystemExtrasSnapshot {
    if let Some(full) = EXTRAS_CACHE.fresh(ExtrasScope::Full) {
        return full;
    }
    let memsize_bytes = physical_memory_bytes();
    let mut snap = default_extras_snapshot(memsize_bytes);
    std::thread::scope(|s| {
        let disk_h = s.spawn(sample_root_disk_metrics);
        let power_h = s.spawn(devices::collect_silicon_power_and_pressure);
        let agents_h = include_agents_and_tokens.then(|| {
            s.spawn(|| {
                let ps = crate::proc_info::shared_ps_snapshot().unwrap_or_default();
                let ports = ports::cached_listening_ports_snapshot();
                detect_ai_agents_fast(&ps, &ports)
            })
        });
        let tokens_h =
            include_agents_and_tokens.then(|| s.spawn(developer_tools::collect_ai_token_usage));

        snap.memory = native_sample_memory(memsize_bytes).unwrap_or_else(|| {
            let vm = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
            let swap = cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
            sample_memory(&vm, &swap)
        });
        if !network::sample_network_traffic_native(&mut snap.network) {
            let net = cmd_stdout("/usr/sbin/netstat", &["-ibn"]).unwrap_or_default();
            network::sample_network_traffic(&net, &mut snap.network);
        }
        if let Ok(disk) = disk_h.join() {
            snap.disk = disk;
        }
        if let Ok((power, pressure)) = power_h.join() {
            snap.silicon_power = power;
            snap.thermal_pressure = pressure;
        }
        if let Some(Ok(agents)) = agents_h.map(|h| h.join()) {
            snap.agents = agents;
        }
        if let Some(Ok(tokens)) = tokens_h.map(|h| h.join()) {
            snap.ai_token_usage = tokens;
        }
    });
    snap
}

/// Token usage, agent sessions, and local models only; skips every unrelated
/// collector `collect_system_extras` runs.
pub fn collect_ai_usage_snapshot() -> (
    developer_tools::AiTokenUsageSummary,
    Vec<AgentSessionItem>,
    Vec<OllamaModelItem>,
) {
    if let Some(full) = EXTRAS_CACHE.fresh(ExtrasScope::Full) {
        return (full.ai_token_usage, full.agents, full.ollama_models);
    }
    std::thread::scope(|s| {
        let tokens_h = s.spawn(developer_tools::collect_ai_token_usage);
        let models_h = s.spawn(developer_tools::collect_ollama_models);
        let ps = crate::proc_info::shared_ps_snapshot().unwrap_or_default();
        let ports = ports::list_listening_ports_fast().unwrap_or_default();
        let agents = developer_tools::detect_ai_agents(&ps, &ports);
        (
            tokens_h.join().unwrap_or_default(),
            agents,
            models_h.join().unwrap_or_default(),
        )
    })
}

fn collect_system_extras_uncached() -> SystemExtrasSnapshot {
    let memsize_bytes = physical_memory_bytes();
    let (native_mem, mut disk, ps_raw, listening_ports) = std::thread::scope(|s| {
        let disk_handle = s.spawn(sample_root_disk_metrics);
        let ps_handle = s.spawn(|| crate::proc_info::shared_ps_snapshot().unwrap_or_default());
        let mem = native_sample_memory(memsize_bytes);
        let ports = ports::list_listening_ports_fast().unwrap_or_default();
        (
            mem,
            disk_handle
                .join()
                .unwrap_or_else(|_| sample_root_disk_metrics()),
            ps_handle.join().unwrap_or_default(),
            ports,
        )
    });

    let memory = native_mem.unwrap_or_else(|| {
        let vm = cmd_stdout("/usr/bin/vm_stat", &[]).unwrap_or_default();
        let swap = cmd_stdout("/usr/sbin/sysctl", &["-n", "vm.swapusage"]).unwrap_or_default();
        sample_memory(&vm, &swap)
    });

    let self_cost = parse_self_cost(&ps_raw, std::process::id());
    // In blocking CLI/MCP mode the SWR collectors run synchronously, so
    // fanning them out bounds the cold path by the slowest probe. In GUI mode
    // each SWR check returns immediately without blocking.
    let (
        (caches, ssd_health, apfs_snapshots_count),
        (agents, orphaned_servers, audio),
        network,
        (silicon_power, thermal_pressure),
        (ollama_models, containers),
        (launch_agents, quick_switches, ai_token_usage),
    ) = if is_swr_blocking_mode() {
        std::thread::scope(|s| {
            let storage_h = s.spawn(|| {
                (
                    storage::scan_rebuildable_caches(),
                    storage::collect_ssd_health(),
                    count_apfs_snapshots(),
                )
            });
            let procs_h = s.spawn(|| {
                (
                    developer_tools::detect_ai_agents(&ps_raw, &listening_ports),
                    developer_tools::detect_orphaned_dev_servers(&ps_raw, &listening_ports),
                    audio::collect_audio_snapshot(&ps_raw),
                )
            });
            let network_h = s.spawn(network::collect_network_snapshot);
            let runtimes_h = s.spawn(|| {
                (
                    developer_tools::collect_ollama_models(),
                    developer_tools::collect_containers(),
                )
            });
            let host_h = s.spawn(|| {
                (
                    developer_tools::collect_launch_agents(),
                    developer_tools::read_quick_switch_state(),
                    developer_tools::collect_ai_token_usage(),
                )
            });
            let power = devices::collect_silicon_power_and_pressure();
            (
                storage_h.join().unwrap_or_default(),
                procs_h.join().unwrap_or_default(),
                network_h.join().unwrap_or_default(),
                power,
                runtimes_h.join().unwrap_or_default(),
                host_h.join().unwrap_or_default(),
            )
        })
    } else {
        let storage_res = (
            storage::scan_rebuildable_caches(),
            storage::collect_ssd_health(),
            count_apfs_snapshots(),
        );
        let procs_res = (
            developer_tools::detect_ai_agents(&ps_raw, &listening_ports),
            developer_tools::detect_orphaned_dev_servers(&ps_raw, &listening_ports),
            audio::collect_audio_snapshot(&ps_raw),
        );
        let network_res = network::collect_network_snapshot();
        let power_res = devices::collect_silicon_power_and_pressure();
        let runtimes_res = (
            developer_tools::collect_ollama_models(),
            developer_tools::collect_containers(),
        );
        let host_res = (
            developer_tools::collect_launch_agents(),
            developer_tools::read_quick_switch_state(),
            developer_tools::collect_ai_token_usage(),
        );
        (
            storage_res,
            procs_res,
            network_res,
            power_res,
            runtimes_res,
            host_res,
        )
    };
    disk.apfs_snapshots_count = apfs_snapshots_count;
    PILL_PROC_CACHE.store((self_cost.clone(), agents.clone()));

    SystemExtrasSnapshot {
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
        ai_token_usage,
    }
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

fn parse_vm_stat_with_paging(
    memsize_bytes: u64,
    vm_stat_raw: &str,
    swap_raw: &str,
) -> (MemoryMetrics, u64, u64) {
    let mut page_size: u64 = 16384;
    let mut active_pages: u64 = 0;
    let mut wired_pages: u64 = 0;
    let mut compressed_pages: u64 = 0;
    let mut pageins: u64 = 0;
    let mut swapouts: u64 = 0;

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
            let count = v.trim().trim_end_matches('.').parse::<u64>().unwrap_or(0);
            match k.trim() {
                "Pages active" => active_pages = count,
                "Pages wired down" => wired_pages = count,
                "Pages occupied by compressor" => compressed_pages = count,
                "Pageins" => pageins = count,
                "Swapouts" => swapouts = count,
                _ => {}
            }
        }
    }

    let swap_used_mb = parse_swap_used_mb(swap_raw);
    let metrics = build_memory_metrics(
        memsize_bytes,
        active_pages * page_size,
        wired_pages * page_size,
        compressed_pages * page_size,
        swap_used_mb,
        (0.0, 0.0),
    );
    (metrics, pageins, swapouts)
}

#[cfg(test)]
pub(crate) fn parse_vm_stat_and_mem(
    memsize_bytes: u64,
    vm_stat_raw: &str,
    swap_raw: &str,
) -> MemoryMetrics {
    parse_vm_stat_with_paging(memsize_bytes, vm_stat_raw, swap_raw).0
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

#[derive(Clone, Debug)]
pub(super) struct PsRow {
    pid: u32,
    ppid: u32,
    etime: String,
    pcpu: f64,
    rss_kb: u64,
    comm: String,
}

pub(super) fn parse_ps_rows(ps_raw: &str) -> Vec<PsRow> {
    let mut rows = Vec::new();
    for line in ps_raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        let Some((pid_s, rest1)) = split_token(s) else {
            continue;
        };
        let Some((ppid_s, rest2)) = split_token(rest1) else {
            continue;
        };
        let Some((mut etime_s, mut rest3)) = split_token(rest2) else {
            continue;
        };
        // The shared table carries a process-state column before etime;
        // elapsed times always contain ':', states never do.
        if !etime_s.contains(':') {
            let Some((real_etime, after)) = split_token(rest3) else {
                continue;
            };
            etime_s = real_etime;
            rest3 = after;
        }
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

pub(super) fn split_token(input: &str) -> Option<(&str, &str)> {
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
    #[ignore = "opt-in host diagnostic: reads macOS sensors and system services"]
    fn collect_system_extras_completes_within_latency_budget() {
        let start = Instant::now();
        let _snap = collect_system_extras();
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 1200,
            "collect_system_extras() took {elapsed_ms}ms, exceeding 1200ms budget"
        );
    }

    #[test]
    fn extras_cache_coalesces_scopes_and_invalidation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = ExtrasCache::new();
        let fetches = AtomicUsize::new(0);
        let call = |scope| {
            cache.get(scope, |_| {
                fetches.fetch_add(1, Ordering::SeqCst);
                default_extras_snapshot(0)
            })
        };
        call(ExtrasScope::Pill);
        // A repeated pill call reuses the fresh pill entry.
        call(ExtrasScope::Pill);
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        // Pill-only data can never mask a full request.
        call(ExtrasScope::Full);
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
        // A full entry satisfies either scope.
        call(ExtrasScope::Pill);
        call(ExtrasScope::Full);
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
        cache.invalidate();
        call(ExtrasScope::Pill);
        assert_eq!(fetches.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn extras_cache_runs_one_collector_for_concurrent_callers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = std::sync::Arc::new(ExtrasCache::new());
        let fetches = std::sync::Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let cache = cache.clone();
            let fetches = fetches.clone();
            handles.push(std::thread::spawn(move || {
                cache.get(ExtrasScope::Full, |_| {
                    fetches.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(50));
                    default_extras_snapshot(0)
                })
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn extras_cache_latest_snapshot_does_not_block_during_fetch() {
        let cache = std::sync::Arc::new(ExtrasCache::new());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker = {
            let cache = cache.clone();
            std::thread::spawn(move || {
                cache.get(ExtrasScope::Full, |_| {
                    started_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(150));
                    default_extras_snapshot(0)
                })
            })
        };
        started_rx.recv().unwrap();
        let start = Instant::now();
        assert!(cache.peek_latest().is_none());
        assert!(
            start.elapsed() < Duration::from_millis(20),
            "peek_latest waited behind an in-flight collection"
        );
        worker.join().unwrap();
        assert!(cache.peek_latest().is_some());
    }

    #[test]
    fn extras_cache_invalidation_during_fetch_discards_the_stale_result() {
        let cache = ExtrasCache::new();
        cache.get(ExtrasScope::Full, |_| {
            cache.invalidate();
            default_extras_snapshot(0)
        });
        assert!(cache.peek_latest().is_none());
    }

    #[test]
    fn parses_ps_rows_with_and_without_state_column() {
        let rows = parse_ps_rows(
            "  10     1  14:22   4.5  2048 /usr/local/bin/claude\n  11    10 Ss+  1-02:03:04  1.5  1024 /bin/zsh\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].pid, rows[0].ppid), (10, 1));
        assert_eq!(rows[0].etime, "14:22");
        assert_eq!((rows[1].pid, rows[1].ppid), (11, 10));
        assert_eq!(rows[1].etime, "1-02:03:04");
        assert_eq!(rows[1].comm, "/bin/zsh");
    }

    #[test]
    fn swr_cache_blocking_mode_populates_synchronously_on_cold_read() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static BLOCKING_CELL: OnceLock<Mutex<SwrEntry<u32>>> = OnceLock::new();
        static ASYNC_CELL: OnceLock<Mutex<SwrEntry<u32>>> = OnceLock::new();

        let blocking_calls = std::sync::Arc::new(AtomicUsize::new(0));
        let bc = blocking_calls.clone();
        let val = get_or_refresh_swr_with_mode(
            &BLOCKING_CELL,
            Duration::from_secs(60),
            true,
            move || {
                bc.fetch_add(1, Ordering::SeqCst);
                42
            },
        );
        assert_eq!(val, 42);
        assert_eq!(blocking_calls.load(Ordering::SeqCst), 1);

        let bc2 = blocking_calls.clone();
        let cached = get_or_refresh_swr_with_mode(
            &BLOCKING_CELL,
            Duration::from_secs(60),
            true,
            move || {
                bc2.fetch_add(1, Ordering::SeqCst);
                99
            },
        );
        assert_eq!(cached, 42);
        assert_eq!(blocking_calls.load(Ordering::SeqCst), 1);

        let cold_async =
            get_or_refresh_swr_with_mode(&ASYNC_CELL, Duration::from_secs(60), false, || 77);
        assert_eq!(cold_async, 0);
    }

    #[test]
    #[ignore = "opt-in host diagnostic: reads macOS sensors and system services"]
    fn collect_metrics_extras_completes_under_300ms() {
        let _ = collect_metrics_extras(true);
        let start = Instant::now();
        let snap = collect_metrics_extras(true);
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 300,
            "collect_metrics_extras() took {elapsed_ms}ms, exceeding 300ms budget"
        );
        assert!(snap.memory.total_gb > 0.0);
    }

    #[test]
    #[ignore = "opt-in host diagnostic: reads macOS sensors and system services"]
    fn collect_pill_extras_completes_under_150ms() {
        let _ = collect_pill_extras();
        EXTRAS_CACHE.invalidate();
        let start = Instant::now();
        let snap = collect_pill_extras();
        let elapsed_ms = start.elapsed().as_millis();
        assert!(
            elapsed_ms < 150,
            "collect_pill_extras() took {elapsed_ms}ms, exceeding 150ms budget"
        );
        assert!(snap.memory.total_gb > 0.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires live macOS host"]
    fn native_memory_sampling_succeeds_on_macos() {
        let memsize = physical_memory_bytes();
        assert!(memsize > 0);
        let mem = native_sample_memory(memsize).expect("native Mach memory sample");
        assert!(mem.total_gb > 0.0);
        assert!(mem.used_gb > 0.0);
        assert!(mem.used_gb <= mem.total_gb);
    }
}
