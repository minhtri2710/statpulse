// Audio/AV probes: output/input volume and microphone/camera activity.

use super::{cmd_stdout, get_or_refresh_swr, parse_ps_rows, SwrEntry};
use crate::hardware_telemetry::adapters::process::canonical_process_name;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

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

static AUDIO_VOLUME_CACHE: OnceLock<Mutex<SwrEntry<Option<(u8, u8, bool)>>>> = OnceLock::new();

static AV_PRIVACY_CACHE: OnceLock<Mutex<SwrEntry<(Option<String>, bool)>>> = OnceLock::new();

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
    let vol_raw =
        cmd_stdout("/usr/bin/osascript", &["-e", "get volume settings"]).unwrap_or_default();
    parse_osascript_volume(&vol_raw)
}

pub(super) fn collect_audio_snapshot(ps_raw: &str) -> AudioSnapshot {
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
        .map(|g| {
            g.updated_at
                .map(|ts| ts.elapsed() >= Duration::from_secs(5))
                .unwrap_or(true)
        })
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
        cmds.push(format!(
            "set volume output muted {}",
            if m { "true" } else { "false" }
        ));
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
    super::EXTRAS_CACHE.invalidate();
    let ps_raw =
        cmd_stdout("/bin/ps", &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="]).unwrap_or_default();
    let (mic_in_use_by, camera_in_use) = collect_av_privacy(&ps_raw);
    Ok(AudioSnapshot {
        output_volume: fresh_vol.0,
        input_volume: fresh_vol.1,
        output_muted: fresh_vol.2,
        mic_in_use_by,
        camera_in_use,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_osascript_volume() {
        assert_eq!(
            parse_osascript_volume(
                "output volume:85, input volume:60, alert volume:52, output muted:false"
            ),
            (85, 60, false)
        );
    }
}
