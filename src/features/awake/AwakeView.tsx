import { Activity, Coffee, RefreshCw } from "lucide-react";
import { formatDuration } from "../../services/format";
import type { AwakeStatus, KeepAwakeRules } from "../../types";

interface AwakeViewProps {
  awakeStatus: AwakeStatus;
  awakeError: string | null;
  handleSetKeepAwake: (enabled: boolean, durationSeconds: number | null) => Promise<void>;
  handleToggleKeepAwakeRule: (key: keyof KeepAwakeRules) => Promise<void>;
  refreshAwake: () => Promise<void>;
}

export const AwakeView = ({
  awakeStatus,
  awakeError,
  handleSetKeepAwake,
  handleToggleKeepAwakeRule,
  refreshAwake,
}: AwakeViewProps) => {

  return (
    <>
              {awakeError && (
                <div className="px-2.5 py-1.5 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  {awakeError}
                </div>
              )}

              <div className="pp-card gap-2">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1.5">
                    <Coffee className="w-3.5 h-3.5 text-[var(--acc)]" />
                    Keep awake
                  </span>
                  <em>
                    {awakeStatus.keep_awake_active
                      ? awakeStatus.remaining_seconds !== null
                        ? `On · ${formatDuration(awakeStatus.remaining_seconds)} left`
                        : "On · Always"
                      : "Off"}
                  </em>
                </div>

                <div className="grid grid-cols-5 gap-1">
                  {[
                    { label: "30m", secs: 1800 },
                    { label: "1h", secs: 3600 },
                    { label: "2h", secs: 7200 },
                    { label: "Always", secs: null },
                  ].map((opt) => (
                    <button
                      key={opt.label}
                      type="button"
                      onClick={() => void handleSetKeepAwake(true, opt.secs)}
                      className={`pp-btn py-1 ${
                        awakeStatus.keep_awake_active &&
                        ((opt.secs === null && awakeStatus.remaining_seconds === null) ||
                          (opt.secs !== null && awakeStatus.remaining_seconds !== null))
                          ? "pp-btn--active"
                          : ""
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                  <button
                    type="button"
                    onClick={() => void handleSetKeepAwake(false, null)}
                    className={`pp-btn py-1 ${!awakeStatus.keep_awake_active ? "pp-btn--active" : ""}`}
                  >
                    Off
                  </button>
                </div>

                <div className="flex items-center justify-between gap-2 pt-1 border-t border-white/5">
                  <span className="text-[10px] text-[var(--sec)]">
                    <Activity className="w-3 h-3 inline mr-1 text-[var(--acc)]" />
                    {awakeStatus.auto_rule_reason ?? "Smart Auto Rules"}
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_on_ac")}
                      className={`pp-btn ${awakeStatus.rules.while_on_ac ? "pp-btn--active" : ""}`}
                    >
                      On AC
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_ports_active")}
                      className={`pp-btn ${awakeStatus.rules.while_ports_active ? "pp-btn--active" : ""}`}
                    >
                      Dev Ports
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleToggleKeepAwakeRule("while_ai_agents_active")}
                      className={`pp-btn ${awakeStatus.rules.while_ai_agents_active ? "pp-btn--active" : ""}`}
                    >
                      AI Agents
                    </button>
                  </div>
                </div>

                <p className="text-[10px] text-[var(--sec)]">
                  Uses macOS <code className="text-[var(--ter)]">caffeinate -di</code> to prevent idle system &amp; display sleep while StatPulse is running.
                </p>
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span>Why won&apos;t my Mac sleep?</span>
                  <button
                    type="button"
                    onClick={() => void refreshAwake()}
                    className="inline-flex items-center gap-1 text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    <RefreshCw className="w-2.5 h-2.5" />
                    {awakeStatus.assertions.length} active
                  </button>
                </div>

                {awakeStatus.assertions.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)] py-1">
                    No active power assertions blocking sleep.
                  </div>
                ) : (
                  <div className="flex flex-col gap-1.5">
                    {awakeStatus.assertions.map((a, idx) => (
                      <div
                        key={`${a.pid}-${idx}`}
                        className="px-2.5 py-1.5 rounded-lg bg-[var(--surface)] border border-[var(--card-border)] flex flex-col gap-0.5"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="font-semibold text-[var(--ink)] truncate">
                            {a.process_name}{" "}
                            <span className="text-[9.5px] font-mono text-[var(--sec)]">
                              pid {a.pid}
                            </span>
                          </span>
                          <span className="text-[9px] font-mono text-[var(--acc)] shrink-0">
                            {a.assertion_type}
                          </span>
                        </div>
                        <span className="text-[10px] text-[var(--sec)] truncate" title={a.reason}>
                          {a.reason}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
    </>
  );
};
