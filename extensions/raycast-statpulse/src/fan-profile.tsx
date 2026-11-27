import { Action, ActionPanel, Icon, List, open, showToast, Toast } from "@raycast/api";
import presets from "../../../shared/thermal-presets.json";
import { runStatPulseCli } from "./statpulse";

const PROFILES = [
  { id: "auto", title: "System Auto", subtitle: "Default macOS Apple SMC fan control", icon: Icon.CheckCircle },
  {
    id: "quiet",
    title: "Quiet Profile",
    subtitle: `Ultra-quiet acoustic curve (${presets.quiet.low_celsius}°C – ${presets.quiet.high_celsius}°C)`,
    icon: Icon.Moon,
  },
  {
    id: "performance",
    title: "Performance Profile",
    subtitle: `Aggressive cooling curve (${presets.performance.low_celsius}°C – ${presets.performance.high_celsius}°C)`,
    icon: Icon.Flame,
  },
];

export default function Command() {
  const applyMode = async (mode: string) => {
    try {
      const msg = await runStatPulseCli(["fan", mode]);
      await showToast({ style: Toast.Style.Success, title: msg });
    } catch (err) {
      await showToast({ style: Toast.Style.Failure, title: "Failed to set fan mode", message: String(err) });
    }
  };

  return (
    <List searchBarPlaceholder="Select fan thermal policy...">
      {PROFILES.map((p) => (
        <List.Item
          key={p.id}
          icon={p.icon}
          title={p.title}
          subtitle={p.subtitle}
          actions={
            <ActionPanel>
              <Action title="Apply Profile" onAction={() => applyMode(p.id)} />
            </ActionPanel>
          }
        />
      ))}
      <List.Item
        icon={Icon.Wind}
        title="15s Fan Dust Blast (100% RPM)"
        subtitle="Temporarily spin all fans at maximum RPM for 15 seconds"
        actions={
          <ActionPanel>
            <Action title="Start Dust Blast" onAction={() => open("statpulse://dust-blast")} />
          </ActionPanel>
        }
      />
    </List>
  );
}
