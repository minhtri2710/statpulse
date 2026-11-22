import { Box, Check, Copy, Cpu, RotateCw, Sparkles, Terminal } from "lucide-react";
import { formatMemory } from "../../services/format";
import type { AppTab } from "../../navigation";
import type { SystemExtrasSnapshot } from "../../types";

interface AgentsViewProps {
  extras: SystemExtrasSnapshot | null;
  unloadingModel: string | null;
  controllingCtr: string | null;
  copiedMcpConfig: boolean | null;
  handleUnloadOllama: (name: string) => Promise<void>;
  handleControlContainer: (id: string, action: "stop" | "restart") => Promise<void>;
  handleCopyMcpConfig: () => Promise<void>;
  setActiveTab: (tab: AppTab) => void;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
}

export const AgentsView = ({
  extras,
  unloadingModel,
  controllingCtr,
  copiedMcpConfig,
  handleUnloadOllama,
  handleControlContainer,
  handleCopyMcpConfig,
  setActiveTab,
  handleTerminateProcess,
}: AgentsViewProps) => {
  const agents = extras?.agents ?? [];

  return (
    <>
              <div className="pp-card gap-1">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Sparkles className="w-3 h-3 text-[var(--acc)]" />
                    AI Agent Sessions
                  </span>
                  <em>{agents.length} running</em>
                </div>
                <p className="text-[10px] text-[var(--sec)]">
                  Tracks Claude Code, Codex, Cursor, Antigravity, and Ollama sessions along with child processes and opened ports.
                </p>
              </div>

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
                  <code className="text-[var(--acc)]">kill_port</code> &amp;{" "}
                  <code className="text-[var(--acc)]">unload_ollama_model</code>.
                </div>
              </div>
              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Cpu className="w-3 h-3 text-[var(--acc)]" />
                    Local LLM &amp; Ollama VRAM
                  </span>
                  <em>{extras?.ollama_models.length ?? 0} loaded</em>
                </div>
                {!extras || extras.ollama_models.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">
                    No local LLM models currently resident in Unified Memory.
                  </div>
                ) : (
                  <div className="flex flex-col gap-1">
                    {extras.ollama_models.map((m) => (
                      <div key={m.id} className="pp-kv text-[10px]">
                        <div className="min-w-0">
                          <div className="font-mono font-semibold text-[var(--ink)] truncate">{m.name}</div>
                          <div className="text-[9px] text-[var(--sec)] font-mono">
                            {m.size_gb.toFixed(1)} GB · {m.processor} · {m.until}
                          </div>
                        </div>
                        <button
                          type="button"
                          disabled={unloadingModel === m.name}
                          onClick={() => void handleUnloadOllama(m.name)}
                          className="pp-btn pp-btn--danger shrink-0"
                        >
                          {unloadingModel === m.name ? "..." : "Unload"}
                        </button>
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
                    <div key={ag.agent_name} className="pp-card gap-1.5">
                      <div className="flex items-center justify-between gap-2">
                        <div className="min-w-0">
                          <div className="flex items-center gap-1.5">
                            <b className="text-xs text-[var(--ink)]">{ag.agent_name}</b>
                            <span className="text-[9.5px] font-mono text-[var(--sec)]">
                              up {ag.uptime} · {ag.process_count} proc
                            </span>
                          </div>
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
                  ))}
                </div>
              )}
    </>
  );
};
