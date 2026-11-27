import { BatteryCharging, Bluetooth, RefreshCw, ShieldCheck, Usb } from "lucide-react";
import { formatDuration } from "../../services/format";
import type { AlertPreferences, BatteryReading, DevicesSnapshot } from "../../types";

interface DevicesViewProps {
  devices: DevicesSnapshot | null;
  battery: BatteryReading | null;
  alerts: AlertPreferences;
  onUpdateAlerts: (next: AlertPreferences) => Promise<void>;
  loadingDevices: boolean;
  refreshDevices: () => Promise<void>;
}

export const DevicesView = ({
  devices,
  battery,
  alerts,
  onUpdateAlerts,
  loadingDevices,
  refreshDevices,
}: DevicesViewProps) => {

  return (
    <>
              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <BatteryCharging className="w-3 h-3" />
                    Charger &amp; Power
                  </span>
                  <button
                    type="button"
                    onClick={() => void refreshDevices()}
                    className="inline-flex items-center gap-1 text-[10px] text-[var(--sec)] hover:text-[var(--ink)] cursor-pointer"
                  >
                    <RefreshCw className={`w-2.5 h-2.5 ${loadingDevices ? "animate-spin" : ""}`} />
                    Refresh
                  </button>
                </div>

                {devices?.charger.connected || battery?.is_charging === true ? (
                  <div className="pp-kv">
                    <span className="text-[var(--ter)]">
                      {devices?.charger.description ?? "USB-C Power Adapter"}
                    </span>
                    <b className="font-mono text-[var(--ink)]">
                      {devices?.charger.watts !== null && devices?.charger.watts !== undefined
                        ? `${devices.charger.watts}W`
                        : "Connected"}
                      {devices?.charger.voltage_v ? ` · ${devices.charger.voltage_v}V` : ""}
                      {devices?.charger.current_a ? ` · ${devices.charger.current_a}A` : ""}
                    </b>
                  </div>
                ) : (
                  <div className="text-[10px] text-[var(--sec)]">Running on battery (no AC adapter).</div>
                )}

                {battery && (
                  <div className="pp-kv text-[10px]">
                    <span className="text-[var(--sec)]">System draw / battery</span>
                    <b className="font-mono">
                      {battery.power_watts !== null ? `${battery.power_watts.toFixed(1)}W` : "--"} ·{" "}
                      {battery.charge_percent ?? "--"}% ({battery.cycle_count ?? "--"} cyc)
                    </b>
                  </div>
                )}

                <div className="flex items-center justify-between gap-2 pt-1 border-t border-white/5 text-[10px]">
                  <span className="flex items-center gap-1 text-[var(--sec)]">
                    <ShieldCheck className="w-3 h-3 text-[var(--good)]" />
                    Smart Charge Alert ({alerts.charge_limit_percent}% · Heat {alerts.battery_heat_celsius}°C)
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      void onUpdateAlerts({
                        ...alerts,
                        notify_charge_limit: !alerts.notify_charge_limit,
                      })
                    }
                    className={`pp-btn ${alerts.notify_charge_limit ? "pp-btn--active" : ""}`}
                  >
                    {alerts.notify_charge_limit ? "On" : "Off"}
                  </button>
                </div>
              </div>

              {devices && devices.battery_health.design_capacity_mah !== null && (
                <div className="pp-card gap-1.5">
                  <div className="pp-card__head">
                    <span className="flex items-center gap-1">
                      <BatteryCharging className="w-3 h-3 text-[var(--acc)]" />
                      Battery Health Vitals
                    </span>
                    <em className="font-mono text-[var(--good)]">
                      {devices.battery_health.health_percent ?? "--"}% Health
                    </em>
                  </div>
                  <div className="pp-speed">
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Full Max</span>
                      <b className="text-xs font-mono">
                        {devices.battery_health.raw_max_capacity_mah ??
                          devices.battery_health.nominal_capacity_mah ??
                          "--"}
                      </b>
                      <small className="text-[9px] text-[var(--sec)]">
                        / {devices.battery_health.design_capacity_mah} mAh
                      </small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Voltage</span>
                      <b className="text-xs font-mono">{devices.battery_health.voltage_v ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">V</small>
                    </div>
                    <div>
                      <span className="text-[9.5px] text-[var(--sec)]">Current</span>
                      <b className="text-xs font-mono">{devices.battery_health.amperage_ma ?? "--"}</b>
                      <small className="text-[9px] text-[var(--sec)]">
                        mA
                        {devices.battery_health.time_remaining_min
                          ? ` · ${formatDuration(devices.battery_health.time_remaining_min * 60)}`
                          : ""}
                      </small>
                    </div>
                  </div>
                </div>
              )}

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Bluetooth className="w-3 h-3" />
                    Bluetooth
                  </span>
                  <em>{devices?.bluetooth.filter((b) => b.connected).length ?? 0} connected</em>
                </div>

                {!devices || devices.bluetooth.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">No Bluetooth devices reported.</div>
                ) : (
                  <div className="flex flex-col gap-1">
                    {devices.bluetooth.slice(0, 8).map((bt, idx) => (
                      <div key={`${bt.name}-${idx}`} className="pp-kv">
                        <span className="flex items-center gap-1.5 min-w-0 truncate">
                          <span
                            className={`pp-dot ${bt.connected ? "pp-dot--good" : ""}`}
                            style={!bt.connected ? { background: "#52555d" } : undefined}
                          />
                          <span className="truncate text-[var(--ink)]">{bt.name}</span>
                          <span className="text-[9.5px] text-[var(--sec)]">{bt.device_type}</span>
                        </span>
                        <b className="font-mono text-[var(--ter)] shrink-0">
                          {bt.battery_percent !== null
                            ? `${bt.battery_percent}%`
                            : bt.connected
                              ? "Connected"
                              : bt.rssi !== null
                                ? `${bt.rssi} dBm`
                                : "Paired"}
                        </b>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="pp-card gap-1.5">
                <div className="pp-card__head">
                  <span className="flex items-center gap-1">
                    <Usb className="w-3 h-3" />
                    USB Tree
                  </span>
                  <em>{devices?.usb.length ?? 0} devices</em>
                </div>

                {!devices || devices.usb.length === 0 ? (
                  <div className="text-[10px] text-[var(--sec)]">No external USB devices attached.</div>
                ) : (
                  <div className="flex flex-col gap-1 font-mono text-[10px]">
                    {devices.usb.map((u, idx) => (
                      <div
                        key={`${u.name}-${idx}`}
                        className="truncate text-[var(--ter)]"
                        style={{ paddingLeft: `${Math.min(u.depth, 4) * 10}px` }}
                      >
                        ↳ {u.name}
                      </div>
                    ))}
                  </div>
                )}
              </div>
    </>
  );
};
