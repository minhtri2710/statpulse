import { useMemo } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Box, Check, Code2, Copy, ExternalLink, FolderOpen, Globe, RefreshCw, Search, Square, Terminal } from "lucide-react";
import type { ListeningPort, PortHttpProbe, SshTunnelItem } from "../../types";

interface PortsViewProps {
  ports: ListeningPort[];
  tunnels?: SshTunnelItem[];
  portSearch: string;
  setPortSearch: (q: string) => void;
  httpProbes: Record<number, PortHttpProbe>;
  probingPorts: boolean;
  handleProbeHttpPorts: (ports: number[]) => Promise<void>;
  copiedPort: number | null;
  handleCopyPortUrl: (port: number) => Promise<void>;
  portsError: string | null;
  stackFeedback: { kind: "ok" | "err"; text: string } | null;
  refreshPorts: () => Promise<void>;
  handleStopProjectStack: (projectPath: string, pids: number[]) => Promise<void>;
  handleRevealInFinder: (path: string) => Promise<void>;
  handleOpenInEditor: (path: string) => Promise<void>;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
}

export const PortsView = ({
  ports,
  tunnels = [],
  portSearch,
  setPortSearch,
  httpProbes,
  probingPorts,
  handleProbeHttpPorts,
  copiedPort,
  handleCopyPortUrl,
  portsError,
  stackFeedback,
  refreshPorts,
  handleStopProjectStack,
  handleRevealInFinder,
  handleOpenInEditor,
  handleTerminateProcess,
}: PortsViewProps) => {
  const filteredPorts = useMemo(() => {
    const q = portSearch.trim().toLowerCase();
    if (!q) return ports;
    return ports.filter(
      (p) =>
        String(p.port).includes(q) ||
        p.process_name.toLowerCase().includes(q) ||
        p.bind_address.toLowerCase().includes(q) ||
        (p.project_path?.toLowerCase().includes(q) ?? false) ||
        (p.launcher?.toLowerCase().includes(q) ?? false),
    );
  }, [ports, portSearch]);

  const exposedPorts = useMemo(() => ports.filter((p) => p.is_exposed), [ports]);

  const projectStacks = useMemo(() => {
    const grouped = new Map<string, { path: string; ports: ListeningPort[]; pids: number[] }>();
    for (const p of ports) {
      if (!p.project_path) continue;
      const existing = grouped.get(p.project_path) ?? { path: p.project_path, ports: [], pids: [] };
      existing.ports.push(p);
      if (!existing.pids.includes(p.pid)) {
        existing.pids.push(p.pid);
      }
      grouped.set(p.project_path, existing);
    }
    return Array.from(grouped.values()).filter((s) => s.ports.length >= 1);
  }, [ports]);

  return (
    <>
              <div className="flex items-center gap-1.5">
                <div className="pp-search flex-1">
                  <Search className="w-3.5 h-3.5 shrink-0" />
                  <input
                    type="text"
                    value={portSearch}
                    onChange={(e) => setPortSearch(e.target.value)}
                    placeholder="Filter port, process, or bind"
                    aria-label="Filter listening ports"
                  />
                </div>
                <button
                  type="button"
                  disabled={probingPorts || filteredPorts.length === 0}
                  onClick={() => void handleProbeHttpPorts(filteredPorts.slice(0, 20).map((p) => p.port))}
                  className="pp-btn py-1.5 px-2 flex items-center gap-1 disabled:opacity-40"
                  title="Probe HTTP status & <title> on listening localhost ports"
                >
                  <Globe className={`w-3 h-3 ${probingPorts ? "animate-spin" : ""}`} />
                  HTTP
                </button>
                <button
                  type="button"
                  onClick={() => void refreshPorts()}
                  className="pp-btn py-1.5 px-2 flex items-center gap-1"
                  title="Refresh listening ports"
                >
                  <RefreshCw className="w-3 h-3" />
                </button>
              </div>

              {portsError && (
                <div className="px-2.5 py-1.5 rounded-lg bg-rose-500/10 border border-rose-500/20 text-[10px] text-rose-200">
                  {portsError}
                </div>
              )}

              {stackFeedback && (
                <div
                  className={`px-2.5 py-1.5 rounded-lg text-[10px] border ${
                    stackFeedback.kind === "err"
                      ? "bg-rose-500/10 border-rose-500/20 text-rose-200"
                      : "bg-emerald-500/10 border-emerald-500/20 text-emerald-200"
                  }`}
                >
                  {stackFeedback.text}
                </div>
              )}

              {exposedPorts.length > 0 && (
                <div className="pp-card pp-card--attn text-[10px]">
                  <div className="pp-attn">
                    <span className="pp-dot pp-dot--warn" />
                    <span className="text-[var(--ter)]">
                      <b className="text-[var(--ink)]">{exposedPorts.length} port{exposedPorts.length > 1 ? "s" : ""}</b>{" "}
                      bound to all interfaces (<code className="text-[var(--acc)]">0.0.0.0 / *</code>) — reachable on your LAN.
                    </span>
                  </div>
                </div>
              )}

              {tunnels.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Terminal className="w-3 h-3 text-[var(--acc)]" />
                      Active Tunnels &amp; Forwards
                    </span>
                    <em>{tunnels.length}</em>
                  </div>
                  {tunnels.map((t) => (
                    <div key={t.pid} className="pp-kv text-[10px]">
                      <span className="truncate font-mono text-[var(--ter)]" title={`${t.kind} ${t.spec} ${t.target}`}>
                        <b className="text-[var(--acc)]">{t.kind}</b> {t.spec} → {t.target || `pid ${t.pid}`}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleTerminateProcess(t.pid, `${t.kind} ${t.spec}`)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Stop
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {projectStacks.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span>Project Stacks (by workspace)</span>
                    <em>{projectStacks.length} active</em>
                  </div>
                  {projectStacks.slice(0, 4).map((stack) => (
                    <div key={stack.path} className="pp-kv text-[10px]">
                      <div className="min-w-0">
                        <div className="font-mono text-[var(--ink)] truncate" title={stack.path}>
                          {stack.path.split("/").slice(-2).join("/")}
                        </div>
                        <div className="font-mono text-[9px] text-[var(--acc)]">
                          {stack.ports.map((p) => `:${p.port}`).join(" ")}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => void handleStopProjectStack(stack.path, stack.pids)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Stop Stack
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {filteredPorts.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  {portSearch
                    ? `No listening TCP ports matching "${portSearch}".`
                    : "No listening TCP ports found."}
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {filteredPorts.map((item) => (
                    <div key={`${item.port}-${item.pid}`} className="pp-card gap-1">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-1.5 min-w-0">
                          <b className="font-mono text-xs text-[var(--acc)]">:{item.port}</b>
                          <span className="font-semibold text-[var(--ink)] truncate">
                            {item.process_name}
                          </span>
                          <span className="text-[9.5px] font-mono text-[var(--sec)]">
                            pid {item.pid}
                          </span>
                          {item.launcher && (
                            <span className="px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/20 text-[9px] text-cyan-300 shrink-0">
                              {item.launcher}
                            </span>
                          )}
                          {item.container_name && (
                            <span className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-indigo-500/15 border border-indigo-500/30 text-[9px] font-mono text-indigo-300 shrink-0">
                              <Box className="w-2.5 h-2.5" />
                              {item.container_name}
                            </span>
                          )}
                          {httpProbes[item.port]?.status_code && (
                            <span
                              className="px-1.5 py-0.5 rounded bg-emerald-500/10 border border-emerald-500/25 text-[9px] font-mono text-emerald-300 truncate max-w-[140px]"
                              title={httpProbes[item.port].title ?? undefined}
                            >
                              {httpProbes[item.port].status_code}
                              {httpProbes[item.port].title ? ` · ${httpProbes[item.port].title}` : ""}
                            </span>
                          )}
                        </div>

                        <span
                          className={`px-1.5 py-0.5 rounded text-[9px] font-mono shrink-0 ${
                            item.is_exposed
                              ? "bg-amber-500/15 text-amber-300 border border-amber-500/30"
                              : "bg-white/5 text-[var(--sec)] border border-white/10"
                          }`}
                        >
                          {item.is_exposed ? `${item.bind_address} · exposed` : item.bind_address}
                        </span>
                      </div>

                      {item.project_path && (
                        <div
                          className="text-[9.5px] font-mono text-[var(--sec)] truncate"
                          title={item.project_path}
                        >
                          📁 {item.project_path}
                        </div>
                      )}

                      <div className="flex items-center justify-end gap-1 pt-0.5">
                        {item.project_path && (
                          <>
                            <button
                              type="button"
                              onClick={() => void handleOpenInEditor(item.project_path!)}
                              className="pp-btn inline-flex items-center gap-1"
                              title="Open project folder in Cursor / VS Code / Zed"
                            >
                              <Code2 className="w-2.5 h-2.5" />
                              IDE
                            </button>
                            <button
                              type="button"
                              onClick={() => void handleRevealInFinder(item.project_path!)}
                              className="pp-btn inline-flex items-center gap-1"
                              title="Reveal project folder in Finder"
                            >
                              <FolderOpen className="w-2.5 h-2.5" />
                              Folder
                            </button>
                          </>
                        )}
                        <button
                          type="button"
                          onClick={() => void openUrl(`http://localhost:${item.port}`)}
                          className="pp-btn inline-flex items-center gap-1"
                        >
                          <ExternalLink className="w-2.5 h-2.5" />
                          Open
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleCopyPortUrl(item.port)}
                          className="pp-btn inline-flex items-center gap-1"
                        >
                          {copiedPort === item.port ? (
                            <>
                              <Check className="w-2.5 h-2.5 text-[var(--good)]" />
                              Copied
                            </>
                          ) : (
                            <>
                              <Copy className="w-2.5 h-2.5" />
                              Copy
                            </>
                          )}
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void handleTerminateProcess(item.pid, `${item.process_name} :${item.port}`)
                          }
                          className="pp-btn pp-btn--danger inline-flex items-center gap-1"
                        >
                          <Square className="w-2.5 h-2.5" />
                          Stop
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
    </>
  );
};
