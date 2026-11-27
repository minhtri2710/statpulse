import test from "node:test";
import assert from "node:assert/strict";
import {
  formatDuration,
  formatMemory,
  formatRate,
  sparkPoints,
  unavailableReason,
} from "./format.ts";

test("format helpers handle units, zero/sub-minute/hour durations, and thresholds", () => {
  assert.equal(formatMemory(512), "512M");
  assert.equal(formatMemory(1536), "1.5G");
  assert.equal(formatRate(500), "500B");
  assert.equal(formatRate(2048), "2K");
  assert.equal(formatRate(2.5 * 1024 * 1024), "2.5M");
  assert.equal(formatDuration(0), "0s");
  assert.equal(formatDuration(45), "45s");
  assert.equal(formatDuration(125), "2m");
  assert.equal(formatDuration(3600), "1h");
  assert.equal(formatDuration(5400), "1h 30m");
});

test("unavailableReason returns explicit reason only when unavailable", () => {
  assert.equal(
    unavailableReason({ status: "unavailable", reason: "SMC denied" }, "Not present"),
    "SMC denied",
  );
  assert.equal(unavailableReason({ status: "not_present" }, "No fan"), "No fan");
  assert.equal(unavailableReason({ status: "available" }, "No fan"), "No fan");
});

test("sparkPoints handles empty, single-point, monotonic X, and inverted SVG Y", () => {
  assert.equal(sparkPoints([]), "0,12 100,12");
  assert.equal(sparkPoints([50], 0, 200, 20), "0.0,2.6 200.0,2.6");

  const pts = sparkPoints([10, 50, 30], undefined, 100, 14)
    .split(" ")
    .map((pair) => pair.split(",").map(Number));
  assert.equal(pts.length, 3);
  assert.ok(pts[0][0] < pts[1][0] && pts[1][0] < pts[2][0], "X coordinates increase monotonically");
  assert.ok(pts[1][1] < pts[2][1] && pts[2][1] < pts[0][1], "higher values map to smaller SVG Y");
});
