import { invoke } from "@tauri-apps/api/core";
import { name as cliName } from "../../../package.json";
import type { OllamaModelItem, SystemExtrasSnapshot } from "../../types";
import { type ActionFeedback, useTransientValue } from "../useTransient";
import { useState } from "react";

type ExtrasSetter = React.Dispatch<React.SetStateAction<SystemExtrasSnapshot | null>>;

export const canUnloadModel = (m: Pick<OllamaModelItem, "runtime">): boolean =>
  !m.runtime || m.runtime === "Ollama";

export function useAgentTools(
  setExtras: ExtrasSetter,
  setActionFeedback: (feedback: ActionFeedback | null) => void,
) {
  const [unloadingModel, setUnloadingModel] = useState<string | null>(null);
  const [controllingCtr, setControllingCtr] = useState<string | null>(null);
  const [copiedMcpConfig, flashCopiedMcpConfig] = useTransientValue<boolean>(1800);

  const unloadOllama = async (name: string) => {
    setUnloadingModel(name);
    try {
      const remaining = await invoke<SystemExtrasSnapshot["ollama_models"]>("unload_ollama_model", {
        name,
      });
      setExtras((prev) => (prev ? { ...prev, ollama_models: remaining } : prev));
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    } finally {
      setUnloadingModel(null);
    }
  };

  const controlContainer = async (id: string, action: "stop" | "restart") => {
    setControllingCtr(id);
    try {
      const updated = await invoke<SystemExtrasSnapshot["containers"]>("control_container", {
        id,
        action,
      });
      setExtras((prev) => (prev ? { ...prev, containers: updated } : prev));
    } catch (err) {
      setActionFeedback({ kind: "err", text: String(err) });
    } finally {
      setControllingCtr(null);
    }
  };

  const copyMcpConfig = async () => {
    const cfg = JSON.stringify(
      {
        mcpServers: {
          statpulse: {
            command: `/Applications/StatPulse.app/Contents/MacOS/${cliName}`,
            args: ["--mcp"],
          },
        },
      },
      null,
      2,
    );
    try {
      await navigator.clipboard.writeText(cfg);
      flashCopiedMcpConfig(true);
    } catch {
    }
  };

  return {
    unloadingModel,
    controllingCtr,
    copiedMcpConfig,
    unloadOllama,
    controlContainer,
    copyMcpConfig,
  };
}
