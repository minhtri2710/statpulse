// Pure thermal-history helpers: storage validation, long-window compaction
// and per-window selection. No framework or platform dependencies so the
// logic can be exercised directly under Node's test runner.

export interface HistorySample {
  time: number;
  cpu: number;
  gpu: number | null;
  fan1Rpm: number | null;
  fan2Rpm: number | null;
  powerWatts: number | null;
  batteryPct: number | null;
}

export const WINDOWS_MS = {
  "2m": 120_000,
  "1h": 3_600_000,
  "24h": 86_400_000,
  "7d": 604_800_000,
} as const;
export type HistoryWindow = keyof typeof WINDOWS_MS;
export const HISTORY_WINDOWS = Object.keys(WINDOWS_MS) as HistoryWindow[];

export const LONG_SAMPLE_INTERVAL_MS = 20_000;
const CHART_SAMPLE_CAP = 120;
// At one sample per second, a closed two-minute interval has 121
// observations. This retention budget must not shrink when the rendering
// budget changes.
const LIVE_SAMPLE_CAP = WINDOWS_MS["2m"] / 1_000 + 1;

export function appendLiveSample(
  previous: HistorySample[],
  sample: HistorySample,
): HistorySample[] {
  const start = sample.time - WINDOWS_MS["2m"];
  return [...previous.filter((row) => row.time >= start && row.time < sample.time), sample]
    .slice(-LIVE_SAMPLE_CAP);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNullableNumber(value: unknown): value is number | null {
  return value === null || isFiniteNumber(value);
}

export function isHistorySample(value: unknown): value is HistorySample {
  if (typeof value !== "object" || value === null) return false;
  const sample = value as Record<string, unknown>;
  return (
    isFiniteNumber(sample.time) &&
    isFiniteNumber(sample.cpu) &&
    isNullableNumber(sample.gpu) &&
    isNullableNumber(sample.fan1Rpm) &&
    isNullableNumber(sample.fan2Rpm) &&
    isNullableNumber(sample.powerWatts) &&
    isNullableNumber(sample.batteryPct)
  );
}

// Storage hydration: rejects malformed rows, then orders and de-duplicates by
// timestamp so persisted data can never poison the pipeline.
export function sanitizeHistory(input: unknown): HistorySample[] {
  if (!Array.isArray(input)) return [];
  const rows: HistorySample[] = [];
  const seen = new Set<number>();
  for (const value of input) {
    if (!isHistorySample(value) || seen.has(value.time)) continue;
    seen.add(value.time);
    rows.push(value);
  }
  rows.sort((a, b) => a.time - b.time);
  return rows;
}

function temperaturePeak(sample: HistorySample): number {
  return Math.max(sample.cpu, sample.gpu ?? -Infinity);
}

function compactionWidth(ageMs: number): number {
  if (ageMs <= WINDOWS_MS["1h"]) return LONG_SAMPLE_INTERVAL_MS;
  if (ageMs <= WINDOWS_MS["24h"]) return 300_000;
  return 1_800_000;
}

// Bounds long-term storage while preserving thermal peaks: within each tier
// width bucket (20s for the latest hour, 5m up to a day, 30m beyond) a single
// actual sample survives — the one with the hottest CPU/GPU, newer on ties.
// The newest observation is always kept even when it is not the peak, and
// equal timestamps are de-duplicated. A continuous week of 20s samples stays
// well below 760 representatives.
export function compactHistory(rows: HistorySample[], end: number): HistorySample[] {
  const start = end - WINDOWS_MS["7d"];
  const byKey = new Map<string, HistorySample>();
  let newest: HistorySample | null = null;
  for (const row of rows) {
    if (row.time < start || row.time > end) continue;
    if (newest === null || row.time >= newest.time) newest = row;
    const width = compactionWidth(end - row.time);
    const key = `${width}:${Math.floor(row.time / width)}`;
    const kept = byKey.get(key);
    if (kept === undefined) {
      byKey.set(key, row);
      continue;
    }
    if (row.time === kept.time) continue;
    if (
      temperaturePeak(row) > temperaturePeak(kept) ||
      (temperaturePeak(row) === temperaturePeak(kept) && row.time > kept.time)
    ) {
      byKey.set(key, row);
    }
  }
  const result = [...byKey.values()].sort((a, b) => a.time - b.time);
  if (newest !== null && !result.some((row) => row.time === newest.time)) {
    result.push(newest);
  }
  return result;
}

// Hard display budget: at most two extrema (coldest and hottest by combined
// CPU/GPU peak) per bucket across the elapsed span, first and last samples
// always included.
function capForChart(rows: HistorySample[]): HistorySample[] {
  if (rows.length <= CHART_SAMPLE_CAP) return rows;
  const first = rows[0];
  const last = rows[rows.length - 1];
  const bucketCount = (CHART_SAMPLE_CAP - 2) / 2;
  const span = Math.max(1, last.time - first.time);
  const result: HistorySample[] = [first];
  let previous = -1;
  let low: HistorySample | undefined;
  let high: HistorySample | undefined;
  const flush = () => {
    if (!low || !high) return;
    if (low === high) result.push(low);
    else if (low.time < high.time) result.push(low, high);
    else result.push(high, low);
  };
  for (let i = 1; i < rows.length - 1; i++) {
    const row = rows[i];
    const bucket = Math.min(
      bucketCount - 1,
      Math.floor(((row.time - first.time) * bucketCount) / span),
    );
    if (bucket !== previous) {
      flush();
      low = high = row;
      previous = bucket;
      continue;
    }
    if (temperaturePeak(row) < temperaturePeak(low!)) low = row;
    if (temperaturePeak(row) >= temperaturePeak(high!)) high = row;
  }
  flush();
  result.push(last);
  return result;
}

// Windowed view over live + persisted samples for the chart and metric tiles.
// The 2m view is live-only; longer windows merge persisted rows with any live
// rows newer than the stored data, never fabricating history. Output is
// chronological, de-duplicated by timestamp, and capped for rendering.
export function selectHistory(
  live: HistorySample[],
  stored: HistorySample[],
  window: HistoryWindow,
  end: number,
): HistorySample[] {
  const start = end - WINDOWS_MS[window];
  const inWindow = (row: HistorySample) => row.time >= start && row.time <= end;
  if (window === "2m") {
    return capForChart(live.filter(inWindow));
  }
  const rows: HistorySample[] = [];
  const seen = new Set<number>();
  for (const row of stored) {
    if (!inWindow(row) || seen.has(row.time)) continue;
    seen.add(row.time);
    rows.push(row);
  }
  for (const row of live) {
    if (!inWindow(row) || seen.has(row.time)) continue;
    seen.add(row.time);
    rows.push(row);
  }
  rows.sort((a, b) => a.time - b.time);
  return capForChart(rows);
}
