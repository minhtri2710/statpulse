import test from "node:test";
import assert from "node:assert/strict";
import {
  appendLiveSample,
  compactHistory,
  sanitizeHistory,
  selectHistory,
} from "./thermalHistory.ts";

const EPOCH = 1_700_000_000_000;
const TWO_MINUTES_MS = 120_000;
const DAY_MS = 86_400_000;
const WEEK_MS = 604_800_000;

function sample(time, cpu, gpu = null) {
  return {
    time,
    cpu,
    gpu,
    fan1Rpm: null,
    fan2Rpm: null,
    powerWatts: null,
    batteryPct: null,
  };
}

function week(cadenceMs = 20_000) {
  const rows = [];
  const start = EPOCH;
  const end = start + WEEK_MS;
  for (let time = start; time <= end; time += cadenceMs) {
    rows.push(sample(time, 40 + (time % 10)));
  }
  // A hot spike early in the week and the latest observation at the end.
  rows[10] = sample(start + 10 * cadenceMs, 97, 80);
  rows.push(sample(end, 33));
  return { rows, end };
}

test("a week of 20s samples compacts under budget and keeps the early spike", () => {
  const { rows, end } = week();
  const compacted = compactHistory(rows, end);
  assert.ok(compacted.length < 760, `expected <760, got ${compacted.length}`);
  assert.ok(compacted.some((row) => row.cpu === 97), "early 97C spike must survive");
  assert.equal(compacted[compacted.length - 1].time, end);
  for (let i = 1; i < compacted.length; i += 1) {
    assert.ok(compacted[i].time > compacted[i - 1].time, "output stays chronological");
  }
});

test("incremental compaction across hour/day/week keeps recorded peaks", () => {
  // Simulate capture arriving in batches: compact after each append with a
  // moving end timestamp, so the same data is re-compacted repeatedly.
  let stored = [];
  for (let batch = 0; batch < 60; batch += 1) {
    const end = EPOCH + batch * 10_080_000; // ~1/60 of a week per batch
    const incoming = [];
    for (let time = end - 10_080_000; time < end; time += 20_000) {
      incoming.push(sample(time, 40 + (time % 7)));
    }
    if (batch === 2) incoming[3] = sample(incoming[3].time, 97, 80);
    stored = compactHistory([...stored, ...incoming], end);
    assert.ok(
      batch < 2 || stored.some((row) => row.cpu === 97),
      `peak lost after batch ${batch}`,
    );
  }
  assert.ok(stored.length < 760, `expected <760, got ${stored.length}`);
});

test("the 2m window caps at 120 with both endpoints at any cadence", () => {
  for (const cadence of [1000, 1500, 2500]) {
    const end = EPOCH;
    let live = [];
    const sourceTimes = [];
    for (let time = end - 300_000; time <= end; time += cadence) {
      sourceTimes.push(time);
      live = appendLiveSample(live, sample(time, 50));
    }
    const rows = selectHistory(live, [], "2m", end);
    assert.ok(rows.length <= 120, `cadence ${cadence}: got ${rows.length}`);
    // Endpoints come from the input times, not whichever row was retained
    // first: the live window starts at end - 2m and ends at end.
    const windowStart = Math.max(end - TWO_MINUTES_MS, sourceTimes[0]);
    assert.equal(rows[0].time, windowStart, `cadence ${cadence}: first endpoint`);
    assert.equal(rows[rows.length - 1].time, end, `cadence ${cadence}: last endpoint`);
    for (const row of rows) {
      assert.ok(row.time >= end - TWO_MINUTES_MS && row.time <= end);
    }
  }
});

test("a clock rollback drops rows newer than the new sample time", () => {
  const end = EPOCH;
  let live = [];
  for (let time = end - 60_000; time <= end; time += 1_000) {
    live = appendLiveSample(live, sample(time, 50));
  }
  const rolled = appendLiveSample(live, sample(end - 30_000, 42));
  assert.equal(rolled[rolled.length - 1].time, end - 30_000);
  assert.ok(!rolled.some((row) => row.time > end - 30_000), "future rows must not survive");
});

test("windowed selection is capped at 120 yet retains the historic high", () => {
  const end = EPOCH + DAY_MS;
  const stored = [];
  for (let time = EPOCH; time <= end; time += 20_000) {
    stored.push(sample(time, 45));
  }
  stored[5] = sample(EPOCH + 100_000, 97);
  const rows = selectHistory([], stored, "24h", end);
  assert.ok(rows.length <= 120, `expected <=120, got ${rows.length}`);
  assert.ok(rows.some((row) => row.cpu === 97), "historic high must be retained");
  assert.equal(rows[0].time, EPOCH);
  assert.equal(rows[rows.length - 1].time, end);
});

test("stored and live rows merge without duplicate or reordered timestamps", () => {
  const end = 3_600_000;
  const stored = [sample(3_590_000, 50), sample(3_595_000, 51)];
  const live = [sample(3_595_000, 60), sample(3_600_000, 61)];
  const rows = selectHistory(live, stored, "1h", end);
  assert.deepEqual(
    rows.map((row) => row.time),
    [3_590_000, 3_595_000, 3_600_000],
  );
  assert.equal(rows[1].cpu, 51, "stored row wins over a same-timestamp live row");
});

test("malformed storage input sanitizes to an empty history", () => {
  for (const garbage of ["not json", 42, {}, [{ time: "x" }], [sample(Number.NaN, 50)], null]) {
    assert.deepEqual(sanitizeHistory(garbage), []);
  }
  const ok = sanitizeHistory([sample(2_000, 51), sample(1_000, 50), sample(1_000, 99), "junk"]);
  assert.deepEqual(
    ok.map((row) => row.time),
    [1_000, 2_000],
    "valid rows are ordered and same-timestamp duplicates dropped",
  );
});

test("compaction preserves a GPU-only thermal peak even when CPU stays cool", () => {
  const start = EPOCH;
  const end = start + 2 * 3_600_000;
  const rows = [];
  for (let time = start; time <= end; time += 20_000) {
    rows.push(sample(time, 45, 42));
  }
  rows[5] = sample(start + 5 * 20_000, 48, 96);
  const compacted = compactHistory(rows, end);
  assert.ok(compacted.some((row) => row.gpu === 96), "GPU-only 96C spike must survive");
});
