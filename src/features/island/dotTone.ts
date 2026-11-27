export const computeIslandDotTone = (
  dustBlastSecs: number,
  cameraActive: boolean,
  cpuCelsius: number | null,
  agentCount: number,
): "pp-dot--bad" | "pp-dot--warn" | "pp-dot--good" =>
  dustBlastSecs > 0 || cameraActive || (cpuCelsius ?? 0) >= 80
    ? "pp-dot--bad"
    : (cpuCelsius ?? 0) >= 65 || agentCount > 0
      ? "pp-dot--warn"
      : "pp-dot--good";
