import { formatDuration } from "../../services/format";
import type { AwakeStatus } from "../../types";

export function formatAwakeBadge(status: AwakeStatus | null): string | null {
  if (!status?.keep_awake_active) return null;
  if (status.remaining_seconds !== null) return formatDuration(status.remaining_seconds);
  return status.auto_rule_reason ? "Auto" : "On";
}

export function formatFocusCountdown(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}
