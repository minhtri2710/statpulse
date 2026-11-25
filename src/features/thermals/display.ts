import type { FanReading } from "../../types";

export type TelemetryAvailability<T> =
  | { status: "available"; value: T }
  | { status: "not_present" }
  | { status: "unavailable"; reason: string };

export const unwrapTelemetry = <T>(
  availability: TelemetryAvailability<T> | undefined,
): T | null => (availability?.status === "available" ? availability.value : null);

export const displayTemperature = (c: number, unit: "C" | "F") =>
  unit === "F" ? (c * 9) / 5 + 32 : c;

export const formatTempShort = (celsius: number | null, unit: "C" | "F"): string => {
  if (celsius === null) return `--°${unit}`;
  return `${Math.round(displayTemperature(celsius, unit))}°${unit}`;
};

export const formatTempDecimal = (
  celsius: number | null,
  unit: "C" | "F",
  decimals = 1,
): string => {
  if (celsius === null) return `--°${unit}`;
  return `${displayTemperature(celsius, unit).toFixed(decimals)}°${unit}`;
};

export const fanSpeedPercent = (fan: FanReading | undefined): number | null => {
  if (!fan || fan.min_speed_rpm === null || fan.max_speed_rpm === null) return null;
  if (fan.max_speed_rpm <= fan.min_speed_rpm) return null;
  return Math.min(
    Math.max(
      ((fan.speed_rpm - fan.min_speed_rpm) / (fan.max_speed_rpm - fan.min_speed_rpm)) * 100,
      0,
    ),
    100,
  );
};

export const gaugeTier = (c: number): "cool" | "nominal" | "warm" | "hot" =>
  c < 50 ? "cool" : c < 70 ? "nominal" : c < 85 ? "warm" : "hot";

export const gaugeWidth = (c: number) =>
  ((Math.min(Math.max(c, 20), 100) - 20) / 80) * 100;

export const computeIslandDotTone = (
  dustBlastSecs: number,
  cameraActive: boolean,
  cpuCelsius: number | null,
  agentCount: number,
  highThermalWarning = false,
): "pp-dot--bad" | "pp-dot--warn" | "pp-dot--good" =>
  dustBlastSecs > 0 || cameraActive || (cpuCelsius ?? 0) >= 80 || highThermalWarning
    ? "pp-dot--bad"
    : (cpuCelsius ?? 0) >= 65 || agentCount > 0
      ? "pp-dot--warn"
      : "pp-dot--good";
