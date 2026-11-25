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

#[derive(Debug, Clone, Default)]
pub struct NativeAudioDevice {
    pub id: u32,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub is_default_output: bool,
    pub is_default_input: bool,
}

#[cfg(target_os = "macos")]
mod native {
    use super::{AppleSiliconMetrics, NativeAudioDevice, NativeWifiReading};
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

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct AudioDeviceNativeC {
        id: u32,
        name: [c_char; 96],
        is_input: u8,
        is_output: u8,
        is_default_output: u8,
        is_default_input: u8,
    }

    extern "C" {
        fn sp_sample_apple_silicon_metrics(out: *mut AppleSiliconMetricsC) -> c_int;
        fn sp_read_thermal_pressure_level() -> c_int;
        fn sp_read_wifi_native(out: *mut WifiNativeC) -> c_int;
        fn sp_get_audio_volume_native(out_vol: *mut u8, in_vol: *mut u8, muted: *mut u8) -> c_int;
        fn sp_set_audio_volume_native(out_vol: c_int, in_vol: c_int, muted: c_int) -> c_int;
        fn sp_list_audio_devices_native(out: *mut AudioDeviceNativeC, max_count: c_int) -> c_int;
        fn sp_set_default_audio_device_native(device_id: u32, is_input: c_int) -> c_int;
        fn sp_get_input_mute_native(muted: *mut u8) -> c_int;
        fn sp_set_input_mute_native(muted: c_int) -> c_int;
        fn sp_register_global_hotkey(
            enabled: c_int,
            key_code: u32,
            modifier_bits: u32,
            callback: extern "C" fn(),
        ) -> c_int;
        fn sp_register_mic_mute_hotkey(
            enabled: c_int,
            key_code: u32,
            modifier_bits: u32,
            callback: extern "C" fn(),
        ) -> c_int;
        fn sp_post_user_notification(title: *const c_char, subtitle: *const c_char) -> c_int;
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

    pub fn register_global_hotkey(binding: Option<(u16, u8)>, callback: extern "C" fn()) {
        let (enabled, key_code, modifiers) = match binding {
            Some((k, m)) => (1, u32::from(k), u32::from(m)),
            None => (0, 0, 0),
        };
        unsafe {
            let _ = sp_register_global_hotkey(enabled, key_code, modifiers, callback);
        }
    }

    pub fn register_mic_mute_hotkey(binding: Option<(u16, u8)>, callback: extern "C" fn()) {
        let (enabled, key_code, modifiers) = match binding {
            Some((k, m)) => (1, u32::from(k), u32::from(m)),
            None => (0, 0, 0),
        };
        unsafe {
            let _ = sp_register_mic_mute_hotkey(enabled, key_code, modifiers, callback);
        }
    }

    pub fn list_audio_devices() -> Vec<NativeAudioDevice> {
        let mut buf: Vec<AudioDeviceNativeC> = vec![unsafe { std::mem::zeroed() }; 32];
        let n = unsafe { sp_list_audio_devices_native(buf.as_mut_ptr(), 32) };
        if n <= 0 {
            return Vec::new();
        }
        buf.into_iter()
            .take(n as usize)
            .map(|raw| {
                let name = unsafe { CStr::from_ptr(raw.name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
                NativeAudioDevice {
                    id: raw.id,
                    name,
                    is_input: raw.is_input != 0,
                    is_output: raw.is_output != 0,
                    is_default_output: raw.is_default_output != 0,
                    is_default_input: raw.is_default_input != 0,
                }
            })
            .collect()
    }

    pub fn set_default_audio_device(device_id: u32, is_input: bool) -> bool {
        unsafe { sp_set_default_audio_device_native(device_id, if is_input { 1 } else { 0 }) == 1 }
    }

    pub fn get_input_mute_native() -> Option<bool> {
        let mut muted: u8 = 0;
        if unsafe { sp_get_input_mute_native(&mut muted) } == 1 {
            Some(muted != 0)
        } else {
            None
        }
    }

    pub fn set_input_mute_native(muted: bool) -> bool {
        unsafe { sp_set_input_mute_native(if muted { 1 } else { 0 }) == 1 }
    }

    pub fn post_user_notification(title: &str, subtitle: &str) {
        let Ok(c_title) = std::ffi::CString::new(title) else {
            return;
        };
        let Ok(c_sub) = std::ffi::CString::new(subtitle) else {
            return;
        };
        unsafe {
            let _ = sp_post_user_notification(c_title.as_ptr(), c_sub.as_ptr());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::{AppleSiliconMetrics, NativeAudioDevice, NativeWifiReading};

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

    pub fn register_global_hotkey(_binding: Option<(u16, u8)>, _callback: extern "C" fn()) {}

    pub fn register_mic_mute_hotkey(_binding: Option<(u16, u8)>, _callback: extern "C" fn()) {}

    pub fn list_audio_devices() -> Vec<NativeAudioDevice> {
        Vec::new()
    }

    pub fn set_default_audio_device(_device_id: u32, _is_input: bool) -> bool {
        false
    }

    pub fn get_input_mute_native() -> Option<bool> {
        None
    }

    pub fn set_input_mute_native(_muted: bool) -> bool {
        false
    }

    pub fn post_user_notification(_title: &str, _subtitle: &str) {}
}

pub use native::{
    get_input_mute_native, get_native_audio_volume, list_audio_devices, post_user_notification,
    read_native_thermal_pressure, read_native_wifi, register_global_hotkey,
    register_mic_mute_hotkey, sample_apple_silicon_metrics, set_default_audio_device,
    set_input_mute_native, set_native_audio_volume,
};
