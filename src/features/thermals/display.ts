import type { FanReading } from "../../types";

export const displayTemperature = (c: number, unit: "C" | "F") =>
  unit === "F" ? (c * 9) / 5 + 32 : c;

export const formatTempShort = (celsius: number | null, unit: "C" | "F"): string => {
  if (celsius === null) return `--°${unit}`;
  return `${Math.round(displayTemperature(celsius, unit))}°${unit}`;
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
