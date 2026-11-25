import type { IslandAlertPayload } from "./types";

interface IslandAlertStripProps {
  alert: IslandAlertPayload;
  onClick: () => void;
}

export function IslandAlertStrip({ alert, onClick }: IslandAlertStripProps) {
  const dotClass =
    alert.tone === "danger"
      ? "pp-dot--bad"
      : alert.tone === "warn"
        ? "pp-dot--warn"
        : alert.tone === "good"
          ? "pp-dot--good"
          : "";

  return (
    <button
      type="button"
      data-tone={alert.tone}
      className="di-alert-strip"
      onClick={onClick}
      title="Click to inspect details"
    >
      <span className="flex items-center gap-1.5 min-w-0">
        <span
          className={`pp-dot shrink-0 ${dotClass}`}
          style={alert.tone === "info" ? { background: "var(--acc)" } : undefined}
        />
        <span className="font-semibold text-[10px] text-[var(--ink)] truncate">{alert.title}</span>
        {alert.detail && (
          <span className="font-mono text-[9.5px] text-[var(--sec)] truncate">{alert.detail}</span>
        )}
      </span>
      <span className="text-[9.5px] font-medium text-[var(--sec)] shrink-0">Open ›</span>
    </button>
  );
}
