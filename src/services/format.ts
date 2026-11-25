export function sparkPoints(
  values: number[],
  minFloor?: number,
  width = 100,
  height = 14,
): string {
  const padY = Math.min(2, height / 4);
  const baseline = height - padY;
  if (values.length === 0) return `0,${baseline} ${width},${baseline}`;
  const series = values.length === 1 ? [values[0], values[0]] : values;
  const minVal = minFloor !== undefined ? Math.min(minFloor, ...series) : Math.min(...series) - 2;
  const maxVal = Math.max(...series) + 2;
  const span = Math.max(maxVal - minVal, 1);
  const plotH = Math.max(height - padY * 2, 1);
  return series
    .map((v, i) => {
      const x = (i / Math.max(series.length - 1, 1)) * width;
      const y = baseline - ((v - minVal) / span) * plotH;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

export function formatMemory(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)}G`;
  return `${Math.round(mb)}M`;
}

export function formatRate(bytesPerSec: number): string {
  if (bytesPerSec >= 1024 * 1024) return `${(bytesPerSec / (1024 * 1024)).toFixed(1)}M`;
  if (bytesPerSec >= 1024) return `${Math.round(bytesPerSec / 1024)}K`;
  return `${Math.round(bytesPerSec)}B`;
}

export function formatDuration(secs: number): string {
  const hrs = Math.floor(secs / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (hrs > 0) return mins > 0 ? `${hrs}h ${mins}m` : `${hrs}h`;
  if (mins > 0) return `${mins}m`;
  return `${secs}s`;
}

export function unavailableReason(
  availability:
    | { status: "available" }
    | { status: "not_present" }
    | { status: "unavailable"; reason: string },
  notPresentMessage: string,
) {
  return availability.status === "unavailable" ? availability.reason : notPresentMessage;
}
