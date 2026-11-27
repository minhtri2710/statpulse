import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HardwareTelemetrySnapshot } from "../../types";
import {
  appendLiveSample,
  compactHistory,
  HistorySample,
  HistoryWindow,
  LONG_SAMPLE_INTERVAL_MS,
  sanitizeHistory,
  selectHistory,
} from "../../services/thermalHistory";
import { useTransientValue } from "../useTransient";

const LONG_HISTORY_STORAGE_KEY = "statpulse.long_history.v1";

type RecordedSample = {
  time: number;
  cpu: number;
  gpu: number | null;
  fan: number | null;
  watts: number | null;
  memGb: number | null;
};

// Hydrate the persisted v1 array defensively, then compact relative to the
// newest usable observation — never against the wall clock, so fixtures and
// resumed windows retain their early peaks.
function loadLongHistory(): HistorySample[] {
  try {
    const raw = localStorage.getItem(LONG_HISTORY_STORAGE_KEY);
    if (!raw) return [];
    const rows = sanitizeHistory(JSON.parse(raw));
    const newest = rows[rows.length - 1];
    return newest ? compactHistory(rows, newest.time) : [];
  } catch {
    return [];
  }
}

export function useTelemetryHistory(memoryGb: number | null, collapsed: boolean) {
  const [telemetry, setTelemetry] = useState<HardwareTelemetrySnapshot | null>(null);
  const [history, setHistory] = useState<HistorySample[]>([]);
  const [longHistory, setLongHistory] = useState<HistorySample[]>(() => loadLongHistory());
  const [historyWindow, setHistoryWindow] = useState<HistoryWindow>("2m");
  const lastLongSampleRef = useRef<number>(0);
  const lastCaptureMs = useRef<number | null>(null);
  const memoryGbRef = useRef<number | null>(null);
  memoryGbRef.current = memoryGb;
  // Recorded rows live in a ref: the telemetry listener is long-lived and a
  // state array would be both stale inside its closure and O(n) to extend.
  const samplesRef = useRef<RecordedSample[] | null>(null);
  const [recordedCount, setRecordedCount] = useState<number | null>(null);
  const [sessionExportMsg, flashSessionExportMsg] = useTransientValue<string>(4000);

  const recordSnapshot = useCallback((snapshot: HardwareTelemetrySnapshot) => {
    const capturedAt = snapshot.captured_at_unix_ms;
    const lastCapture = lastCaptureMs.current;
    if (lastCapture !== null && capturedAt <= lastCapture) {
      // Tray-reuse replies can be duplicates or older; they are dropped. A
      // large backward step means the wall clock was adjusted, so the dedup
      // baseline resets rather than muting every future sample.
      if (lastCapture - capturedAt <= 60_000) return;
    }
    lastCaptureMs.current = capturedAt;
    setTelemetry(snapshot);
    const temps = snapshot.temperatures.status === "available" ? snapshot.temperatures.value : null;
    const cpu = temps?.cpu_celsius ?? null;
    if (cpu === null) return;

    const fanList = snapshot.fans.status === "available" ? snapshot.fans.value : [];
    const bat = snapshot.battery.status === "available" ? snapshot.battery.value : null;

    const sample: HistorySample = {
      time: snapshot.captured_at_unix_ms,
      cpu,
      gpu: temps?.gpu_celsius ?? null,
      fan1Rpm: fanList[0]?.speed_rpm ?? null,
      fan2Rpm: fanList[1]?.speed_rpm ?? null,
      powerWatts: bat?.power_watts ?? null,
      batteryPct: bat?.charge_percent ?? null,
    };

    setHistory((previous) => appendLiveSample(previous, sample));

    const recordedRows = samplesRef.current;
    if (recordedRows !== null) {
      recordedRows.push({
        time: sample.time,
        cpu,
        gpu: temps?.gpu_celsius ?? null,
        fan: fanList[0]?.speed_rpm ?? null,
        watts: bat?.power_watts ?? null,
        memGb: memoryGbRef.current,
      });
      setRecordedCount(recordedRows.length);
    }

    // A backward wall-clock jump invalidates the throttle and any rows dated
    // in the future of the newest real observation.
    if (lastLongSampleRef.current - sample.time > LONG_SAMPLE_INTERVAL_MS) {
      lastLongSampleRef.current = 0;
      setLongHistory((prev) => prev.filter((row) => row.time <= sample.time));
    }
    if (sample.time - lastLongSampleRef.current >= LONG_SAMPLE_INTERVAL_MS) {
      lastLongSampleRef.current = sample.time;
      setLongHistory((prev) =>
        compactHistory([...prev.filter((row) => row.time <= sample.time), sample], sample.time),
      );
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    invoke<HardwareTelemetrySnapshot>("fetch_telemetry")
      .then((snapshot) => {
        if (!cancelled) recordSnapshot(snapshot);
      })
      .catch((error) => console.error("Hardware telemetry snapshot fetch failed:", error));
    const unlistenTelemetry = listen<HardwareTelemetrySnapshot>("telemetry-update", (event) => {
      if (!cancelled) recordSnapshot(event.payload);
    });
    return () => {
      cancelled = true;
      unlistenTelemetry.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [recordSnapshot]);

  // Persisted only after a real append (lastLongSampleRef > 0), so the
  // restored history is not rewritten on mount.
  useEffect(() => {
    if (lastLongSampleRef.current === 0) return;
    try {
      localStorage.setItem(LONG_HISTORY_STORAGE_KEY, JSON.stringify(longHistory));
    } catch {
    }
  }, [longHistory]);

  const toggleRecording = async () => {
    if (recordedCount === null) {
      samplesRef.current = [];
      setRecordedCount(0);
      flashSessionExportMsg(null);
      return;
    }
    // Detach the buffer synchronously: telemetry events between here and the
    // awaited clipboard write must not land in the report.
    const recordedSamples = samplesRef.current ?? [];
    samplesRef.current = null;
    setRecordedCount(null);
    if (recordedSamples.length === 0) {
      flashSessionExportMsg("No telemetry samples captured.");
      return;
    }
    const maxCpu = recordedSamples.reduce((peak, s) => Math.max(peak, s.cpu), -Infinity);
    const avgCpu = recordedSamples.reduce((a, s) => a + s.cpu, 0) / recordedSamples.length;
    const maxFan = recordedSamples.reduce(
      (peak, s) => (s.fan !== null ? Math.max(peak, s.fan) : peak),
      0,
    );
    const watts = recordedSamples.flatMap((s) => (s.watts !== null ? [s.watts] : []));
    const avgWatts = watts.length > 0 ? watts.reduce((a, b) => a + b, 0) / watts.length : 0;
    const csvRows = [
      "timestamp,cpu_c,gpu_c,fan_rpm,package_watts,mem_gb",
      ...recordedSamples.map(
        (s) =>
          `${new Date(s.time).toISOString()},${s.cpu.toFixed(1)},${s.gpu?.toFixed(1) ?? ""},${s.fan ?? ""},${s.watts?.toFixed(1) ?? ""},${s.memGb?.toFixed(1) ?? ""}`,
      ),
    ].join("\n");
    const report = `# StatPulse Thermal Session (${recordedSamples.length} samples)\n- Avg CPU: ${avgCpu.toFixed(1)}°C | Peak CPU: ${maxCpu.toFixed(1)}°C | Max Fan: ${maxFan} RPM | Avg Power: ${avgWatts.toFixed(1)}W\n\n\`\`\`csv\n${csvRows}\n\`\`\``;
    try {
      await navigator.clipboard.writeText(report);
      flashSessionExportMsg(
        `Copied ${recordedSamples.length} samples (Peak ${Math.round(maxCpu)}°C) to clipboard.`,
      );
    } catch {
      flashSessionExportMsg(
        `Recorded ${recordedSamples.length} samples (Peak ${Math.round(maxCpu)}°C).`,
      );
    }
  };

  const windowedHistory = useMemo(
    () =>
      collapsed
        ? []
        : selectHistory(
            history,
            longHistory,
            historyWindow,
            history[history.length - 1]?.time ?? Date.now(),
          ),
    [collapsed, history, longHistory, historyWindow],
  );

  return {
    telemetry,
    windowedHistory,
    historyWindow,
    setHistoryWindow,
    recordSnapshot,
    recordedCount,
    toggleRecording,
    sessionExportMsg,
  };
}
