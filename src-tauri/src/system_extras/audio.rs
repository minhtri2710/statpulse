// Audio/AV probes: output/input volume and microphone/camera activity.

use super::{parse_ps_rows, SwrCache};
use crate::proc_info::{canonical_process_name, cmd_stdout};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AudioDeviceItem {
    pub id: u32,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    pub is_default_output: bool,
    pub is_default_input: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct NowPlayingItem {
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct AudioSnapshot {
    pub output_volume: u8,
    pub input_volume: u8,
    pub output_muted: bool,
    #[serde(default)]
    pub input_muted: bool,
    pub mic_in_use_by: Option<String>,
    #[serde(default)]
    pub camera_in_use: bool,
    #[serde(default)]
    pub now_playing: Option<NowPlayingItem>,
    #[serde(default)]
    pub output_devices: Vec<AudioDeviceItem>,
    #[serde(default)]
    pub input_devices: Vec<AudioDeviceItem>,
}

static AUDIO_VOLUME_CACHE: SwrCache<Option<(u8, u8, bool, bool)>> = SwrCache::new();

static AUDIO_DEVICES_CACHE: SwrCache<(Vec<AudioDeviceItem>, Vec<AudioDeviceItem>)> = SwrCache::new();

static AV_PRIVACY_CACHE: SwrCache<(Option<String>, bool)> = SwrCache::new();

static NOW_PLAYING_CACHE: SwrCache<Option<NowPlayingItem>> = SwrCache::new();

static SAVED_INPUT_GAIN: Mutex<u8> = Mutex::new(75);

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
    AV_PRIVACY_CACHE.get_or_refresh(Duration::from_secs(5), move || {
        (detect_mic_user(&ps_owned), detect_camera_in_use(&ps_owned))
    })
}

fn read_audio_volume_uncached() -> (u8, u8, bool, bool) {
    let (out_v, in_v, out_muted) = if let Some(native) = crate::macos_bridge::get_native_audio_volume() {
        native
    } else {
        let vol_raw =
            cmd_stdout("/usr/bin/osascript", &["-e", "get volume settings"]).unwrap_or_default();
        parse_osascript_volume(&vol_raw)
    };
    let in_muted = crate::macos_bridge::get_input_mute_native().unwrap_or(in_v == 0);
    (out_v, in_v, out_muted, in_muted)
}

fn read_audio_devices_uncached() -> (Vec<AudioDeviceItem>, Vec<AudioDeviceItem>) {
    let all = crate::macos_bridge::list_audio_devices();
    let mut outputs = Vec::new();
    let mut inputs = Vec::new();
    for dev in all {
        let item = AudioDeviceItem {
            id: dev.id,
            name: dev.name,
            is_input: dev.is_input,
            is_output: dev.is_output,
            is_default_output: dev.is_default_output,
            is_default_input: dev.is_default_input,
        };
        if item.is_output {
            outputs.push(item.clone());
        }
        if item.is_input {
            inputs.push(item);
        }
    }
    (outputs, inputs)
}

pub fn parse_now_playing_line(raw: &str, app_name: &str) -> Option<NowPlayingItem> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('\t').collect();
    if parts.len() < 2 {
        return None;
    }
    let playing = parts[0].trim().eq_ignore_ascii_case("playing");
    let title = parts[1].trim().to_string();
    if title.is_empty() {
        return None;
    }
    let artist = parts.get(2).map(|s| s.trim()).unwrap_or("").to_string();
    Some(NowPlayingItem {
        app: app_name.to_string(),
        title,
        artist,
        playing,
    })
}

fn running_media_apps(ps_raw: &str) -> (bool, bool) {
    let mut spotify = false;
    let mut music = false;
    for row in parse_ps_rows(ps_raw) {
        if row.comm.contains("Spotify.app/Contents/MacOS/Spotify") || row.comm == "Spotify" {
            spotify = true;
        } else if row.comm.contains("Music.app/Contents/MacOS/Music") || row.comm == "Music" {
            music = true;
        }
    }
    (spotify, music)
}

fn query_now_playing_for_apps(spotify: bool, music: bool) -> Option<NowPlayingItem> {
    let mut best: Option<NowPlayingItem> = None;
    for (enabled, app_name) in [(spotify, "Spotify"), (music, "Music")] {
        if !enabled {
            continue;
        }
        let script = format!(
            "tell application \"{app_name}\" to if player state is playing or player state is paused then return (player state as string) & \"\\t\" & name of current track & \"\\t\" & artist of current track"
        );
        if let Some(raw) = cmd_stdout("/usr/bin/osascript", &["-e", &script]) {
            if let Some(item) = parse_now_playing_line(&raw, app_name) {
                if item.playing {
                    return Some(item);
                }
                if best.is_none() {
                    best = Some(item);
                }
            }
        }
    }
    best
}

fn collect_now_playing(ps_raw: &str) -> Option<NowPlayingItem> {
    let (spotify, music) = running_media_apps(ps_raw);
    if !spotify && !music {
        return None;
    }
    NOW_PLAYING_CACHE.get_or_refresh(Duration::from_secs(4), move || {
        query_now_playing_for_apps(spotify, music)
    })
}

pub fn control_media_playback(action: &str) -> Result<Option<NowPlayingItem>, String> {
    let cmd_verb = match action {
        "play_pause" => "playpause",
        "next" => "next track",
        "previous" => "previous track",
        _ => return Err(format!("Unsupported media action: {action}")),
    };
    let ps_raw =
        cmd_stdout("/bin/ps", &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="]).unwrap_or_default();
    let (spotify, music) = running_media_apps(&ps_raw);
    let target = if spotify {
        Some("Spotify")
    } else if music {
        Some("Music")
    } else {
        None
    };
    let Some(app_name) = target else {
        return Err("No active Spotify or Music player found.".into());
    };
    let script = format!("tell application \"{app_name}\" to {cmd_verb}");
    let status = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .status()
        .map_err(|e| format!("Failed to control {app_name}: {e}"))?;
    if !status.success() {
        return Err(format!("{app_name} media control failed."));
    }
    let fresh = query_now_playing_for_apps(spotify, music);
    NOW_PLAYING_CACHE.store(fresh.clone());
    super::EXTRAS_CACHE.invalidate();
    Ok(fresh)
}

pub(super) fn collect_audio_snapshot(ps_raw: &str) -> AudioSnapshot {
    let (output_volume, input_volume, output_muted, input_muted) = AUDIO_VOLUME_CACHE
        .get_or_refresh(Duration::from_secs(3), || Some(read_audio_volume_uncached()))
        .unwrap_or((50, 50, false, false));

    let (output_devices, input_devices) =
        AUDIO_DEVICES_CACHE.get_or_refresh(Duration::from_secs(3), read_audio_devices_uncached);
    let (mic_in_use_by, camera_in_use) = collect_av_privacy(ps_raw);
    let now_playing = collect_now_playing(ps_raw);
    AudioSnapshot {
        output_volume,
        input_volume,
        output_muted,
        input_muted,
        mic_in_use_by,
        camera_in_use,
        now_playing,
        output_devices,
        input_devices,
    }
}

pub fn is_camera_or_mic_active() -> bool {
    let ps_raw = if AV_PRIVACY_CACHE.is_stale(Duration::from_secs(5)) {
        cmd_stdout("/bin/ps", &["-axo", "pid=,ppid=,etime=,pcpu=,rss=,comm="]).unwrap_or_default()
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
    if !crate::macos_bridge::set_native_audio_volume(output_volume, input_volume, muted) {
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
    }
    let fresh_vol = read_audio_volume_uncached();
    AUDIO_VOLUME_CACHE.store(Some(fresh_vol));
    super::EXTRAS_CACHE.invalidate();
    let (output_devices, input_devices) = AUDIO_DEVICES_CACHE.peek();
    let (mic_in_use_by, camera_in_use) = AV_PRIVACY_CACHE.peek();
    let now_playing = NOW_PLAYING_CACHE.peek();
    Ok(AudioSnapshot {
        output_volume: fresh_vol.0,
        input_volume: fresh_vol.1,
        output_muted: fresh_vol.2,
        input_muted: fresh_vol.3,
        mic_in_use_by,
        camera_in_use,
        now_playing,
        output_devices,
        input_devices,
    })
}

pub fn set_audio_device(device_id: u32, kind: &str) -> Result<AudioSnapshot, String> {
    let is_input = match kind {
        "input" => true,
        "output" => false,
        _ => return Err("Audio device kind must be 'output' or 'input'".into()),
    };
    if !crate::macos_bridge::set_default_audio_device(device_id, is_input) {
        return Err(format!("Failed to switch default {kind} audio device"));
    }
    let devices = read_audio_devices_uncached();
    AUDIO_DEVICES_CACHE.store(devices.clone());
    let fresh_vol = read_audio_volume_uncached();
    AUDIO_VOLUME_CACHE.store(Some(fresh_vol));
    super::EXTRAS_CACHE.invalidate();
    let (mic_in_use_by, camera_in_use) = AV_PRIVACY_CACHE.peek();
    let now_playing = NOW_PLAYING_CACHE.peek();
    Ok(AudioSnapshot {
        output_volume: fresh_vol.0,
        input_volume: fresh_vol.1,
        output_muted: fresh_vol.2,
        input_muted: fresh_vol.3,
        mic_in_use_by,
        camera_in_use,
        now_playing,
        output_devices: devices.0,
        input_devices: devices.1,
    })
}

pub fn toggle_mic_mute() -> Result<AudioSnapshot, String> {
    let (_, cur_in_vol, _, cur_in_muted) = read_audio_volume_uncached();
    let target_muted = !cur_in_muted;
    if !crate::macos_bridge::set_input_mute_native(target_muted) {
        if target_muted {
            if cur_in_vol > 0 {
                if let Ok(mut g) = SAVED_INPUT_GAIN.lock() {
                    *g = cur_in_vol;
                }
            }
            return set_audio_volume(None, Some(0), None);
        } else {
            let restore = SAVED_INPUT_GAIN.lock().map(|g| (*g).max(20)).unwrap_or(75);
            return set_audio_volume(None, Some(restore), None);
        }
    }
    let fresh_vol = read_audio_volume_uncached();
    AUDIO_VOLUME_CACHE.store(Some(fresh_vol));
    super::EXTRAS_CACHE.invalidate();
    let (output_devices, input_devices) = AUDIO_DEVICES_CACHE.peek();
    let (mic_in_use_by, camera_in_use) = AV_PRIVACY_CACHE.peek();
    let now_playing = NOW_PLAYING_CACHE.peek();
    Ok(AudioSnapshot {
        output_volume: fresh_vol.0,
        input_volume: fresh_vol.1,
        output_muted: fresh_vol.2,
        input_muted: fresh_vol.3,
        mic_in_use_by,
        camera_in_use,
        now_playing,
        output_devices,
        input_devices,
    })
}

pub fn list_audio_devices() -> (Vec<AudioDeviceItem>, Vec<AudioDeviceItem>) {
    read_audio_devices_uncached()
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

    #[test]
    fn parses_now_playing_tab_line() {
        let item = parse_now_playing_line("playing\tMidnight City\tM83\n", "Spotify").unwrap();
        assert!(item.playing);
        assert_eq!(item.app, "Spotify");
        assert_eq!(item.title, "Midnight City");
        assert_eq!(item.artist, "M83");
    }
}
