import { Action, ActionPanel, Icon, List, open, showToast, Toast } from "@raycast/api";
import { useEffect, useState } from "react";
import { runStatPulseCli } from "./statpulse";

interface StatusPayload {
  telemetry: {
    temperatures: {
      status: string;
      value?: { cpu_celsius: number | null; gpu_celsius: number | null };
    };
    fans: {
      status: string;
      value?: { id: number; label: string; speed_rpm: number }[];
    };
    battery: {
      status: string;
      value?: { charge_percent: number | null; power_watts: number | null };
    };
  };
  extras: {
    memory: { used_gb: number; total_gb: number; pressure_level: string };
    disk: { free_gb: number; used_percent: number };
    silicon_power: { package_watts: number };
    agents: { agent_name: string; cpu_percent: number; memory_mb: number }[];
  };
  ports_count: number;
}

export default function Command() {
  const [data, setData] = useState<StatusPayload | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const load = async () => {
    setIsLoading(true);
    try {
      const raw = await runStatPulseCli(["status", "--json"]);
      setData(JSON.parse(raw) as StatusPayload);
    } catch (err) {
      await showToast({
        style: Toast.Style.Failure,
        title: "Failed to query StatPulse",
        message: String(err),
      });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const temps = data?.telemetry.temperatures.value;
  const fans = data?.telemetry.fans.value ?? [];
  const mem = data?.extras.memory;

  return (
    <List isLoading={isLoading} searchBarPlaceholder="Filter hardware metrics...">
      {data && (
        <>
          <List.Item
            icon={Icon.Temperature}
            title="CPU & GPU Thermals"
            subtitle={`CPU ${temps?.cpu_celsius?.toFixed(1) ?? "--"}°C · GPU ${temps?.gpu_celsius?.toFixed(1) ?? "--"}°C`}
            accessories={[{ text: `${data.extras.silicon_power.package_watts.toFixed(1)}W pkg` }]}
            actions={
              <ActionPanel>
                <Action title="Open StatPulse" onAction={() => open("statpulse://open/system")} />
                <Action title="Refresh" onAction={load} />
              </ActionPanel>
            }
          />
          {fans.map((f) => (
            <List.Item
              key={f.id}
              icon={Icon.Wind}
              title={f.label}
              subtitle={`${f.speed_rpm} RPM`}
              actions={
                <ActionPanel>
                  <Action title="Open Fans Tab" onAction={() => open("statpulse://open/fans")} />
                  <Action title="15s Dust Blast" onAction={() => open("statpulse://dust-blast")} />
                </ActionPanel>
              }
            />
          ))}
          {mem && (
            <List.Item
              icon={Icon.MemoryChip}
              title="Memory & Disk"
              subtitle={`${mem.used_gb.toFixed(1)} / ${mem.total_gb.toFixed(0)} GB (${mem.pressure_level})`}
              accessories={[{ text: `${data.extras.disk.free_gb.toFixed(0)} GB free` }]}
            />
          )}
          <List.Item
            icon={Icon.Network}
            title="Developer Cockpit"
            subtitle={`${data.ports_count} listening ports · ${data.extras.agents.length} active AI agents`}
            actions={
              <ActionPanel>
                <Action title="Open Ports" onAction={() => open("statpulse://open/ports")} />
                <Action title="Open AI Agents" onAction={() => open("statpulse://open/agents")} />
              </ActionPanel>
            }
          />
        </>
      )}
    </List>
  );
}
