import type { ThermalPolicyMode } from "../../types";

export const POLICY_MODES = [
  { id: "system_auto", short: "Auto", label: "System Auto" },
  { id: "quiet", short: "Quiet", label: "Quiet" },
  { id: "performance", short: "Perf", label: "Performance" },
  { id: "custom", short: "Custom", label: "Custom", commandLabel: "Custom Rules" },
] as const satisfies readonly {
  id: ThermalPolicyMode;
  short: string;
  label: string;
  commandLabel?: string;
}[];
