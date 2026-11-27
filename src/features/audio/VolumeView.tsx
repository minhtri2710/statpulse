import { useEffect, useRef, useState } from "react";
import {
  Camera,
  Check,
  Mic,
  MicOff,
  Music,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Speaker,
  Volume2,
  VolumeX,
} from "lucide-react";
import type { SystemExtrasSnapshot } from "../../types";

interface VolumeViewProps {
  extras: SystemExtrasSnapshot | null;
  micMuteShortcutLabel: string | null;
  handleUpdateAudio: (
    outputVolume: number | null,
    inputVolume: number | null,
    muted: boolean | null,
  ) => Promise<void>;
  handleSetAudioDevice: (deviceId: number, kind: "output" | "input") => Promise<void>;
  handleToggleMicMute: () => Promise<void>;
  handleMediaControl: (action: "play_pause" | "next" | "previous") => Promise<void>;
}

export const VolumeView = ({
  extras,
  micMuteShortcutLabel,
  handleUpdateAudio,
  handleSetAudioDevice,
  handleToggleMicMute,
  handleMediaControl,
}: VolumeViewProps) => {
  const [outVol, setOutVol] = useState<number>(extras?.audio.output_volume ?? 50);
  const [inVol, setInVol] = useState<number>(extras?.audio.input_volume ?? 50);
  const draggingOutRef = useRef(false);
  const draggingInRef = useRef(false);
  const outDebounceRef = useRef<number | null>(null);
  const inDebounceRef = useRef<number | null>(null);

  useEffect(() => {
    if (!draggingOutRef.current && extras?.audio.output_volume !== undefined) {
      setOutVol(extras.audio.output_volume);
    }
  }, [extras?.audio.output_volume]);

  useEffect(() => {
    if (!draggingInRef.current && extras?.audio.input_volume !== undefined) {
      setInVol(extras.audio.input_volume);
    }
  }, [extras?.audio.input_volume]);

  useEffect(() => {
    return () => {
      if (outDebounceRef.current !== null) window.clearTimeout(outDebounceRef.current);
      if (inDebounceRef.current !== null) window.clearTimeout(inDebounceRef.current);
    };
  }, []);

  const scheduleCommit = (out: number | null, inp: number | null, immediate = false) => {
    const ref = out !== null ? outDebounceRef : inDebounceRef;
    if (ref.current !== null) {
      window.clearTimeout(ref.current);
      ref.current = null;
    }
    if (immediate) {
      void handleUpdateAudio(out, inp, null);
      return;
    }
    ref.current = window.setTimeout(() => {
      ref.current = null;
      void handleUpdateAudio(out, inp, null);
    }, 120);
  };

  const nowPlaying = extras?.audio.now_playing ?? null;

  return (
    <>
              {nowPlaying && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Music className="w-3 h-3 text-[var(--acc)]" />
                      Now Playing
                    </span>
                    <em className="font-mono">{nowPlaying.app}</em>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <div className="min-w-0">
                      <div className="text-[11px] font-semibold text-[var(--ink)] truncate">
                        {nowPlaying.title}
                      </div>
                      {nowPlaying.artist && (
                        <div className="text-[9.5px] text-[var(--sec)] truncate">
                          {nowPlaying.artist}
                        </div>
                      )}
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button
                        type="button"
                        onClick={() => void handleMediaControl("previous")}
                        className="pp-btn p-1"
                        title="Previous Track"
                      >
                        <SkipBack className="w-3 h-3" />
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleMediaControl("play_pause")}
                        className="pp-btn pp-btn--acc p-1"
                        title={nowPlaying.playing ? "Pause" : "Play"}
                      >
                        {nowPlaying.playing ? (
                          <Pause className="w-3 h-3" />
                        ) : (
                          <Play className="w-3 h-3" />
                        )}
                      </button>
                      <button
                        type="button"
                        onClick={() => void handleMediaControl("next")}
                        className="pp-btn p-1"
                        title="Next Track"
                      >
                        <SkipForward className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {extras?.audio.camera_in_use && (
                <div className="pp-card pp-card--attn">
                  <div className="pp-attn">
                    <Camera className="w-3.5 h-3.5 text-rose-400 shrink-0" />
                    <span className="text-[var(--ink)]">
                      <b>Webcam is currently active</b> or streaming
                    </span>
                  </div>
                </div>
              )}

              {extras?.audio.mic_in_use_by && (
                <div className="pp-card pp-card--attn">
                  <div className="pp-attn">
                    <Mic className="w-3.5 h-3.5 text-[var(--acc)] shrink-0" />
                    <span className="text-[var(--ink)]">
                      Mic in use by <b>{extras.audio.mic_in_use_by}</b>
                    </span>
                  </div>
                </div>
              )}

              <div className="pp-card gap-2.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    {extras?.audio.output_muted ? (
                      <VolumeX className="w-3.5 h-3.5 text-rose-400" />
                    ) : (
                      <Volume2 className="w-3.5 h-3.5 text-[var(--acc)]" />
                    )}
                    Output Volume
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      void handleUpdateAudio(null, null, !(extras?.audio.output_muted ?? false))
                    }
                    className={`pp-btn ${extras?.audio.output_muted ? "pp-btn--danger" : ""}`}
                  >
                    {extras?.audio.output_muted ? "Muted" : "Mute"}
                  </button>
                </div>

                <div className="flex items-center gap-3 px-0.5">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={outVol}
                    onPointerDown={() => {
                      draggingOutRef.current = true;
                    }}
                    onChange={(e) => {
                      const val = Number(e.target.value);
                      setOutVol(val);
                      scheduleCommit(val, null);
                    }}
                    onPointerUp={() => {
                      draggingOutRef.current = false;
                      scheduleCommit(outVol, null, true);
                    }}
                    onKeyUp={() => scheduleCommit(outVol, null, true)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-cyan-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras ? `${outVol}%` : "--%"}
                  </b>
                </div>
              </div>

              <div className="pp-card gap-2.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    {extras?.audio.input_muted ? (
                      <MicOff className="w-3.5 h-3.5 text-rose-400" />
                    ) : (
                      <Mic className="w-3.5 h-3.5 text-[var(--good)]" />
                    )}
                    Input Gain &amp; Mic Kill-Switch
                  </span>
                  <div className="flex items-center gap-1.5">
                    {micMuteShortcutLabel && (
                      <span className="text-[9px] font-mono px-1 py-0.5 rounded bg-[var(--rail)] border border-[var(--card-border)] text-[var(--sec)]">
                        {micMuteShortcutLabel}
                      </span>
                    )}
                    <button
                      type="button"
                      onClick={() => void handleToggleMicMute()}
                      className={`pp-btn ${extras?.audio.input_muted ? "pp-btn--danger" : ""}`}
                    >
                      {extras?.audio.input_muted ? "Mic Muted" : "Mute Mic"}
                    </button>
                  </div>
                </div>

                <div className="flex items-center gap-3 px-0.5">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={inVol}
                    onPointerDown={() => {
                      draggingInRef.current = true;
                    }}
                    onChange={(e) => {
                      const val = Number(e.target.value);
                      setInVol(val);
                      scheduleCommit(null, val);
                    }}
                    onPointerUp={() => {
                      draggingInRef.current = false;
                      scheduleCommit(null, inVol, true);
                    }}
                    onKeyUp={() => scheduleCommit(null, inVol, true)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-emerald-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras ? `${inVol}%` : "--%"}
                  </b>
                </div>
              </div>

              {extras && extras.audio.output_devices.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Speaker className="w-3 h-3 text-[var(--acc)]" />
                      Output Device
                    </span>
                    <em>{extras.audio.output_devices.length}</em>
                  </div>
                  <div className="flex flex-col gap-1">
                    {extras.audio.output_devices.map((dev) => (
                      <button
                        key={dev.id}
                        type="button"
                        onClick={() => void handleSetAudioDevice(dev.id, "output")}
                        className={`flex items-center justify-between gap-2 text-[10.5px] w-full text-left px-2.5 py-1.5 rounded-lg border transition-colors cursor-pointer ${
                          dev.is_default_output
                            ? "bg-[var(--surface)] border-[var(--acc)]/40 text-[var(--ink)] font-semibold"
                            : "border-transparent hover:bg-white/5 text-[var(--sec)]"
                        }`}
                      >
                        <span className="truncate">{dev.name}</span>
                        {dev.is_default_output && <Check className="w-3 h-3 text-[var(--acc)] shrink-0" />}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {extras && extras.audio.input_devices.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Mic className="w-3 h-3 text-[var(--good)]" />
                      Input Device
                    </span>
                    <em>{extras.audio.input_devices.length}</em>
                  </div>
                  <div className="flex flex-col gap-1">
                    {extras.audio.input_devices.map((dev) => (
                      <button
                        key={dev.id}
                        type="button"
                        onClick={() => void handleSetAudioDevice(dev.id, "input")}
                        className={`flex items-center justify-between gap-2 text-[10.5px] w-full text-left px-2.5 py-1.5 rounded-lg border transition-colors cursor-pointer ${
                          dev.is_default_input
                            ? "bg-[var(--surface)] border-[var(--good)]/40 text-[var(--ink)] font-semibold"
                            : "border-transparent hover:bg-white/5 text-[var(--sec)]"
                        }`}
                      >
                        <span className="truncate">{dev.name}</span>
                        {dev.is_default_input && <Check className="w-3 h-3 text-[var(--good)] shrink-0" />}
                      </button>
                    ))}
                  </div>
                </div>
              )}
    </>
  );
};
