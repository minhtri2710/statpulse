import { useCallback, useEffect, useRef, useState } from "react";

export interface ActionFeedback {
  kind: "ok" | "err";
  text: string;
}

export function useTransientValue<T>(ttlMs: number): [T | null, (value: T | null) => void] {
  const [value, setValue] = useState<T | null>(null);
  const timerRef = useRef<number | null>(null);

  const flash = useCallback(
    (next: T | null) => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      setValue(next);
      if (next !== null) {
        timerRef.current = window.setTimeout(() => {
          timerRef.current = null;
          setValue(null);
        }, ttlMs);
      }
    },
    [ttlMs],
  );

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  return [value, flash];
}
