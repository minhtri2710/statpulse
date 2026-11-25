import type { RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { IslandAlertPayload } from "./types";

export function useIslandAlert(enabledRef?: RefObject<boolean>) {
  const [islandAlert, setIslandAlert] = useState<IslandAlertPayload | null>(null);
  const timerRef = useRef<number | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const clearAlert = useCallback(() => {
    clearTimer();
    setIslandAlert(null);
  }, [clearTimer]);

  useEffect(() => {
    let cancelled = false;
    const unlistenAlert = listen<IslandAlertPayload>("island-alert", (e) => {
      if (cancelled) return;
      clearTimer();
      if (enabledRef && !enabledRef.current) return;
      setIslandAlert(e.payload);
      timerRef.current = window.setTimeout(() => {
        timerRef.current = null;
        setIslandAlert((curr) => (curr?.id === e.payload.id ? null : curr));
      }, e.payload.duration_ms + 150);
    });

    const unlistenAlertClear = listen<number>("island-alert-clear", (e) => {
      if (cancelled) return;
      if (e.payload === 0) {
        clearAlert();
        return;
      }
      setIslandAlert((curr) => {
        if (curr?.id === e.payload) {
          clearTimer();
          return null;
        }
        return curr;
      });
    });

    return () => {
      cancelled = true;
      clearTimer();
      unlistenAlert.then((u) => u()).catch(() => {});
      unlistenAlertClear.then((u) => u()).catch(() => {});
    };
  }, [clearAlert, clearTimer, enabledRef]);

  return { islandAlert, clearAlert };
}
