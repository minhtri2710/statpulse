import { useState } from "react";
import { Box, Check, Code2, Copy, Cpu, GitBranch, RotateCw, Sparkles, Terminal } from "lucide-react";
import { formatMemory } from "../../services/format";
import type { AppTab } from "../../navigation";
import type { AlertPreferences, SystemExtrasSnapshot } from "../../types";
import type { ActionFeedback } from "../processes/useProcessList";
import { useAgentTools } from "./useAgentTools";

interface AgentsViewProps {
  extras: SystemExtrasSnapshot | null;
  setExtras: React.Dispatch<React.SetStateAction<SystemExtrasSnapshot | null>>;
  alerts: AlertPreferences;
  onUpdateAlerts: (next: AlertPreferences) => Promise<void>;
  setActiveTab: (tab: AppTab) => void;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
  handleOpenInEditor: (path: string) => Promise<unknown>;
}

const formatTokens = (n: number): string => {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
};

export const AgentsView = ({
  extras,
  setExtras,
  alerts,
  onUpdateAlerts,
  setActiveTab,
  handleTerminateProcess,
  handleOpenInEditor,
}: AgentsViewProps) => {
  const [agentFeedback, setAgentFeedback] = useState<ActionFeedback | null>(null);
  const {
    unloadingModel,
    controllingCtr,
    copiedMcpConfig,
    unloadOllama: handleUnloadOllama,
    controlContainer: handleControlContainer,
    copyMcpConfig: handleCopyMcpConfig,
  } = useAgentTools(setExtras, setAgentFeedback);
  const agents = extras?.agents ?? [];
  const tokenUsage = extras?.ai_token_usage;

  return (
    <>
              {agentFeedback && (
                <div
                  className={`pp-pill text-[10px] justify-between ${
                    agentFeedback.kind === "err" ? "pp-pill--bad" : "pp-pill--good"
                  }`}
                >
                  <span className="truncate">{agentFeedback.text}</span>
                  <button
                    type="button"
                    className="ml-2 opacity-70 hover:opacity-100 cursor-pointer"
                    onClick={() => setAgentFeedback(null)}
                  >
                    ×
                  </button>
                </div>
              )}
              <div className="pp-card gap-1">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Sparkles className="w-3 h-3 text-[var(--acc)]" />
                    AI Agent Sessions
                  </span>
                  <em>{agents.length} running</em>
                </div>
                <p className="text-[10px] text-[var(--sec)]">
                  Tracks Claude Code, Codex, Cursor, Antigravity, Windsurf, Aider, OpenCode, and Ollama sessions along with child processes and opened ports.
                </p>
              </div>

              {tokenUsage && (tokenUsage.window_24h_tokens > 0 || tokenUsage.active_sessions_24h > 0) && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Sparkles className="w-3 h-3 text-[var(--acc)]" />
                      Local Token &amp; Cost Tracker
                    </span>
                    <em className="font-mono text-[var(--acc)]">
                      ~${tokenUsage.estimated_cost_usd_24h.toFixed(2)} (24h)
                    </em>
                  </div>
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">1h Burn</span>
                      <b className="text-xs font-mono text-[var(--warn)]">
                        ${tokenUsage.estimated_cost_usd_1h.toFixed(2)}/h
                      </b>
                      <small className="text-[9px] text-[var(--sec)]">
                        {formatTokens(tokenUsage.window_1h_tokens)}
                      </small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">5h Window</span>
                      <b className="text-xs font-mono">{formatTokens(tokenUsage.window_5h_tokens)}</b>
                      <small className="text-[9px] text-[var(--sec)]">tok</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">24h Total</span>
                      <b className="text-xs font-mono">{formatTokens(tokenUsage.window_24h_tokens)}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        {tokenUsage.active_sessions_24h} sess
                      </small>
                    </div>
                  </div>
                  <div className="flex items-center justify-between gap-2 pt-1 border-t border-white/5 text-[9.5px]">
                    <span className="text-[var(--sec)]">
                      Guardrails:
                      <select
                        value={alerts.ai_burn_rate_usd_per_hour}
                        onChange={(e) =>
                          void onUpdateAlerts({
                            ...alerts,
                            ai_burn_rate_usd_per_hour: Number(e.target.value),
                          })
                        }
                        className="ml-1.5 bg-[var(--rail)] border border-[var(--card-border)] rounded px-1 py-0.5 font-mono text-[var(--ink)]"
                      >
                        <option value={0}>Budget Off</option>
                        <option value={5}>$5/hr Alert</option>
                        <option value={10}>$10/hr Alert</option>
                        <option value={25}>$25/hr Alert</option>
                      </select>
                    </span>
                    <button
                      type="button"
                      onClick={() =>
                        void onUpdateAlerts({
                          ...alerts,
                          notify_agent_runaway: !alerts.notify_agent_runaway,
                        })
                      }
                      className={`pp-btn ${alerts.notify_agent_runaway ? "pp-btn--active" : ""}`}
                    >
                      Runaway Guard {alerts.notify_agent_runaway ? "On" : "Off"}
                    </button>
                  </div>
                </div>
              )}

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Terminal className="w-3 h-3 text-[var(--acc)]" />
                    Built-in CLI &amp; MCP Server
                  </span>
                  <button
                    type="button"
                    onClick={() => void handleCopyMcpConfig()}
                    className="pp-btn inline-flex items-center gap-1"
                  >
                    {copiedMcpConfig ? (
                      <>
                        <Check className="w-2.5 h-2.5 text-[var(--good)]" />
                        Copied
                      </>
                    ) : (
                      <>
                        <Copy className="w-2.5 h-2.5" />
                        Copy MCP Config
                      </>
                    )}
                  </button>
                </div>
                <div className="text-[9.5px] font-mono text-[var(--sec)] leading-snug">
                  Run <code className="text-[var(--ter)]">statpulse --mcp</code> to expose{" "}
                  <code className="text-[var(--acc)]">get_system_status</code>,{" "}
                  <code className="text-[var(--acc)]">list_ports</code>,{" "}
                  <code className="text-[var(--acc)]">kill_port</code>,{" "}
                  <code className="text-[var(--acc)]">set_thermal_mode</code> &amp;{" "}
                  <code className="text-[var(--acc)]">get_ai_token_usage</code>.
                </div>
              </div>
              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Cpu className="w-3 h-3 text-[var(--acc)]" />
                    Local LLM &amp; Unified Memory
                  </span>
                  <em>{extras?.ollama_models.length ?? 0} loaded</em>
                </div>
                {extras && extras.silicon_power.dram_bw_gbps > 0 && (
                  <div className="pp-kv text-[9.5px] font-mono border-b border-white/5 pb-1">
                    <span className="text-[var(--sec)]">
                      UMA Bus: <b className="text-[var(--acc)]">{extras.silicon_power.dram_bw_gbps.toFixed(1)}</b>
                      {extras.silicon_power.dram_peak_gbps > 0
                        ? ` / ${extras.silicon_power.dram_peak_gbps.toFixed(0)} GB/s`
                        : " GB/s"}
                    </span>
                    <span className="text-[var(--sec)]">
                      Headroom:{" "}
                      <b className="text-[var(--ink)]">
                        {Math.max(extras.memory.total_gb - extras.memory.used_gb, 0).toFixed(1)}G free
                      </b>
                    </span>
                  </div>
                )}
                {!extras || extras.ollama_models.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">
                    No local LLM models currently active (Ollama, LM Studio, llama.cpp, MLX).
                  </div>
                ) : (
                  <div className="flex flex-col gap-1">
                    {extras.ollama_models.map((m) => (
                      <div key={m.id} className="pp-kv text-[10px]">
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5 min-w-0">
                            <span className="font-mono font-semibold text-[var(--ink)] truncate">{m.name}</span>
                            <span className="text-[8.5px] font-mono px-1 py-0.5 rounded bg-[var(--rail)] border border-[var(--card-border)] text-[var(--acc)] shrink-0">
                              {m.runtime || "Ollama"}
                            </span>
                          </div>
                          <div className="text-[9px] text-[var(--sec)] font-mono">
                            {m.size_gb > 0 ? `${m.size_gb.toFixed(1)} GB · ` : ""}
                            {m.processor} · {m.until}
                          </div>
                        </div>
                        {(!m.runtime || m.runtime === "Ollama") && (
                          <button
                            type="button"
                            disabled={unloadingModel === m.name}
                            onClick={() => void handleUnloadOllama(m.name)}
                            className="pp-btn pp-btn--danger shrink-0"
                          >
                            {unloadingModel === m.name ? "..." : "Unload"}
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
              {extras && extras.containers.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Box className="w-3 h-3 text-[var(--acc)]" />
                      Containers ({extras.containers[0]?.runtime})
                    </span>
                    <em>{extras.containers.length} running</em>
                  </div>
                  <div className="flex flex-col gap-1">
                    {extras.containers.slice(0, 6).map((c) => (
                      <div key={c.id} className="pp-kv text-[10px]">
                        <div className="min-w-0">
                          <div className="font-mono font-semibold text-[var(--ink)] truncate">{c.name}</div>
                          <div className="text-[9px] text-[var(--sec)] truncate">
                            {c.image} · {c.status}
                          </div>
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          <button
                            type="button"
                            disabled={controllingCtr === c.id}
                            onClick={() => void handleControlContainer(c.id, "restart")}
                            className="pp-btn inline-flex items-center"
                            title="Restart container"
                          >
                            <RotateCw className="w-2.5 h-2.5" />
                          </button>
                          <button
                            type="button"
                            disabled={controllingCtr === c.id}
                            onClick={() => void handleControlContainer(c.id, "stop")}
                            className="pp-btn pp-btn--danger"
                          >
                            Stop
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {agents.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  No active AI coding agent processes detected right now.
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {agents.map((ag) => (
                    <div key={ag.session_id || `${ag.agent_name}-${ag.primary_pid}`} className="pp-card gap-1.5">
                      <div className="flex items-center justify-between gap-2">
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5 flex-wrap">
                            <b className="text-xs text-[var(--ink)]">{ag.agent_name}</b>
                            <span
                              className={`px-1.5 py-0.2 rounded text-[8.5px] font-mono border ${
                                ag.status === "working"
                                  ? "bg-emerald-500/15 border-emerald-500/30 text-emerald-300"
                                  : "bg-white/5 border-white/10 text-[var(--sec)]"
                              }`}
                            >
                              {ag.status === "working" ? "Working" : "Idle"}
                            </span>
                            {ag.git_branch && (
                              <span className="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded bg-cyan-500/10 border border-cyan-500/20 text-[8.5px] font-mono text-cyan-300">
                                <GitBranch className="w-2.5 h-2.5" />
                                {ag.git_branch}
                              </span>
                            )}
                            <span className="text-[9.5px] font-mono text-[var(--sec)]">
                              up {ag.uptime} · {ag.process_count} proc
                            </span>
                          </div>
                          {ag.project_path && (
                            <div
                              className="text-[9.5px] font-mono text-[var(--ter)] truncate mt-0.5"
                              title={ag.project_path}
                            >
                              📁 {ag.project_path.split("/").slice(-2).join("/")}
                            </div>
                          )}
                          {ag.children.length > 0 && (
                            <div className="text-[9.5px] font-mono text-[var(--sec)] truncate mt-0.5">
                              Children: {ag.children.join(", ")}
                            </div>
                          )}
                        </div>

                        <div className="text-right font-mono shrink-0">
                          <div className="text-[11px] font-bold text-[var(--acc)]">
                            {ag.cpu_percent.toFixed(1)}%
                          </div>
                          <div className="text-[9.5px] text-[var(--sec)]">
                            {formatMemory(ag.memory_mb)}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center justify-between gap-2 pt-0.5">
                        <div className="flex items-center gap-1 flex-wrap">
                          {ag.ports.map((p) => (
                            <button
                              key={p}
                              type="button"
                              onClick={() => setActiveTab("ports")}
                              className="px-1.5 py-0.5 rounded bg-[var(--surface)] border border-[var(--card-border)] font-mono text-[9.5px] text-[var(--acc)]"
                            >
                              :{p}
                            </button>
                          ))}
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          {ag.project_path && (
                            <button
                              type="button"
                              onClick={() => void handleOpenInEditor(ag.project_path!)}
                              className="pp-btn inline-flex items-center gap-1"
                              title="Open workspace in IDE / Terminal"
                            >
                              <Code2 className="w-2.5 h-2.5" />
                              IDE
                            </button>
                          )}
                          <button
                            type="button"
                            disabled={ag.primary_pid <= 1}
                            onClick={() => void handleTerminateProcess(ag.primary_pid, ag.agent_name)}
                            className="pp-btn pp-btn--danger shrink-0 disabled:opacity-40"
                          >
                            Stop Session
                          </button>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
    </>
  );
};
