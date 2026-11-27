import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTransientValue } from "../useTransient";
import type { ListeningPort, PortHttpProbe } from "../../types";

// Listening-port list, HTTP probing, copy feedback and project-stack stop.
// Stack-stop outcome is its own feedback state so a later successful list
// refresh never erases a partial-failure message.
export function usePortList(refreshExtras: () => void) {
  const [ports, setPorts] = useState<ListeningPort[]>([]);
  const [portSearch, setPortSearch] = useState("");
  const [copiedPort, flashCopiedPort] = useTransientValue<number>(1500);
  const [portsError, setPortsError] = useState<string | null>(null);
  const [stackFeedback, setStackFeedback] = useState<{ kind: "ok" | "err"; text: string } | null>(
    null,
  );
  const [httpProbes, setHttpProbes] = useState<Record<number, PortHttpProbe>>({});
  const [probingPorts, setProbingPorts] = useState(false);
  const portsInFlightRef = useRef(false);

  const refreshPorts = useCallback(async () => {
    if (portsInFlightRef.current) return;
    portsInFlightRef.current = true;
    try {
      const list = await invoke<ListeningPort[]>("fetch_listening_ports");
      setPorts(list);
      setPortsError(null);
    } catch (error) {
      setPortsError(String(error));
    } finally {
      portsInFlightRef.current = false;
    }
  }, []);

  const probeHttpPorts = async (targetPorts: number[]) => {
    if (targetPorts.length === 0) return;
    setProbingPorts(true);
    try {
      const results = await invoke<PortHttpProbe[]>("probe_http_ports", { ports: targetPorts });
      const mapped: Record<number, PortHttpProbe> = {};
      for (const item of results) {
        mapped[item.port] = item;
      }
      setHttpProbes((prev) => ({ ...prev, ...mapped }));
    } catch (err) {
      setPortsError(String(err));
    } finally {
      setProbingPorts(false);
    }
  };

  const copyPortUrl = async (port: number) => {
    try {
      await navigator.clipboard.writeText(`http://localhost:${port}`);
      flashCopiedPort(port);
    } catch {
    }
  };

  const stopProjectStack = async (projectPath: string, pids: number[]) => {
    let sent = 0;
    let failed = 0;
    for (const pid of pids) {
      if (pid <= 1) {
        failed += 1;
        continue;
      }
      try {
        await invoke("terminate_process", { pid });
        sent += 1;
      } catch {
        failed += 1;
      }
    }
    setStackFeedback(
      failed > 0
        ? {
            kind: "err",
            text: `${failed} of ${sent + failed} process(es) in ${projectPath} refused or had invalid PIDs; ${sent} signalled.`,
          }
        : {
            kind: "ok",
            text: `Sent SIGTERM to ${sent} process(es) in ${projectPath}.`,
          },
    );
    void refreshPorts();
    refreshExtras();
  };

  return {
    ports,
    portSearch,
    setPortSearch,
    copiedPort,
    portsError,
    stackFeedback,
    httpProbes,
    probingPorts,
    refreshPorts,
    probeHttpPorts,
    copyPortUrl,
    stopProjectStack,
  };
}
