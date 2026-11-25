import type { AppTab } from "../../navigation";

export type IslandAlertPayload = {
  id: number;
  title: string;
  detail?: string | null;
  tone: "info" | "warn" | "danger" | "good";
  tab?: AppTab | null;
  duration_ms: number;
};

export type IslandDisplayContext = {
  has_notch: boolean;
  display_index: number;
};
