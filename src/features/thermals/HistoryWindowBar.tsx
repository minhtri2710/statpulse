import { HISTORY_WINDOWS, type HistoryWindow } from "../../services/thermalHistory";

interface HistoryWindowBarProps {
  historyWindow: HistoryWindow;
  setHistoryWindow: (w: HistoryWindow) => void;
}

export function HistoryWindowBar({ historyWindow, setHistoryWindow }: HistoryWindowBarProps) {
  return (
    <div className="flex items-center gap-0.5 bg-[var(--rail)] p-0.5 rounded-md border border-[var(--card-border)] shrink-0">
      {HISTORY_WINDOWS.map((w) => (
        <button
          key={w}
          type="button"
          onClick={() => setHistoryWindow(w)}
          className={`px-1.5 py-0.5 rounded text-[9px] font-mono font-semibold transition-colors cursor-pointer ${
            historyWindow === w
              ? "bg-[var(--surface)] text-[var(--acc)]"
              : "text-[var(--sec)] hover:text-[var(--ink)]"
          }`}
        >
          {w}
        </button>
      ))}
    </div>
  );
}
