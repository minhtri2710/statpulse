import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useFocusTimer() {
  const [focusTargetSecs, setFocusTargetSecs] = useState<number | null>(null);
  const [endAtMs, setEndAtMs] = useState<number | null>(null);
  const [focusRemainingSecs, setFocusRemainingSecs] = useState<number | null>(null);

  const startFocusTimer = useCallback((seconds: number | null) => {
    setFocusTargetSecs(seconds);
    if (seconds === null || seconds <= 0) {
      setEndAtMs(null);
      setFocusRemainingSecs(null);
      return;
    }
    setEndAtMs(Date.now() + seconds * 1000);
    setFocusRemainingSecs(seconds);
  }, []);

  useEffect(() => {
    if (endAtMs === null) return;
    const sync = () => {
      const remaining = Math.ceil((endAtMs - Date.now()) / 1000);
      if (remaining <= 0) {
        setEndAtMs(null);
        setFocusTargetSecs(null);
        setFocusRemainingSecs(null);
        void invoke("trigger_custom_alert", {
          title: "Focus Timer Complete",
          detail: "Session finished",
          tone: "good",
        }).catch(() => {});
        return false;
      }
      setFocusRemainingSecs(remaining);
      return true;
    };
    if (!sync()) return;
    const id = window.setInterval(sync, 1000);
    window.addEventListener("focus", sync);
    document.addEventListener("visibilitychange", sync);
    return () => {
      window.clearInterval(id);
      window.removeEventListener("focus", sync);
      document.removeEventListener("visibilitychange", sync);
    };
  }, [endAtMs]);

  return { focusRemainingSecs, focusTargetSecs, startFocusTimer };
}
