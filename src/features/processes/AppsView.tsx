import { useMemo } from "react";
import { ChevronRight, FolderOpen, Search } from "lucide-react";
import { formatMemory } from "../../services/format";
import type { ActionFeedback } from "../useTransient";
import type {
  HardwareTelemetrySnapshot,
  ProcessInspection,
  ProcessReading,
  SystemExtrasSnapshot,
} from "../../types";

interface AppsViewProps {
  telemetry: HardwareTelemetrySnapshot | null;
  processes: ProcessReading[];
  extras: SystemExtrasSnapshot | null;
  packageWatts: number;
  appSearch: string;
  setAppSearch: (q: string) => void;
  appSort: "cpu" | "mem" | "energy";
  setAppSort: (sort: "cpu" | "mem" | "energy") => void;
  expandedAppName: string | null;
  setExpandedAppName: (name: string | null) => void;
  processInspections: Record<number, ProcessInspection>;
  inspectingPid: number | null;
  actionFeedback: ActionFeedback | null;
  setActionFeedback: (feedback: ActionFeedback | null) => void;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
  handleTogglePauseProcess: (pid: number, label: string, paused: boolean) => Promise<unknown>;
  handleInspectProcess: (pid: number) => Promise<void>;
  handleCleanupOrphans: () => Promise<void>;
  handleRevealInFinder: (path: string) => Promise<void>;
}

export const AppsView = ({
  telemetry,
  processes,
  extras,
  packageWatts,
  appSearch,
  setAppSearch,
  appSort,
  setAppSort,
  expandedAppName,
  setExpandedAppName,
  processInspections,
  inspectingPid,
  actionFeedback,
  setActionFeedback,
  handleTerminateProcess,
  handleTogglePauseProcess,
  handleInspectProcess,
  handleCleanupOrphans,
  handleRevealInFinder,
}: AppsViewProps) => {
  const orphanedServers = extras?.orphaned_servers ?? [];
  const totalProcCpu = useMemo(
    () => Math.max(processes.reduce((s, p) => s + p.cpu_percent, 0), 1),
    [processes],
  );
  const filteredProcesses = useMemo(() => {
    const q = appSearch.trim().toLowerCase();
    const list = q ? processes.filter((p) => p.name.toLowerCase().includes(q)) : [...processes];
    if (appSort === "mem") {
      return [...list].sort((a, b) => b.memory_mb - a.memory_mb);
    }
    if (appSort === "energy") {
      return [...list].sort(
        (a, b) => b.cpu_percent * 2 + b.memory_mb * 0.01 - (a.cpu_percent * 2 + a.memory_mb * 0.01),
      );
    }
    return [...list].sort((a, b) => b.cpu_percent - a.cpu_percent);
  }, [processes, appSearch, appSort]);

  return (
    <>
              <div className="pp-search">
                <Search className="w-3.5 h-3.5 shrink-0" />
                <input
                  type="text"
                  value={appSearch}
                  onChange={(e) => setAppSearch(e.target.value)}
                  placeholder="Search apps"
                  aria-label="Search apps"
                />
                {appSearch && (
                  <button
                    type="button"
                    onClick={() => setAppSearch("")}
                    className="text-[9.5px] text-[var(--sec)] hover:text-[var(--ink)]"
                  >
                    Clear
                  </button>
                )}
              </div>

              {actionFeedback && (
                <div
                  className={`px-2.5 py-1.5 rounded-lg border text-[10px] font-mono flex items-center justify-between gap-2 ${
                    actionFeedback.kind === "err"
                      ? "bg-rose-500/10 border-rose-500/30 text-rose-200"
                      : "bg-emerald-500/10 border-emerald-500/30 text-emerald-200"
                  }`}
                >
                  <span className="truncate">{actionFeedback.text}</span>
                  <button
                    type="button"
                    onClick={() => setActionFeedback(null)}
                    className="text-[9px] underline shrink-0"
                  >
                    Dismiss
                  </button>
                </div>
              )}

              {orphanedServers.length > 0 && (
                <div className="pp-card pp-card--attn gap-1.5">
                  <div className="pp-card__head">
                    <span>Orphaned Dev Servers (ppid 1)</span>
                    <button
                      type="button"
                      onClick={() => void handleCleanupOrphans()}
                      className="pp-btn pp-btn--danger"
                    >
                      Clean All ({orphanedServers.length})
                    </button>
                  </div>
                  {orphanedServers.slice(0, 4).map((o) => (
                    <div key={o.pid} className="pp-kv text-[10px]">
                      <span className="truncate text-[var(--ter)] font-mono">
                        {o.name} (pid {o.pid}){o.port ? ` :${o.port}` : ""}
                      </span>
                      <div className="flex items-center gap-2 shrink-0">
                        <span className="font-mono text-[var(--sec)]">{formatMemory(o.memory_mb)}</span>
                        <button
                          type="button"
                          onClick={() => void handleTerminateProcess(o.pid, o.name)}
                          className="pp-btn pp-btn--danger"
                        >
                          Quit
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="flex items-center justify-between gap-2 pb-1 border-b border-[var(--card-border)]">
                <span className="text-[9.5px] font-semibold uppercase tracking-wider text-[var(--sec)]">
                  App / process
                </span>
                <div className="flex items-center gap-1 text-[9.5px] font-mono">
                  {(
                    [
                      { id: "cpu", label: "CPU" },
                      { id: "mem", label: "MEM" },
                      { id: "energy", label: "ENERGY (mW)" },
                    ] as const
                  ).map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      onClick={() => setAppSort(s.id)}
                      className={`px-1.5 py-0.5 rounded cursor-pointer ${
                        appSort === s.id
                          ? "bg-[var(--surface)] text-[var(--acc)] font-bold"
                          : "text-[var(--sec)] hover:text-[var(--ink)]"
                      }`}
                    >
                      {s.label}
                      {appSort === s.id ? " ▾" : ""}
                    </button>
                  ))}
                </div>
              </div>

              {telemetry?.top_processes.status === "unavailable" ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  Process telemetry unavailable: {telemetry.top_processes.reason}
                </div>
              ) : filteredProcesses.length === 0 ? (
                <div className="pp-card text-[10px] text-[var(--sec)]">
                  {appSearch ? `No processes matching "${appSearch}".` : "All apps idle (< 0.1% CPU)."}
                </div>
              ) : (
                <div className="flex flex-col gap-1">
                  {filteredProcesses.map((proc) => {
                    const isOpen = expandedAppName === proc.name;
                    const inspection = processInspections[proc.primary_pid];
                    const estMilliwatts =
                      packageWatts > 0
                        ? Math.round((proc.cpu_percent / totalProcCpu) * packageWatts * 1000)
                        : null;
                    return (
                      <div key={proc.name} className="flex flex-col gap-1">
                        <button
                          type="button"
                          onClick={() => {
                            const willOpen = expandedAppName !== proc.name;
                            setExpandedAppName(willOpen ? proc.name : null);
                            if (willOpen && proc.primary_pid > 1) {
                              void handleInspectProcess(proc.primary_pid);
                            }
                          }}
                          aria-expanded={isOpen}
                          className="pp-row4 text-left hover:border-[var(--border)] transition-colors cursor-pointer"
                        >
                          <span className="flex items-center gap-1 min-w-0 font-semibold text-[var(--ink)]">
                            <ChevronRight
                              className={`w-3 h-3 text-[var(--sec)] shrink-0 transition-transform ${
                                isOpen ? "rotate-90 text-[var(--acc)]" : ""
                              }`}
                            />
                            <span className="truncate">{proc.name}</span>
                            {proc.is_paused && (
                              <span className="px-1 py-0.2 rounded bg-amber-500/15 border border-amber-500/30 text-[8.5px] font-mono text-amber-300 shrink-0">
                                Paused
                              </span>
                            )}
                          </span>
                          <b className="text-right font-mono text-[var(--acc)]">
                            {appSort === "energy" && estMilliwatts !== null
                              ? `${estMilliwatts}mW`
                              : `${proc.cpu_percent.toFixed(1)}%`}
                          </b>
                          <span className="text-right font-mono text-[var(--sec)]">
                            {formatMemory(proc.memory_mb)}
                          </span>
                          <span className="text-right font-mono text-[var(--sec)]">
                            ×{proc.process_count}
                          </span>
                        </button>

                        {isOpen && (
                          <div className="px-2.5 py-2 rounded-lg bg-black/25 border border-[var(--card-border)] flex flex-col gap-1.5 text-[10px]">
                            <div className="flex items-center justify-between gap-2">
                              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[var(--sec)] font-mono">
                                <span>
                                  PID:{" "}
                                  <b className="text-[var(--ink)]">
                                    {proc.primary_pid > 0 ? proc.primary_pid : "--"}
                                  </b>
                                </span>
                                <span>
                                  Workers: <b className="text-[var(--ink)]">{proc.process_count}</b>
                                </span>
                                {inspection && (
                                  <span
                                    className={`px-1.5 py-0.5 rounded text-[9px] border ${
                                      inspection.arch.includes("Rosetta")
                                        ? "bg-amber-500/15 border-amber-500/30 text-amber-300"
                                        : "bg-emerald-500/10 border-emerald-500/25 text-emerald-300"
                                    }`}
                                  >
                                    {inspection.arch}
                                  </span>
                                )}
                              </div>

                              <div className="flex items-center gap-1 shrink-0">
                                <button
                                  type="button"
                                  disabled={proc.primary_pid <= 1}
                                  onClick={() =>
                                    void handleTogglePauseProcess(
                                      proc.primary_pid,
                                      proc.name,
                                      !proc.is_paused,
                                    )
                                  }
                                  className={`pp-btn disabled:opacity-40 ${
                                    proc.is_paused ? "pp-btn--acc" : ""
                                  }`}
                                  title={
                                    proc.is_paused
                                      ? "Resume process (SIGCONT)"
                                      : "Freeze process (SIGSTOP) to cut CPU/heat"
                                  }
                                >
                                  {proc.is_paused ? "Resume" : "Pause"}
                                </button>
                                <button
                                  type="button"
                                  disabled={proc.primary_pid <= 1}
                                  onClick={() => void handleTerminateProcess(proc.primary_pid, proc.name)}
                                  className="pp-btn pp-btn--danger disabled:opacity-40"
                                >
                                  Quit
                                </button>
                              </div>
                            </div>

                            {inspectingPid === proc.primary_pid && !inspection && (
                              <div className="text-[9.5px] font-mono text-[var(--sec)]">
                                Inspecting signature &amp; open sockets...
                              </div>
                            )}

                            {inspection && (
                              <div className="flex flex-col gap-1 pt-1 border-t border-white/5 font-mono text-[9.5px]">
                                <div className="flex items-center justify-between gap-2 text-[var(--sec)]">
                                  <span className="truncate">
                                    Parent: <b className="text-[var(--ter)]">{inspection.parent_name}</b> (ppid{" "}
                                    {inspection.parent_pid})
                                  </span>
                                  <span className="shrink-0">CPU {inspection.cpu_time}</span>
                                </div>

                                <div className="flex flex-wrap items-center gap-1">
                                  <span
                                    className={`px-1.5 py-0.5 rounded border ${
                                      inspection.notarized
                                        ? "bg-emerald-500/10 border-emerald-500/25 text-emerald-300"
                                        : "bg-white/5 border-white/10 text-[var(--sec)]"
                                    }`}
                                  >
                                    {inspection.authority ?? "Unsigned / Ad-hoc"}
                                  </span>
                                  {inspection.team_id && (
                                    <span className="px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-[var(--sec)]">
                                      Team {inspection.team_id}
                                    </span>
                                  )}
                                  {inspection.hardened_runtime && (
                                    <span className="px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/20 text-cyan-300">
                                      Hardened
                                    </span>
                                  )}
                                  {inspection.sandboxed && (
                                    <span className="px-1.5 py-0.5 rounded bg-purple-500/10 border border-purple-500/20 text-purple-300">
                                      Sandboxed
                                    </span>
                                  )}
                                </div>

                                {inspection.cwd && (
                                  <div className="flex items-center justify-between gap-2 text-[var(--sec)]">
                                    <span className="truncate" title={inspection.cwd}>
                                      cwd: {inspection.cwd}
                                    </span>
                                    <button
                                      type="button"
                                      onClick={() => void handleRevealInFinder(inspection.cwd!)}
                                      className="pp-btn inline-flex items-center gap-1 shrink-0"
                                    >
                                      <FolderOpen className="w-2.5 h-2.5" />
                                      Reveal
                                    </button>
                                  </div>
                                )}

                                {inspection.open_files.length > 0 && (
                                  <div className="flex flex-col gap-0.5 text-[9px] text-[var(--sec)] max-h-20 overflow-y-auto bg-black/25 p-1.5 rounded">
                                    {inspection.open_files.map((f, fi) => (
                                      <div key={fi} className="truncate">
                                        {f}
                                      </div>
                                    ))}
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
    </>
  );
};
