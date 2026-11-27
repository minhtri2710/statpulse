import { Terminal } from "lucide-react";
import type { SshTunnelItem } from "../../types";

interface ActiveTunnelsCardProps {
  tunnels: SshTunnelItem[];
  actionLabel: string;
  onTerminate: (pid: number, label: string) => Promise<unknown>;
}

export function ActiveTunnelsCard({ tunnels, actionLabel, onTerminate }: ActiveTunnelsCardProps) {
  if (tunnels.length === 0) return null;

  return (
    <div className="pp-card gap-1.5">
      <div className="pp-card__head">
        <span className="flex items-center gap-1">
          <Terminal className="w-3 h-3 text-[var(--acc)]" />
          Active Tunnels &amp; Forwards
        </span>
        <em>{tunnels.length}</em>
      </div>
      {tunnels.map((t) => (
        <div key={t.pid} className="pp-kv text-[10px]">
          <span
            className="truncate font-mono text-[var(--ter)]"
            title={`${t.kind} ${t.spec} ${t.target}`}
          >
            <b className="text-[var(--acc)]">{t.kind}</b> {t.spec} → {t.target || `pid ${t.pid}`}
          </span>
          <button
            type="button"
            onClick={() => void onTerminate(t.pid, `${t.kind} ${t.spec}`)}
            className="pp-btn pp-btn--danger shrink-0"
          >
            {actionLabel}
          </button>
        </div>
      ))}
    </div>
  );
}
