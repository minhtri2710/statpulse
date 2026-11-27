import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search } from "lucide-react";
import { type AppTab, QUICK_SWITCHES, TABS } from "../../navigation";
import type {
  ApplicationPreferenceChange,
  AudioSnapshot,
  ContainerItem,
  LayoutDensity,
  ListeningPort,
  OllamaModelItem,
  ProcessReading,
  QuickSwitchId,
  ThermalPolicyMode,
  UserScriptCommand,
} from "../../types";
import { canUnloadModel } from "../agents/useAgentTools";
import { PALETTE_LOOK_IDS, lookName } from "../looks";
import { POLICY_MODES } from "../thermals/policyModes";

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  ports: ListeningPort[];
  processes: ProcessReading[];
  audio: AudioSnapshot | null;
  containers: ContainerItem[];
  ollamaModels: OllamaModelItem[];
  recordedCount: number | null;
  keepAwakeActive: boolean;
  focusRemainingSecs: number | null;
  dustBlastSecs: number;
  layoutDensity: LayoutDensity;
  islandCollapsed: boolean;
  onUpdatePreferences: (change: ApplicationPreferenceChange) => Promise<void>;
  onSelectPolicyMode: (mode: ThermalPolicyMode) => Promise<void>;
  onSelectTab: (tab: AppTab) => void;
  onOpenHealthReport: () => void;
  onTerminateProcess: (pid: number, label: string) => Promise<unknown>;
  onTogglePauseProcess: (pid: number, label: string, paused: boolean) => Promise<unknown>;
  onOpenInEditor: (path: string) => Promise<unknown>;
  onSetAudioDevice: (deviceId: number, kind: "output" | "input") => Promise<void>;
  onToggleMicMute: () => Promise<void>;
  onUnloadOllama: (name: string) => Promise<unknown>;
  onControlContainer: (id: string, action: "stop" | "restart") => Promise<unknown>;
  onRunUserScript: (scriptId: string) => Promise<void>;
  onToggleRecording: () => Promise<void>;
  onRunMaintenance: (action: "flush_dns" | "purge_ram") => Promise<unknown>;
  onCleanupOrphans: () => Promise<void>;
  onSetKeepAwake: (enabled: boolean, durationSecs: number | null) => Promise<void>;
  onStartFocusTimer: (secs: number | null) => void;
  onMediaControl: (action: "play_pause" | "next" | "previous") => Promise<void>;
  onRunQuickSwitch: (switchId: QuickSwitchId) => Promise<void>;
  onStartDustBlast: () => Promise<void>;
  onCancelDustBlast: () => Promise<void>;
  onRunDiskBenchmark: () => Promise<unknown>;
  onToggleIslandCollapsed: (collapsed: boolean) => Promise<void>;
}

export function CommandPalette({
  open,
  onClose,
  ports,
  processes,
  audio,
  containers,
  ollamaModels,
  recordedCount,
  keepAwakeActive,
  focusRemainingSecs,
  dustBlastSecs,
  layoutDensity,
  islandCollapsed,
  onUpdatePreferences,
  onSelectPolicyMode,
  onSelectTab,
  onOpenHealthReport,
  onTerminateProcess,
  onTogglePauseProcess,
  onOpenInEditor,
  onSetAudioDevice,
  onToggleMicMute,
  onUnloadOllama,
  onControlContainer,
  onRunUserScript,
  onToggleRecording,
  onRunMaintenance,
  onCleanupOrphans,
  onSetKeepAwake,
  onStartFocusTimer,
  onMediaControl,
  onRunQuickSwitch,
  onStartDustBlast,
  onCancelDustBlast,
  onRunDiskBenchmark,
  onToggleIslandCollapsed,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [userScripts, setUserScripts] = useState<UserScriptCommand[]>([]);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setSelectedIndex(0);
      invoke<UserScriptCommand[]>("list_user_scripts")
        .then(setUserScripts)
        .catch(() => setUserScripts([]));
    }
  }, [open]);

  const commandItems = useMemo(() => {
    if (!open) return [];
    const themeItems = PALETTE_LOOK_IDS.map((id) => ({
      id: `look-${id}`,
      label: `Theme: ${lookName(id)}`,
      category: "Theme",
      run: () => void onUpdatePreferences({ type: "set_popover_look", value: id }),
    }));

    const policyItems = POLICY_MODES.map((mode) => ({
      id: `pol-${mode.id}`,
      label: `Thermal Policy: ${"commandLabel" in mode ? mode.commandLabel : mode.label}`,
      category: "Fans",
      run: () => void onSelectPolicyMode(mode.id),
    }));

    const tabItems = TABS.map((tab) => ({
      id: `tab-${tab.id}`,
      label: `Open Tab: ${tab.label}`,
      category: "Navigate",
      run: () => onSelectTab(tab.id),
    }));

    const portKillItems = ports.slice(0, 8).map((p) => ({
      id: `kill-${p.port}-${p.pid}`,
      label: `Stop Port :${p.port} (${p.process_name})`,
      category: "Port",
      run: () => void onTerminateProcess(p.pid, `${p.process_name} :${p.port}`),
    }));

    const portEditorItems = ports
      .filter((p) => Boolean(p.project_path))
      .slice(0, 6)
      .map((p) => ({
        id: `ide-${p.port}-${p.pid}`,
        label: `Open :${p.port} (${p.process_name}) in IDE`,
        category: "IDE",
        run: () => void onOpenInEditor(p.project_path!),
      }));

    const processFreezeItems = processes
      .filter((p) => p.primary_pid > 1)
      .slice(0, 6)
      .map((p) => ({
        id: `freeze-${p.primary_pid}`,
        label: `${p.is_paused ? "Resume" : "Pause"} ${p.name} (${p.cpu_percent.toFixed(0)}% CPU)`,
        category: "Process",
        run: () => void onTogglePauseProcess(p.primary_pid, p.name, !p.is_paused),
      }));

    const audioOutItems = (audio?.output_devices ?? []).map((d) => ({
      id: `audio-out-${d.id}`,
      label: `Audio Out: ${d.name}${d.is_default_output ? " (Active)" : ""}`,
      category: "Audio",
      run: () => void onSetAudioDevice(d.id, "output"),
    }));

    const audioInItems = (audio?.input_devices ?? []).map((d) => ({
      id: `audio-in-${d.id}`,
      label: `Audio In: ${d.name}${d.is_default_input ? " (Active)" : ""}`,
      category: "Audio",
      run: () => void onSetAudioDevice(d.id, "input"),
    }));

    const containerItems = containers.slice(0, 6).map((c) => ({
      id: `ctr-restart-${c.id}`,
      label: `Restart Container: ${c.name}`,
      category: "Docker",
      run: () => void onControlContainer(c.id, "restart"),
    }));

    const ollamaItems = ollamaModels
      .filter(canUnloadModel)
      .map((m) => ({
        id: `ollama-unload-${m.name}`,
        label: `Unload LLM: ${m.name}`,
        category: "AI",
        run: () => void onUnloadOllama(m.name),
      }));

    const scriptItems = userScripts.map((s) => ({
      id: `script-${s.id}`,
      label: `Script: ${s.title}`,
      category: "Script",
      run: () => void onRunUserScript(s.id),
    }));

    const quickOps = [
      {
        id: "op-rec",
        label: recordedCount !== null ? "Stop & Copy Thermal Session" : "Start Thermal Session Recording",
        category: "Action",
        run: () => void onToggleRecording(),
      },
      {
        id: "op-health",
        label: "Toggle Mac Health Report",
        category: "Action",
        run: () => {
          onSelectTab("home");
          onOpenHealthReport();
        },
      },
      {
        id: "op-dns",
        label: "Flush DNS Directory Cache",
        category: "Action",
        run: () => void onRunMaintenance("flush_dns"),
      },
      {
        id: "op-orphans",
        label: "Clean Orphaned Dev Servers",
        category: "Action",
        run: () => void onCleanupOrphans(),
      },
      {
        id: "op-awake",
        label: keepAwakeActive ? "Disable Keep Awake" : "Keep Awake for 1h",
        category: "Awake",
        run: () => void onSetKeepAwake(!keepAwakeActive, keepAwakeActive ? null : 3600),
      },
      {
        id: "op-focus",
        label: focusRemainingSecs !== null ? "Stop Focus Timer" : "Start 25m Focus Timer",
        category: "Focus",
        run: () => onStartFocusTimer(focusRemainingSecs !== null ? null : 1500),
      },
      {
        id: "op-mic-mute",
        label: audio?.input_muted ? "Audio: Unmute Microphone" : "Audio: Mute Microphone",
        category: "Audio",
        run: () => void onToggleMicMute(),
      },
      {
        id: "op-media",
        label: "Media: Play / Pause Music or Spotify",
        category: "Audio",
        run: () => void onMediaControl("play_pause"),
      },
      ...QUICK_SWITCHES.map((sw) => ({
        id: `op-${sw.id}`,
        label: sw.commandLabel,
        category: "Switch",
        run: () => void onRunQuickSwitch(sw.id),
      })),
      {
        id: "op-dust-blast",
        label: dustBlastSecs > 0 ? "Stop 15s Fan Dust Blast" : "Run 15s Fan Dust Blast (100% RPM)",
        category: "Fans",
        run: () => void (dustBlastSecs > 0 ? onCancelDustBlast() : onStartDustBlast()),
      },
      {
        id: "op-disk-bench",
        label: "Run NVMe Disk Speed Benchmark (64 MB)",
        category: "System",
        run: () => {
          onSelectTab("system");
          void onRunDiskBenchmark();
        },
      },
      {
        id: "layout-dynamic-island",
        label: "Layout: Dynamic Island Capsule",
        category: "Layout",
        run: () => void onUpdatePreferences({ type: "set_layout_density", value: "dynamic_island" as LayoutDensity }),
      },
      {
        id: "layout-comfortable",
        label: "Layout: Comfortable Popover",
        category: "Layout",
        run: () => void onUpdatePreferences({ type: "set_layout_density", value: "comfortable" }),
      },
      {
        id: "layout-compact",
        label: "Layout: Compact Popover",
        category: "Layout",
        run: () => void onUpdatePreferences({ type: "set_layout_density", value: "compact" }),
      },
      {
        id: "layout-island-pill",
        label: islandCollapsed ? "Expand Dynamic Island" : "Collapse Dynamic Island to Mini Pill",
        category: "Layout",
        run: () => {
          if (layoutDensity !== "dynamic_island") {
            void onUpdatePreferences({ type: "set_layout_density", value: "dynamic_island" as LayoutDensity });
          } else {
            void onToggleIslandCollapsed(!islandCollapsed);
          }
        },
      },
    ];

    const all = [
      ...quickOps,
      ...portEditorItems,
      ...portKillItems,
      ...processFreezeItems,
      ...audioOutItems,
      ...audioInItems,
      ...ollamaItems,
      ...containerItems,
      ...scriptItems,
      ...policyItems,
      ...themeItems,
      ...tabItems,
    ];
    const q = query.trim().toLowerCase().replace(/^:/, "");
    if (!q) return all;
    return all.filter(
      (item) => item.label.toLowerCase().includes(q) || item.category.toLowerCase().includes(q),
    );
  }, [
    open,
    ports,
    processes,
    audio,
    containers,
    ollamaModels,
    userScripts,
    recordedCount,
    keepAwakeActive,
    focusRemainingSecs,
    dustBlastSecs,
    layoutDensity,
    islandCollapsed,
    query,
    onUpdatePreferences,
    onSelectPolicyMode,
    onSelectTab,
    onOpenHealthReport,
    onTerminateProcess,
    onTogglePauseProcess,
    onOpenInEditor,
    onSetAudioDevice,
    onToggleMicMute,
    onUnloadOllama,
    onControlContainer,
    onToggleRecording,
    onRunMaintenance,
    onCleanupOrphans,
    onSetKeepAwake,
    onStartFocusTimer,
    onMediaControl,
    onRunQuickSwitch,
    onStartDustBlast,
    onCancelDustBlast,
    onRunDiskBenchmark,
    onToggleIslandCollapsed,
  ]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/65 backdrop-blur-sm flex items-start justify-center pt-12 p-3"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[360px] rounded-xl border border-[var(--border)] bg-[var(--bg)] shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="pp-search rounded-none border-0 border-b border-[var(--card-border)] px-3 py-2">
          <Search className="w-3.5 h-3.5 shrink-0 text-[var(--acc)]" />
          <input
            type="text"
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              const visible = commandItems.slice(0, 14);
              if (e.key === "ArrowDown") {
                e.preventDefault();
                if (visible.length > 0) {
                  const next = (selectedIndex + 1) % visible.length;
                  setSelectedIndex(next);
                  (listRef.current?.children[next] as HTMLElement | undefined)?.scrollIntoView({
                    block: "nearest",
                  });
                }
                return;
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                if (visible.length > 0) {
                  const next = (selectedIndex - 1 + visible.length) % visible.length;
                  setSelectedIndex(next);
                  (listRef.current?.children[next] as HTMLElement | undefined)?.scrollIntoView({
                    block: "nearest",
                  });
                }
                return;
              }
              if (e.key === "Enter" && visible[selectedIndex]) {
                visible[selectedIndex].run();
                onClose();
              }
            }}
            placeholder="Type command, theme, policy, or :port..."
            aria-label="Quick command palette"
          />
        </div>

        <div ref={listRef} className="max-h-72 overflow-y-auto p-1.5 flex flex-col gap-0.5">
          {commandItems.length === 0 ? (
            <div className="text-[10px] text-[var(--sec)] px-2 py-3 text-center">
              No matching commands.
            </div>
          ) : (
            commandItems.slice(0, 14).map((cmd, idx) => (
              <button
                key={cmd.id}
                type="button"
                aria-selected={idx === selectedIndex}
                onMouseEnter={() => setSelectedIndex(idx)}
                onClick={() => {
                  cmd.run();
                  onClose();
                }}
                className={`w-full px-2.5 py-1.5 rounded-lg text-left flex items-center justify-between gap-2 text-[10.5px] text-[var(--ink)] cursor-pointer transition-colors ${
                  idx === selectedIndex
                    ? "bg-[var(--surface)] border border-[var(--acc)]/40"
                    : "border border-transparent hover:bg-[var(--surface)]"
                }`}
              >
                <span className="truncate">{cmd.label}</span>
                <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-[var(--rail)] text-[var(--sec)] border border-[var(--card-border)] shrink-0">
                  {cmd.category}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
