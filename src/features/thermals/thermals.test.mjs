import test from "node:test";
import assert from "node:assert/strict";
import { evalCurvePercent, resolveTargetTemp } from "./curve.ts";
import {
  displayTemperature,
  fanSpeedPercent,
  formatTempShort,
  gaugeTier,
  gaugeWidth,
  unwrapTelemetry,
} from "./display.ts";

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

test("thermal display helpers format C/F, unwrap availability, and clamp gauges", () => {
  assert.equal(displayTemperature(100, "C"), 100);
  assert.equal(displayTemperature(100, "F"), 212);
  assert.equal(formatTempShort(null, "C"), "--°C");
  assert.equal(formatTempShort(25, "F"), "77°F");
  assert.equal(unwrapTelemetry({ status: "available", value: 42 }), 42);
  assert.equal(unwrapTelemetry({ status: "not_present" }), null);
  assert.equal(unwrapTelemetry(undefined), null);
  assert.equal(gaugeTier(49), "cool");
  assert.equal(gaugeTier(69), "nominal");
  assert.equal(gaugeTier(84), "warm");
  assert.equal(gaugeTier(85), "hot");
  assert.equal(gaugeWidth(10), 0);
  assert.equal(gaugeWidth(120), 100);
  assert.equal(
    fanSpeedPercent({
      id: 0,
      label: "Fan 1",
      speed_rpm: 3000,
      min_speed_rpm: 1000,
      max_speed_rpm: 5000,
      target_speed_rpm: null,
      mode: null,
    }),
    50,
  );
  assert.equal(
    fanSpeedPercent({
      id: 0,
      label: "Fan 1",
      speed_rpm: 2000,
      min_speed_rpm: 2000,
      max_speed_rpm: 2000,
      target_speed_rpm: null,
      mode: null,
    }),
    null,
  );
});
