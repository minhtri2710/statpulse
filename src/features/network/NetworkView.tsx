import { Globe, Terminal, Wifi } from "lucide-react";
import { formatRate, sparkPoints } from "../../services/format";
import type { AppConnectionItem, SpeedTestResult, SystemExtrasSnapshot } from "../../types";

interface NetworkViewProps {
  extras: SystemExtrasSnapshot | null;
  netHistory: { rx: number; tx: number }[];
  expandedNetApp: string | null;
  appConnections: Record<string, AppConnectionItem[]>;
  appConnectionErrors: Record<string, string>;
  loadingConnApp: string | null;
  handleInspectNetApp: (processName: string) => Promise<void>;
  speedTest: SpeedTestResult | null;
  runningSpeedTest: boolean;
  speedTestError: string | null;
  handleRunSpeedTest: () => Promise<void>;
  handleTerminateProcess: (pid: number, label: string) => Promise<unknown>;
}

export const NetworkView = ({
  extras,
  netHistory,
  expandedNetApp,
  appConnections,
  appConnectionErrors,
  loadingConnApp,
  handleInspectNetApp,
  speedTest,
  runningSpeedTest,
  speedTestError,
  handleRunSpeedTest,
  handleTerminateProcess,
}: NetworkViewProps) => {

  return (
    <>
              <div className="pp-pillrow">
                <span className={`pp-pill ${extras?.network.local_ip !== "Offline" ? "pp-pill--good" : "pp-pill--bad"}`}>
                  ● {extras?.network.local_ip !== "Offline" ? "Online" : "Offline"}
                </span>
                {extras?.network.vpn_active && (
                  <span className="pp-pill" style={{ background: "rgba(126, 200, 227, 0.16)", color: "#7ec8e3" }}>
                    VPN · {extras.network.vpn_ip}
                  </span>
                )}
                <span className="text-[var(--sec)] font-mono text-[10px]">
                  {extras ? `${extras.network.interface} · ${extras.network.local_ip}` : "Reading..."}
                </span>
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Wifi className="w-3 h-3" />
                    Throughput
                  </span>
                  <em>
                    {extras
                      ? `↓${formatRate(extras.network.rx_bytes_per_sec)}/s · ↑${formatRate(extras.network.tx_bytes_per_sec)}/s`
                      : "--"}
                  </em>
                </div>
                <svg className="w-full h-14 bg-[var(--surface)] rounded-md p-1" viewBox="0 0 100 14" preserveAspectRatio="none">
                  <polyline
                    fill="none"
                    stroke="var(--acc)"
                    strokeWidth="1.5"
                    vectorEffect="non-scaling-stroke"
                    points={sparkPoints(netHistory.map((n) => n.rx), 0)}
                  />
                  <polyline
                    fill="none"
                    stroke="var(--good)"
                    strokeWidth="1.1"
                    strokeOpacity="0.7"
                    vectorEffect="non-scaling-stroke"
                    points={sparkPoints(netHistory.map((n) => n.tx), 0)}
                  />
                </svg>
                {extras && (
                  <div className="pp-kv text-[10px] text-[var(--sec)]">
                    <span>Session interface total</span>
                    <b className="font-mono text-[var(--ink)]">
                      ↓ {extras.network.total_rx_gb.toFixed(2)} GB · ↑ {extras.network.total_tx_gb.toFixed(2)} GB
                    </b>
                  </div>
                )}
              </div>

              {extras && extras.network.top_net_apps.length > 0 && (
                <div className="pp-card gap-1">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Globe className="w-3 h-3 text-[var(--acc)]" />
                      Network by app (click for sockets)
                    </span>
                    <em>live nettop delta</em>
                  </div>
                  {extras.network.top_net_apps.map((app) => {
                    const isOpen = expandedNetApp === app.name;
                    const conns = appConnections[app.name] ?? [];
                    return (
                      <div key={app.name} className="flex flex-col gap-1">
                        <button
                          type="button"
                          onClick={() => void handleInspectNetApp(app.name)}
                          className="pp-row3 text-[10px] w-full text-left hover:bg-white/5 rounded px-1 py-0.5 cursor-pointer"
                        >
                          <span className="truncate text-[var(--ink)] font-medium">{app.name}</span>
                          <span className="text-right font-mono text-[var(--acc)]">
                            ↓{formatRate(app.rx_bytes_per_sec)}/s
                          </span>
                          <span className="text-right font-mono text-[var(--good)]">
                            ↑{formatRate(app.tx_bytes_per_sec)}/s
                          </span>
                        </button>

                        {isOpen && (
                          <div className="px-2 py-1.5 rounded bg-black/25 border border-white/5 font-mono text-[9px] text-[var(--sec)] max-h-24 overflow-y-auto">
                            {loadingConnApp === app.name ? (
                              <div>Inspecting active connections...</div>
                            ) : appConnectionErrors[app.name] ? (
                              <div className="text-rose-300">{appConnectionErrors[app.name]}</div>
                            ) : conns.length === 0 ? (
                              <div>No open TCP/UDP sockets reported.</div>
                            ) : (
                              conns.map((c, ci) => (
                                <div key={ci} className="flex items-center justify-between gap-2 truncate">
                                  <span className="truncate text-[var(--ter)]">
                                    {c.protocol} {c.remote_endpoint}
                                  </span>
                                  <span className="text-[var(--acc)] shrink-0">{c.state}</span>
                                </div>
                              ))
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span>Latency (1.1.1.1)</span>
                  <em>ICMP probe</em>
                </div>
                <div className="pp-kv">
                  <span>
                    Latency{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.latency_ms !== null && extras?.network.latency_ms !== undefined
                        ? `${extras.network.latency_ms.toFixed(1)} ms`
                        : "--"}
                    </b>
                  </span>
                  <span>
                    Loss{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.packet_loss_percent !== null &&
                      extras?.network.packet_loss_percent !== undefined
                        ? `${extras.network.packet_loss_percent.toFixed(0)}%`
                        : "--"}
                    </b>
                  </span>
                  <span>
                    Jitter{" "}
                    <b className="font-mono text-[var(--ink)]">
                      {extras?.network.jitter_ms !== null && extras?.network.jitter_ms !== undefined
                        ? `${extras.network.jitter_ms.toFixed(1)} ms`
                        : "--"}
                    </b>
                  </span>
                </div>
              </div>

              <div className="pp-card gap-2">
                <div className="pp-card__head">
                  <span>Speed test</span>
                  <button
                    type="button"
                    disabled={runningSpeedTest}
                    onClick={() => void handleRunSpeedTest()}
                    className="pp-btn pp-btn--acc disabled:opacity-50"
                  >
                    {runningSpeedTest ? "Testing..." : "Run networkQuality"}
                  </button>
                </div>

                {speedTestError && (
                  <div className="text-[10px] text-rose-300 font-mono">{speedTestError}</div>
                )}

                {speedTest ? (
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Download</span>
                      <b className="text-sm font-mono">{speedTest.download_mbps}</b>
                      <small className="text-[9px] text-[var(--sec)]">Mbps</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Upload</span>
                      <b className="text-sm font-mono">{speedTest.upload_mbps}</b>
                      <small className="text-[9px] text-[var(--sec)]">Mbps</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Responsive</span>
                      <b className="text-sm font-mono">{speedTest.responsiveness_rpm}</b>
                      <small className="text-[9px] text-[var(--sec)]">RPM</small>
                    </div>
                  </div>
                ) : (
                  <div className="text-[10px] text-[var(--sec)]">
                    Runs Apple&apos;s native <code className="text-[var(--ter)]">networkQuality</code> benchmark on demand.
                  </div>
                )}
              </div>

              {extras && (extras.network.wifi.channel || extras.network.wifi.rssi_dbm !== null) && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Wifi className="w-3 h-3 text-[var(--acc)]" />
                      Wi-Fi Radio
                    </span>
                    <em className="font-mono">
                      {extras.network.wifi.phy_mode || "Wi-Fi"} · Ch {extras.network.wifi.channel || "--"}
                    </em>
                  </div>
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">RSSI</span>
                      <b className="text-xs font-mono">{extras.network.wifi.rssi_dbm ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">dBm</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">SNR</span>
                      <b className="text-xs font-mono">{extras.network.wifi.snr_db ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        dB ({extras.network.wifi.noise_dbm ?? "--"})
                      </small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Tx Rate</span>
                      <b className="text-xs font-mono">{extras.network.wifi.tx_rate_mbps ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        Mbps{extras.network.wifi.mcs_index !== null ? ` · MCS${extras.network.wifi.mcs_index}` : ""}
                      </small>
                    </div>
                  </div>
                  {extras.network.wifi.security && (
                    <div className="text-[9.5px] font-mono text-[var(--sec)]">
                      Security: <span className="text-[var(--ter)]">{extras.network.wifi.security}</span>
                    </div>
                  )}
                </div>
              )}

              {extras && extras.network.tailscale.installed && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Globe className="w-3 h-3 text-[var(--acc)]" />
                      Tailscale Mesh
                    </span>
                    <em className={extras.network.tailscale.running ? "text-[var(--good)]" : ""}>
                      {extras.network.tailscale.running
                        ? `Running · ${extras.network.tailscale.online_peers} online peer(s)`
                        : "Stopped"}
                    </em>
                  </div>
                  <div className="pp-kv text-[10px]">
                    <span className="text-[var(--ter)] truncate">
                      {extras.network.tailscale.dns_name ?? extras.network.tailscale.host_name ?? "Tailnet"}
                    </span>
                    <b className="font-mono text-[var(--ink)]">
                      {extras.network.tailscale.tailscale_ip ?? "--"}
                    </b>
                  </div>
                </div>
              )}

              {extras && extras.network.ssh_tunnels.length > 0 && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <Terminal className="w-3 h-3 text-[var(--acc)]" />
                      Active SSH Tunnels
                    </span>
                    <em>{extras.network.ssh_tunnels.length}</em>
                  </div>
                  {extras.network.ssh_tunnels.map((t) => (
                    <div key={t.pid} className="pp-kv text-[10px]">
                      <span className="truncate font-mono text-[var(--ter)]" title={`${t.kind} ${t.spec} ${t.target}`}>
                        <b className="text-[var(--acc)]">{t.kind}</b> {t.spec} → {t.target || `pid ${t.pid}`}
                      </span>
                      <button
                        type="button"
                        onClick={() => void handleTerminateProcess(t.pid, `SSH ${t.kind} ${t.spec}`)}
                        className="pp-btn pp-btn--danger shrink-0"
                      >
                        Disconnect
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {extras && extras.network.custom_hosts.length > 0 && (
                <div className="pp-card gap-1">
                  <div className="pp-card__head">
                    <span>Custom /etc/hosts Overrides</span>
                    <em>{extras.network.custom_hosts.length}</em>
                  </div>
                  <div className="flex flex-col gap-0.5 max-h-24 overflow-y-auto font-mono text-[9.5px] text-[var(--ter)]">
                    {extras.network.custom_hosts.map((line, idx) => (
                      <div key={idx} className="truncate">
                        {line}
                      </div>
                    ))}
                  </div>
                </div>
              )}
    </>
  );
};
