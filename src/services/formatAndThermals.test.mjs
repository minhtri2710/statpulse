import test from "node:test";
import assert from "node:assert/strict";
import { formatDuration, formatMemory, formatRate, sparkPoints } from "./format.ts";
import { evalCurvePercent, resolveTargetTemp } from "../features/thermals/curve.ts";

test("format helpers handle units and thresholds cleanly", () => {
  assert.equal(formatMemory(512), "512M");
  assert.equal(formatMemory(1536), "1.5G");
  assert.equal(formatRate(500), "500B");
  assert.equal(formatRate(2048), "2K");
  assert.equal(formatRate(2.5 * 1024 * 1024), "2.5M");
  assert.equal(formatDuration(45), "45s");
  assert.equal(formatDuration(125), "2m");
  assert.equal(formatDuration(3600), "1h");
  assert.equal(formatDuration(5400), "1h 30m");
});

test("sparkPoints handles empty, single-point, and custom dimensions", () => {
  assert.equal(sparkPoints([]), "0,12 100,12");
  assert.equal(sparkPoints([50], 0, 200, 20), "0.0,2.6 200.0,2.6");
  const pts = sparkPoints([10, 20, 30], undefined, 100, 14);
  assert.equal(pts.split(" ").length, 3);
});

test("resolveTargetTemp selects cpu, gpu, sensor_key, and hottest", () => {
  const sensors = [
    { key: "soc0", label: "SoC 0", celsius: 62 },
    { key: "nand", label: "NAND", celsius: 38 },
  ];
  assert.equal(resolveTargetTemp({ type: "cpu" }, 71, 65, sensors), 71);
  assert.equal(resolveTargetTemp({ type: "gpu" }, 71, 65, sensors), 65);
  assert.equal(resolveTargetTemp({ type: "sensor_key", key: "soc0" }, 71, 65, sensors), 62);
  assert.equal(resolveTargetTemp({ type: "sensor_key", key: "missing" }, 71, 65, sensors), null);
  assert.equal(resolveTargetTemp({ type: "hottest" }, 55, 78, sensors), 78);
  assert.equal(
    resolveTargetTemp({ type: "hottest" }, null, null, [{ key: "nand", label: "NAND", celsius: 91 }]),
    91,
  );
  assert.equal(resolveTargetTemp({ type: "hottest" }, null, null, []), null);
});

test("evalCurvePercent clamps below/above and interpolates linearly", () => {
  const rule = {
    id: "custom",
    name: "Test",
    target: { type: "cpu" },
    low_celsius: 40,
    high_celsius: 80,
    min_fan_percent: 20,
    max_fan_percent: 100,
    active: true,
  };
  assert.equal(evalCurvePercent(rule, 30), 20);
  assert.equal(evalCurvePercent(rule, 40), 20);
  assert.equal(evalCurvePercent(rule, 60), 60);
  assert.equal(evalCurvePercent(rule, 80), 100);
  assert.equal(evalCurvePercent(rule, 95), 100);
  assert.equal(evalCurvePercent({ ...rule, low_celsius: 70, high_celsius: 70 }, 70), 20);
});
