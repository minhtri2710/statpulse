import { Camera, Mic, Volume2, VolumeX } from "lucide-react";
import type { SystemExtrasSnapshot } from "../../types";

interface VolumeViewProps {
  extras: SystemExtrasSnapshot | null;
  handleUpdateAudio: (
    outputVolume: number | null,
    inputVolume: number | null,
    muted: boolean | null,
  ) => Promise<void>;
}

export const VolumeView = ({
  extras,
  handleUpdateAudio,
}: VolumeViewProps) => {

  return (
    <>
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

                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={extras?.audio.output_volume ?? 50}
                    onChange={(e) => void handleUpdateAudio(Number(e.target.value), null, null)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-cyan-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras?.audio.output_volume ?? "--"}%
                  </b>
                </div>
              </div>

              <div className="pp-card gap-2.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Mic className="w-3.5 h-3.5 text-[var(--good)]" />
                    Input Gain
                  </span>
                  <em>{extras?.audio.input_volume ?? "--"}%</em>
                </div>

                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={100}
                    value={extras?.audio.input_volume ?? 50}
                    onChange={(e) => void handleUpdateAudio(null, Number(e.target.value), null)}
                    className="flex-1 h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-emerald-400"
                  />
                  <b className="font-mono text-xs w-9 text-right">
                    {extras?.audio.input_volume ?? "--"}%
                  </b>
                </div>
              </div>
    </>
  );
};
