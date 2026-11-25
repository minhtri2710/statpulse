import { Action, ActionPanel, Icon, List, showToast, Toast } from "@raycast/api";
import { useEffect, useState } from "react";
import { runStatPulseCli } from "./statpulse";

interface ListeningPort {
  port: number;
  pid: number;
  process_name: string;
  bind_address: string;
  is_exposed: boolean;
  project_path: string | null;
  launcher: string | null;
}

export default function Command() {
  const [ports, setPorts] = useState<ListeningPort[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  const load = async () => {
    setIsLoading(true);
    try {
      const raw = await runStatPulseCli(["ports", "--json"]);
      setPorts(JSON.parse(raw) as ListeningPort[]);
    } catch (err) {
      await showToast({ style: Toast.Style.Failure, title: "Failed to list ports", message: String(err) });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const killPort = async (port: number) => {
    try {
      const msg = await runStatPulseCli(["kill", String(port)]);
      await showToast({ style: Toast.Style.Success, title: msg });
      await load();
    } catch (err) {
      await showToast({ style: Toast.Style.Failure, title: "Kill failed", message: String(err) });
    }
  };

  return (
    <List isLoading={isLoading} searchBarPlaceholder="Filter port, process, or project...">
      {ports.map((p) => (
        <List.Item
          key={`${p.port}-${p.pid}`}
          icon={p.is_exposed ? Icon.Globe : Icon.Plug}
          title={`:${p.port} — ${p.process_name}`}
          subtitle={p.project_path ?? p.bind_address}
          accessories={[{ text: p.launcher ?? `pid ${p.pid}` }]}
          actions={
            <ActionPanel>
              <Action.OpenInBrowser url={`http://localhost:${p.port}`} />
              <Action title="Kill Port Process" style={Action.Style.Destructive} onAction={() => killPort(p.port)} />
              <Action.CopyToClipboard title="Copy Localhost URL" content={`http://localhost:${p.port}`} />
              <Action title="Refresh" onAction={load} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
