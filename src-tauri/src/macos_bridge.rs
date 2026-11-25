#[derive(Debug, Clone, Copy, Default)]
pub struct AppleSiliconMetrics {
    pub gpu_watts: f64,
    pub cpu_watts: f64,
    pub ane_watts: f64,
    pub dram_watts: f64,
    pub e_mhz: u32,
    pub p_mhz: u32,
    pub gpu_mhz: u32,
    pub e_active_pct: u8,
    pub p_active_pct: u8,
    pub gpu_active_pct: u8,
}

#[derive(Debug, Clone, Default)]
pub struct NativeWifiReading {
    pub channel: String,
    pub phy_mode: String,
    pub security: String,
    pub tx_rate_mbps: u32,
    pub rssi_dbm: i32,
    pub noise_dbm: i32,
    pub snr_db: i32,
}

#[cfg(target_os = "macos")]
mod native {
    use super::{AppleSiliconMetrics, NativeWifiReading};
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int};

    #[repr(C)]
    #[derive(Default)]
    struct AppleSiliconMetricsC {
        gpu_watts: f64,
        cpu_watts: f64,
        ane_watts: f64,
        dram_watts: f64,
        e_mhz: u32,
        p_mhz: u32,
        gpu_mhz: u32,
        e_active_pct: u8,
        p_active_pct: u8,
        gpu_active_pct: u8,
        valid: u8,
    }

    #[repr(C)]
    struct WifiNativeC {
        channel: [c_char; 48],
        phy_mode: [c_char; 24],
        security: [c_char; 32],
        tx_rate_mbps: u32,
        rssi_dbm: i32,
        noise_dbm: i32,
        snr_db: i32,
        connected: u8,
    }

    extern "C" {
        fn sp_sample_apple_silicon_metrics(out: *mut AppleSiliconMetricsC) -> c_int;
        fn sp_read_thermal_pressure_level() -> c_int;
        fn sp_read_wifi_native(out: *mut WifiNativeC) -> c_int;
        fn sp_get_audio_volume_native(out_vol: *mut u8, in_vol: *mut u8, muted: *mut u8) -> c_int;
        fn sp_set_audio_volume_native(out_vol: c_int, in_vol: c_int, muted: c_int) -> c_int;
        fn sp_register_global_hotkey(preset_id: c_int, callback: extern "C" fn()) -> c_int;
    }

    pub fn sample_apple_silicon_metrics() -> Option<AppleSiliconMetrics> {
        let mut raw = AppleSiliconMetricsC::default();
        if unsafe { sp_sample_apple_silicon_metrics(&mut raw) } == 1 && raw.valid != 0 {
            Some(AppleSiliconMetrics {
                gpu_watts: raw.gpu_watts,
                cpu_watts: raw.cpu_watts,
                ane_watts: raw.ane_watts,
                dram_watts: raw.dram_watts,
                e_mhz: raw.e_mhz,
                p_mhz: raw.p_mhz,
                gpu_mhz: raw.gpu_mhz,
                e_active_pct: raw.e_active_pct,
                p_active_pct: raw.p_active_pct,
                gpu_active_pct: raw.gpu_active_pct,
            })
        } else {
            None
        }
    }

    pub fn read_native_thermal_pressure() -> Option<&'static str> {
        match unsafe { sp_read_thermal_pressure_level() } {
            0 => Some("nominal"),
            1 => Some("moderate"),
            2 => Some("heavy"),
            3 => Some("critical"),
            _ => None,
        }
    }

    pub fn read_native_wifi() -> Option<NativeWifiReading> {
        let mut raw: WifiNativeC = unsafe { std::mem::zeroed() };
        if unsafe { sp_read_wifi_native(&mut raw) } != 1 || raw.connected == 0 {
            return None;
        }
        let cstr = |buf: &[c_char]| -> String {
            unsafe { CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        };
        Some(NativeWifiReading {
            channel: cstr(&raw.channel),
            phy_mode: cstr(&raw.phy_mode),
            security: cstr(&raw.security),
            tx_rate_mbps: raw.tx_rate_mbps,
            rssi_dbm: raw.rssi_dbm,
            noise_dbm: raw.noise_dbm,
            snr_db: raw.snr_db,
        })
    }

    pub fn get_native_audio_volume() -> Option<(u8, u8, bool)> {
        let (mut out_v, mut in_v, mut muted) = (0_u8, 50_u8, 0_u8);
        if unsafe { sp_get_audio_volume_native(&mut out_v, &mut in_v, &mut muted) } == 1 {
            Some((out_v, in_v, muted != 0))
        } else {
            None
        }
    }

    pub fn set_native_audio_volume(
        out_vol: Option<u8>,
        in_vol: Option<u8>,
        muted: Option<bool>,
    ) -> bool {
        let o = out_vol.map(|v| i32::from(v.min(100))).unwrap_or(-1);
        let i = in_vol.map(|v| i32::from(v.min(100))).unwrap_or(-1);
        let m = muted.map(|b| if b { 1 } else { 0 }).unwrap_or(-1);
        unsafe { sp_set_audio_volume_native(o, i, m) == 1 }
    }

    pub fn register_global_hotkey(preset_id: i32, callback: extern "C" fn()) {
        unsafe {
            let _ = sp_register_global_hotkey(preset_id, callback);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::{AppleSiliconMetrics, NativeWifiReading};

    pub fn sample_apple_silicon_metrics() -> Option<AppleSiliconMetrics> {
        None
    }

    pub fn read_native_thermal_pressure() -> Option<&'static str> {
        None
    }

    pub fn read_native_wifi() -> Option<NativeWifiReading> {
        None
    }

    pub fn get_native_audio_volume() -> Option<(u8, u8, bool)> {
        None
    }

    pub fn set_native_audio_volume(
        _out_vol: Option<u8>,
        _in_vol: Option<u8>,
        _muted: Option<bool>,
    ) -> bool {
        false
    }

    pub fn register_global_hotkey(_preset_id: i32, _callback: extern "C" fn()) {}
}

pub use native::{
    get_native_audio_volume, read_native_thermal_pressure, read_native_wifi,
    register_global_hotkey, sample_apple_silicon_metrics, set_native_audio_volume,
};
