import type { TemperatureReading, ThermalRule, ThermalTarget } from "../../types";

export function resolveTargetTemp(
  target: ThermalTarget,
  cpu: number | null,
  gpu: number | null,
  sensors: TemperatureReading[],
): number | null {
  if (target.type === "cpu") return cpu;
  if (target.type === "gpu") return gpu;
  if (target.type === "sensor_key") {
    return sensors.find((s) => s.key === target.key)?.celsius ?? null;
  }
  const all = [
    ...(cpu !== null ? [cpu] : []),
    ...(gpu !== null ? [gpu] : []),
    ...sensors.map((s) => s.celsius),
  ];
  return all.length > 0 ? Math.max(...all) : null;
}

export function evalCurvePercent(rule: ThermalRule, celsius: number): number {
  if (celsius <= rule.low_celsius) return rule.min_fan_percent;
  if (celsius >= rule.high_celsius) return rule.max_fan_percent;
  const t = (celsius - rule.low_celsius) / Math.max(rule.high_celsius - rule.low_celsius, 1);
  return rule.min_fan_percent + t * (rule.max_fan_percent - rule.min_fan_percent);
}
