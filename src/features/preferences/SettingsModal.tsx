import React, { useEffect, useRef, useState } from "react";
import {
  ApplicationPreferenceChange,
  ApplicationPreferences,
  ConfigurationBundle,
  FanActuationStatus,
  GlobalShortcutPreset,
} from "../../types";
import {
  Bell,
  ShieldCheck,
  ShieldAlert,
  Clock,
  Command,
  Thermometer,
  Wrench,
  CheckCircle2,
  Power,
  ToggleLeft,
  ToggleRight,
  RefreshCw,
  Sparkles,
  Download,
  Upload,
  Copy,
  MicOff,
  FolderSearch,
  Link2,
  Plus,
  X,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { checkForAppUpdate, CURRENT_VERSION } from "../updates/updater";
import type { UpdateOffer } from "../../types";
import { ActionFeedback, useTransientValue } from "../useTransient";
import {
  captureShortcutEvent,
  effectiveMicShortcutLabel,
} from "../../services/shortcutKeymap";

interface SettingsModalProps {
  preferences: ApplicationPreferences;
  fanActuationStatus: FanActuationStatus;
  onUpdatePreferences: (change: ApplicationPreferenceChange) => Promise<void>;
  onShowUpdateModal: (release: UpdateOffer) => void;
  onHelperInstalled?: () => Promise<void> | void;
  onBundleImported?: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  preferences,
  fanActuationStatus,
  onUpdatePreferences,
  onShowUpdateModal,
  onHelperInstalled,
  onBundleImported,
}) => {
  const [installing, setInstalling] = useState(false);
  const [installMsg, setInstallMsg] = useState<ActionFeedback | null>(null);

  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateStatusMsg, setUpdateStatusMsg] = useState<string | null>(null);

  const [showImportDrawer, setShowImportDrawer] = useState(false);
  const [importText, setImportText] = useState("");
  const [backupMsg, setBackupMsg] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [newWorkspaceRoot, setNewWorkspaceRoot] = useState("");
  const [copiedDeepLink, setCopiedDeepLink] = useTransientValue<string>(1500);
  const [recordingTarget, setRecordingTarget] = useState<"global" | "mic" | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  useEffect(() => {
    if (!recordingTarget) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const result = captureShortcutEvent(e);
      if (result.kind === "cancel") {
        setRecordingTarget(null);
        setShortcutError(null);
        return;
      }
      if (result.kind === "ignore") return;
      if (result.kind === "error") {
        setShortcutError(result.message);
        return;
      }
      const target = recordingTarget;
      setRecordingTarget(null);
      setShortcutError(null);
      const change: ApplicationPreferenceChange =
        target === "global"
          ? { type: "set_global_shortcut_custom", value: result.binding }
          : { type: "set_mic_mute_shortcut_custom", value: result.binding };
      void Promise.resolve(onUpdatePreferences(change)).catch((err: unknown) => {
        setShortcutError(err instanceof Error ? err.message : String(err));
      });
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [recordingTarget, onUpdatePreferences]);

  // Login-item status is only the OS-confirmed value: the stored preference
  // is a hint, not the truth, and a denied Automation permission must show as
  // unavailable rather than pretending to be disabled.
  const [loginState, setLoginState] = useState<"checking" | "unavailable" | boolean>("checking");
  const [loginPending, setLoginPending] = useState(false);
  const [loginProbe, setLoginProbe] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoginState("checking");
    invoke<ApplicationPreferences>("confirmed_application_preferences")
      .then((prefs) => {
        if (!cancelled) setLoginState(prefs.launch_at_login);
      })
      .catch(() => {
        if (!cancelled) setLoginState("unavailable");
      });
    return () => {
      cancelled = true;
    };
  }, [loginProbe]);

  const handleToggleAutostart = async () => {
    if (loginState === "checking" || loginState === "unavailable" || loginPending) return;
    setLoginPending(true);
    try {
      await onUpdatePreferences({
        type: "set_launch_at_login",
        value: !loginState,
      });
      const confirmed = await invoke<ApplicationPreferences>(
        "confirmed_application_preferences",
      );
      setLoginState(confirmed.launch_at_login);
    } catch {
      setLoginProbe((probe) => probe + 1);
    } finally {
      setLoginPending(false);
    }
  };

  const handleFanActuationAction = async () => {
    setInstalling(true);
    try {
      setInstallMsg({
        kind: "ok",
        text:
          fanActuationStatus === "ready"
            ? "Reinstalling Fan actuation helper..."
            : fanActuationStatus === "not_registered"
              ? "Installing Fan actuation helper..."
              : "Repairing Fan actuation helper...",
      });
      const result = await invoke<FanActuationStatus>("install_fan_actuation_helper", {
        force: fanActuationStatus === "ready",
      });
      setInstallMsg({ kind: "ok", text: `Fan actuation helper status: ${result}` });
      if (onHelperInstalled) {
        await onHelperInstalled();
      }
    } catch (err: unknown) {
      setInstallMsg({ kind: "err", text: `Error: ${String(err)}` });
    } finally {
      setInstalling(false);
    }
  };

  const handleCheckForUpdates = async () => {
    setCheckingUpdate(true);
    setUpdateStatusMsg(null);
    try {
      const offer = await checkForAppUpdate();
      if (offer) {
        setUpdateStatusMsg(`New version v${offer.version} is available!`);
        onShowUpdateModal(offer);
      } else {
        setUpdateStatusMsg(`StatPulse is up to date! (v${CURRENT_VERSION})`);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setUpdateStatusMsg(`Check failed: ${msg}`);
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleCopyProfileJson = async () => {
    try {
      const bundle = await invoke<ConfigurationBundle>("export_configuration_bundle");
      const json = JSON.stringify(bundle, null, 2);
      await navigator.clipboard.writeText(json);
      setBackupMsg({
        kind: "ok",
        text: "Profile JSON copied to clipboard (preferences + thermal rules).",
      });
    } catch (err: unknown) {
      setBackupMsg({
        kind: "err",
        text: `Failed to copy profile: ${String(err)}`,
      });
    }
  };

  const handleDownloadProfileJson = async () => {
    try {
      const bundle = await invoke<ConfigurationBundle>("export_configuration_bundle");
      const json = JSON.stringify(bundle, null, 2);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `statpulse-profile-v${bundle.version}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setBackupMsg({
        kind: "ok",
        text: `Downloaded statpulse-profile-v${bundle.version}.json.`,
      });
    } catch (err: unknown) {
      setBackupMsg({
        kind: "err",
        text: `Failed to export file: ${String(err)}`,
      });
    }
  };

  const applyImportedJson = async (rawJson: string) => {
    try {
      const parsed = JSON.parse(rawJson) as ConfigurationBundle;
      await invoke<ConfigurationBundle>("import_configuration_bundle", {
        bundle: parsed,
      });
      onBundleImported?.();
      setLoginProbe((probe) => probe + 1);
      setShowImportDrawer(false);
      setImportText("");
      setBackupMsg({
        kind: "ok",
        text: "Profile imported and applied to menu bar & thermal rules.",
      });
    } catch (err: unknown) {
      setBackupMsg({
        kind: "err",
        text: `Import failed: ${String(err)}`,
      });
    }
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const content = typeof reader.result === "string" ? reader.result : "";
      setImportText(content);
      void applyImportedJson(content);
    };
    reader.readAsText(file);
    e.target.value = "";
  };

  return (
    <div className="flex flex-col gap-3 p-1">
      <h2 className="text-xs font-bold uppercase tracking-wider text-slate-400">Application Settings</h2>
      <div className="glass-card p-3 rounded-xl flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Thermometer className="w-4 h-4 text-cyan-400" />
          <div>
            <div className="text-xs font-semibold text-white">Temperature Unit</div>
            <div className="text-[10px] text-slate-400">Display values in Celsius or Fahrenheit</div>
          </div>
        </div>

        <div className="flex items-center p-0.5 bg-slate-900/60 rounded-lg border border-white/5 text-[11px]">
          <button
            type="button"
            onClick={() => void onUpdatePreferences({ type: "set_temperature_unit", value: "celsius" })}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              preferences.temperature_unit === "celsius"
                ? "bg-cyan-500/20 text-cyan-300 border border-cyan-500/30"
                : "text-slate-400 hover:text-white"
            }`}
          >
            °C
          </button>
          <button
            type="button"
            onClick={() => void onUpdatePreferences({ type: "set_temperature_unit", value: "fahrenheit" })}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              preferences.temperature_unit === "fahrenheit"
                ? "bg-cyan-500/20 text-cyan-300 border border-cyan-500/30"
                : "text-slate-400 hover:text-white"
            }`}
          >
            °F
          </button>
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Power className="w-4 h-4 text-emerald-400" />
          <div>
            <div className="text-xs font-semibold text-white">Launch at Login</div>
            <div className="text-[10px] text-slate-400">Auto start StatPulse when macOS turns on</div>
          </div>
        </div>

        {loginState === "checking" ? (
          <span className="text-[10px] text-slate-500">Checking...</span>
        ) : loginState === "unavailable" ? (
          <span className="flex items-center gap-1">
            <span className="text-[10px] text-slate-500">Unavailable</span>
            <button
              type="button"
              onClick={() => setLoginProbe((probe) => probe + 1)}
              className="text-[10px] text-cyan-400 hover:text-cyan-300 transition-colors"
            >
              Retry
            </button>
          </span>
        ) : (
          <button
            type="button"
            onClick={() => void handleToggleAutostart()}
            disabled={loginPending}
            className="text-slate-300 hover:text-white transition-colors disabled:opacity-50"
            aria-label="Toggle Launch at Login"
          >
            {loginState ? (
              <ToggleRight className="w-7 h-7 text-cyan-400" />
            ) : (
              <ToggleLeft className="w-7 h-7 text-slate-600" />
            )}
          </button>
        )}
      </div>
      <div className="glass-card p-3 rounded-xl flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Clock className="w-4 h-4 text-amber-400" />
          <div>
            <div className="text-xs font-semibold text-white">Refresh Rate</div>
            <div className="text-[10px] text-slate-400">Telemetry polling frequency</div>
          </div>
        </div>

        <select
          value={preferences.telemetry_interval_ms}
          onChange={(e) =>
            onUpdatePreferences({ type: "set_telemetry_interval_ms", value: Number(e.target.value) })
          }
          className="bg-slate-900/80 border border-white/10 text-white text-xs rounded-lg px-2 py-1 outline-none font-mono cursor-pointer"
        >
          <option value={1000}>1.0s (Fast)</option>
          <option value={1500}>1.5s (Balanced)</option>
          <option value={2500}>2.5s (Eco)</option>
        </select>
      </div>

      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Command className="w-4 h-4 text-cyan-400" />
            <div>
              <div className="text-xs font-semibold text-white">Global Hotkey</div>
              <div className="text-[10px] text-slate-400">Toggle StatPulse from anywhere</div>
            </div>
          </div>

          <div className="flex items-center gap-1.5">
            <select
              value={preferences.global_shortcut}
              onChange={(e) => {
                const next = e.target.value as GlobalShortcutPreset;
                setShortcutError(null);
                if (next === "custom" && !preferences.global_shortcut_custom) {
                  setRecordingTarget("global");
                  return;
                }
                void Promise.resolve(
                  onUpdatePreferences({ type: "set_global_shortcut", value: next }),
                ).catch((err: unknown) => {
                  setShortcutError(err instanceof Error ? err.message : String(err));
                });
              }}
              className="bg-slate-900/80 border border-white/10 text-white text-xs rounded-lg px-2 py-1 outline-none font-mono cursor-pointer"
            >
              <option value="cmd_shift_s">⌘⇧S</option>
              <option value="opt_space">⌥Space</option>
              <option value="custom">
                {preferences.global_shortcut_custom
                  ? `Custom (${preferences.global_shortcut_custom.label})`
                  : "Custom…"}
              </option>
              <option value="disabled">Disabled</option>
            </select>
            <button
              type="button"
              onClick={() => {
                setShortcutError(null);
                setRecordingTarget((prev) => (prev === "global" ? null : "global"));
              }}
              className={`px-2 py-1 rounded-md text-[10px] font-semibold border transition-colors cursor-pointer ${
                recordingTarget === "global"
                  ? "bg-amber-500/20 text-amber-300 border-amber-500/40 animate-pulse"
                  : "bg-slate-900/60 text-slate-300 hover:text-white border-white/10"
              }`}
              title="Press a custom key combination (Esc to cancel)"
            >
              {recordingTarget === "global" ? "Press keys…" : "Record"}
            </button>
          </div>
        </div>
      </div>

      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <MicOff className="w-4 h-4 text-rose-400" />
            <div>
              <div className="text-xs font-semibold text-white">Global Mic Mute Hotkey</div>
              <div className="text-[10px] text-slate-400">
                Toggle default microphone mute with{" "}
                {effectiveMicShortcutLabel(preferences) ??
                  preferences.mic_mute_shortcut_custom?.label ??
                  "⌘⇧M"}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            {preferences.mic_mute_shortcut_custom && (
              <button
                type="button"
                onClick={() => {
                  setShortcutError(null);
                  void Promise.resolve(
                    onUpdatePreferences({ type: "set_mic_mute_shortcut_custom", value: null }),
                  ).catch((err: unknown) => {
                    setShortcutError(err instanceof Error ? err.message : String(err));
                  });
                }}
                className="px-1.5 py-1 rounded-md text-[9.5px] text-slate-400 hover:text-white border border-white/10 bg-slate-900/60 cursor-pointer"
                title="Reset to default ⌘⇧M"
              >
                Reset
              </button>
            )}
            <button
              type="button"
              onClick={() => {
                setShortcutError(null);
                setRecordingTarget((prev) => (prev === "mic" ? null : "mic"));
              }}
              className={`px-2 py-1 rounded-md text-[10px] font-semibold border transition-colors cursor-pointer ${
                recordingTarget === "mic"
                  ? "bg-amber-500/20 text-amber-300 border-amber-500/40 animate-pulse"
                  : "bg-slate-900/60 text-slate-300 hover:text-white border-white/10"
              }`}
              title="Press a custom key combination (Esc to cancel)"
            >
              {recordingTarget === "mic" ? "Press keys…" : "Record"}
            </button>
            <button
              type="button"
              onClick={() => {
                setShortcutError(null);
                void Promise.resolve(
                  onUpdatePreferences({
                    type: "set_mic_mute_shortcut",
                    value: !preferences.mic_mute_shortcut,
                  }),
                ).catch((err: unknown) => {
                  setShortcutError(err instanceof Error ? err.message : String(err));
                });
              }}
              className={`px-2.5 py-1 rounded-md text-[10px] font-semibold border transition-colors cursor-pointer ${
                preferences.mic_mute_shortcut
                  ? "bg-cyan-500/20 text-cyan-300 border-cyan-500/30"
                  : "bg-slate-900/60 text-slate-400 border-white/10"
              }`}
            >
              {preferences.mic_mute_shortcut
                ? `${effectiveMicShortcutLabel(preferences)} Active`
                : "Off"}
            </button>
          </div>
        </div>
        {(recordingTarget !== null || shortcutError) && (
          <div
            className={`text-[10px] px-2 py-1 rounded-md border ${
              shortcutError
                ? "bg-rose-500/10 border-rose-500/30 text-rose-300"
                : "bg-amber-500/10 border-amber-500/30 text-amber-200"
            }`}
          >
            {shortcutError ??
              "Press your desired shortcut (include ⌘, ⌥, or ⌃; F1–F20 also allow ⇧). Press Esc to cancel."}
          </div>
        )}
      </div>

      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <FolderSearch className="w-4 h-4 text-emerald-400" />
          <div>
            <div className="text-xs font-semibold text-white">Workspace Scan Folders</div>
            <div className="text-[10px] text-slate-400">
              Home-relative folders scanned for stale node_modules, target, .next &amp; .venv
            </div>
          </div>
        </div>
        <div className="flex flex-wrap gap-1">
          {preferences.workspace_roots.map((root) => (
            <span
              key={root}
              className="inline-flex items-center gap-1 px-2 py-0.5 rounded bg-slate-900/80 border border-white/10 text-[10px] font-mono text-slate-200"
            >
              ~/{root}
              {preferences.workspace_roots.length > 1 && (
                <button
                  type="button"
                  onClick={() =>
                    void onUpdatePreferences({
                      type: "set_workspace_roots",
                      value: preferences.workspace_roots.filter((r) => r !== root),
                    })
                  }
                  className="text-slate-400 hover:text-rose-300"
                  aria-label={`Remove ${root}`}
                >
                  <X className="w-2.5 h-2.5" />
                </button>
              )}
            </span>
          ))}
          {["Projects", "Developer", "Code"]
            .filter((p) => !preferences.workspace_roots.includes(p))
            .map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() =>
                  void onUpdatePreferences({
                    type: "set_workspace_roots",
                    value: [...preferences.workspace_roots, preset],
                  })
                }
                className="px-2 py-0.5 rounded border border-dashed border-white/15 text-[9.5px] font-mono text-slate-400 hover:text-white"
              >
                + ~/{preset}
              </button>
            ))}
        </div>
        <div className="flex items-center gap-1.5">
          <input
            type="text"
            value={newWorkspaceRoot}
            onChange={(e) => setNewWorkspaceRoot(e.target.value)}
            placeholder="Folder name under $HOME (e.g. Repos)"
            className="flex-1 bg-slate-900/80 border border-white/10 rounded-md px-2.5 py-1.5 text-[10px] font-mono text-white outline-none focus:border-cyan-400"
          />
          <button
            type="button"
            onClick={() => {
              const v = newWorkspaceRoot.trim();
              if (!v || preferences.workspace_roots.includes(v)) return;
              void onUpdatePreferences({
                type: "set_workspace_roots",
                value: [...preferences.workspace_roots, v],
              });
              setNewWorkspaceRoot("");
            }}
            className="px-2 py-1 rounded-md bg-slate-800 hover:bg-slate-700 border border-white/10 text-[10px] font-semibold text-slate-200 inline-flex items-center gap-1"
          >
            <Plus className="w-3 h-3" />
            Add
          </button>
        </div>
      </div>

      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <Link2 className="w-4 h-4 text-cyan-400" />
          <div>
            <div className="text-xs font-semibold text-white">Raycast &amp; Deep Links</div>
            <div className="text-[10px] text-slate-400">
              Trigger actions from Raycast, Shortcuts, or Terminal (<code className="text-cyan-300">statpulse://</code>)
            </div>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-1.5 text-[9.5px] font-mono">
          {[
            "statpulse://fan/performance",
            "statpulse://mic/toggle",
            "statpulse://awake/1h",
            "statpulse://dust-blast",
          ].map((link) => (
            <button
              key={link}
              type="button"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(link);
                  setCopiedDeepLink(link);
                } catch {
                  // Clipboard write can fail when window loses focus.
                }
              }}
              className="px-2 py-1 rounded bg-slate-900/80 border border-white/10 text-left text-slate-300 hover:border-cyan-400/40 truncate flex items-center justify-between gap-1"
            >
              <span className="truncate">{link}</span>
              <span className="text-[8.5px] text-cyan-400 shrink-0">
                {copiedDeepLink === link ? "Copied" : "Copy"}
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Bell className="w-4 h-4 text-amber-400" />
            <div>
              <div className="text-xs font-semibold text-white">Smart Alerts &amp; Thresholds</div>
              <div className="text-[10px] text-slate-400">
                Shown on Dynamic Island pill &amp; panel (optional macOS banner)
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={() =>
              void onUpdatePreferences({
                type: "set_alerts",
                value: {
                  ...preferences.alerts,
                  native_notifications: !preferences.alerts.native_notifications,
                },
              })
            }
            className={`px-2 py-1 rounded-md text-[10px] font-semibold border transition-colors ${
              preferences.alerts.native_notifications
                ? "bg-amber-500/20 text-amber-300 border-amber-500/30"
                : "bg-cyan-500/20 text-cyan-300 border-cyan-500/30"
            }`}
          >
            {preferences.alerts.native_notifications ? "+ macOS Banner" : "Island Only"}
          </button>
        </div>

        <div className="grid grid-cols-3 gap-1.5 text-[10px]">
          <label className="flex flex-col gap-0.5 text-slate-400">
            <span>CPU Alert</span>
            <select
              value={preferences.alerts.high_cpu_celsius}
              onChange={(e) =>
                void onUpdatePreferences({
                  type: "set_alerts",
                  value: { ...preferences.alerts, high_cpu_celsius: Number(e.target.value) },
                })
              }
              className="bg-slate-900/80 border border-white/10 text-white rounded px-1.5 py-1 font-mono"
            >
              <option value={80}>80°C</option>
              <option value={85}>85°C</option>
              <option value={90}>90°C</option>
              <option value={95}>95°C</option>
            </select>
          </label>

          <label className="flex flex-col gap-0.5 text-slate-400">
            <span>Swap Alert</span>
            <select
              value={preferences.alerts.high_swap_mb}
              onChange={(e) =>
                void onUpdatePreferences({
                  type: "set_alerts",
                  value: { ...preferences.alerts, high_swap_mb: Number(e.target.value) },
                })
              }
              className="bg-slate-900/80 border border-white/10 text-white rounded px-1.5 py-1 font-mono"
            >
              <option value={2048}>2 GB</option>
              <option value={4096}>4 GB</option>
              <option value={8192}>8 GB</option>
            </select>
          </label>

          <label className="flex flex-col gap-0.5 text-slate-400">
            <span>Charge Target</span>
            <select
              value={preferences.alerts.notify_charge_limit ? preferences.alerts.charge_limit_percent : 0}
              onChange={(e) => {
                const v = Number(e.target.value);
                void onUpdatePreferences({
                  type: "set_alerts",
                  value: {
                    ...preferences.alerts,
                    notify_charge_limit: v > 0,
                    charge_limit_percent: v > 0 ? v : preferences.alerts.charge_limit_percent,
                  },
                });
              }}
              className="bg-slate-900/80 border border-white/10 text-white rounded px-1.5 py-1 font-mono"
            >
              <option value={0}>Off</option>
              <option value={80}>80%</option>
              <option value={85}>85%</option>
            </select>
          </label>
        </div>
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2.5">
        <div className="flex items-center justify-between gap-2">
          <div>
            <div className="text-xs font-semibold text-white">Profile Backup &amp; Share</div>
            <div className="text-[10px] text-slate-400">
              Export or restore your look, menu bar &amp; thermal rules
            </div>
          </div>
        </div>

        <div className="grid grid-cols-3 gap-1.5 text-[10px]">
          <button
            type="button"
            onClick={handleCopyProfileJson}
            className="px-2 py-1.5 rounded-lg bg-slate-900/75 hover:bg-slate-800 border border-white/10 text-slate-200 font-semibold flex items-center justify-center gap-1 transition-colors"
          >
            <Copy className="w-3 h-3 text-cyan-400" />
            Copy JSON
          </button>

          <button
            type="button"
            onClick={handleDownloadProfileJson}
            className="px-2 py-1.5 rounded-lg bg-slate-900/75 hover:bg-slate-800 border border-white/10 text-slate-200 font-semibold flex items-center justify-center gap-1 transition-colors"
          >
            <Download className="w-3 h-3 text-emerald-400" />
            Export File
          </button>

          <button
            type="button"
            onClick={() => setShowImportDrawer((v) => !v)}
            className={`px-2 py-1.5 rounded-lg border font-semibold flex items-center justify-center gap-1 transition-colors ${
              showImportDrawer
                ? "bg-cyan-500/20 border-cyan-400/40 text-cyan-200"
                : "bg-slate-900/75 hover:bg-slate-800 border-white/10 text-slate-200"
            }`}
          >
            <Upload className="w-3 h-3 text-amber-400" />
            Import
          </button>
        </div>

        {showImportDrawer && (
          <div className="p-2.5 rounded-lg bg-slate-950/85 border border-white/10 flex flex-col gap-2">
            <textarea
              rows={4}
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              placeholder='Paste exported JSON bundle ({"version": 1, ...})'
              className="w-full rounded-lg bg-slate-900/90 border border-white/10 p-2 text-[10px] font-mono text-slate-200 outline-none focus:border-cyan-400 resize-none"
            />
            <div className="flex items-center justify-between gap-2">
              <input
                ref={fileInputRef}
                type="file"
                accept=".json,application/json"
                onChange={handleFileUpload}
                className="hidden"
              />
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="px-2.5 py-1 rounded-md bg-slate-800 hover:bg-slate-700 border border-white/10 text-[10px] font-semibold text-slate-200 transition-colors"
              >
                Load .json File
              </button>
              <button
                type="button"
                disabled={!importText.trim()}
                onClick={() => void applyImportedJson(importText)}
                className="px-3 py-1 rounded-md bg-cyan-500/20 hover:bg-cyan-500/30 disabled:opacity-40 border border-cyan-400/40 text-[10px] font-bold text-cyan-200 transition-colors"
              >
                Apply Profile
              </button>
            </div>
          </div>
        )}

        {backupMsg && (
          <div
            className={`p-2 rounded-lg border text-[10px] font-mono flex items-start gap-1.5 ${
              backupMsg.kind === "err"
                ? "bg-rose-950/40 border-rose-500/30 text-rose-200"
                : "bg-slate-900/60 border-white/10 text-slate-300"
            }`}
          >
            {backupMsg.kind === "err" ? (
              <ShieldAlert className="w-3.5 h-3.5 text-rose-400 shrink-0 mt-0.5" />
            ) : (
              <CheckCircle2 className="w-3.5 h-3.5 text-cyan-400 shrink-0 mt-0.5" />
            )}
            <span className="break-words">{backupMsg.text}</span>
          </div>
        )}
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-amber-400" />
            <div>
              <div className="text-xs font-semibold text-white">Software Update</div>
              <div className="text-[10px] text-slate-400">Version {CURRENT_VERSION}</div>
            </div>
          </div>

          <button
            type="button"
            onClick={handleCheckForUpdates}
            disabled={checkingUpdate}
            className="px-3 py-1.5 rounded-lg text-xs font-semibold bg-slate-800 text-slate-200 hover:bg-slate-700 hover:text-white border border-white/10 flex items-center gap-1.5 transition-all shadow-md"
          >
            <RefreshCw className={`w-3 h-3 ${checkingUpdate ? "animate-spin text-amber-400" : ""}`} />
            {checkingUpdate ? "Checking..." : "Check for Updates"}
          </button>
        </div>

        {updateStatusMsg && (
          <div className="mt-1 p-2 rounded-lg bg-slate-900/60 border border-white/10 text-[10px] font-mono text-slate-300 flex items-start gap-1.5">
            <CheckCircle2 className="w-3.5 h-3.5 text-cyan-400 shrink-0 mt-0.5" />
            <span>{updateStatusMsg}</span>
          </div>
        )}
      </div>
      <div className="glass-card p-3 rounded-xl flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            {fanActuationStatus === "ready" ? (
              <ShieldCheck className="w-4 h-4 text-emerald-400" />
            ) : (
              <ShieldAlert className="w-4 h-4 text-amber-400" />
            )}
            <div>
              <div className="text-xs font-semibold text-white">Fan Actuation Service</div>
              <div className="text-[10px] text-slate-400">
                {fanActuationStatus === "ready"
                  ? "Privileged service is ready"
                  : fanActuationStatus === "unavailable"
                    ? "Repair Helper to enable manual fan control."
                    : "Required for manual fan speed modification"}
              </div>
            </div>
          </div>

          {fanActuationStatus === "ready" ? (
            <div className="flex items-center gap-1.5">
              <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-300 text-xs font-semibold">
                <ShieldCheck className="w-3.5 h-3.5" />
                <span>Active</span>
              </div>
              <button
                type="button"
                onClick={handleFanActuationAction}
                disabled={installing}
                title="Reinstall bundled Fan actuation helper"
                className="px-2.5 py-1 rounded-lg text-xs font-semibold bg-slate-800 text-slate-200 hover:bg-slate-700 hover:text-white border border-white/10 flex items-center gap-1.5 transition-all disabled:opacity-50"
              >
                <RefreshCw className={`w-3 h-3 ${installing ? "animate-spin text-cyan-400" : ""}`} />
                {installing ? "Reinstalling..." : "Reinstall"}
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={handleFanActuationAction}
              disabled={installing}
              className="px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all shadow-md bg-gradient-to-r from-amber-500 to-orange-500 text-white hover:from-amber-600 hover:to-orange-600 shadow-orange-500/20"
            >
              <Wrench className="w-3 h-3" />
              {installing
                ? "Installing..."
                : fanActuationStatus === "not_registered"
                  ? "Install Helper"
                  : "Repair Helper"}
            </button>
          )}
        </div>

        {installMsg && (
          <div
            className={`mt-1 p-2 rounded-lg border text-[10px] font-mono flex items-start gap-1.5 ${
              installMsg.kind === "err"
                ? "bg-rose-950/40 border-rose-500/30 text-rose-200"
                : "bg-slate-900/60 border-white/10 text-slate-300"
            }`}
          >
            {installMsg.kind === "err" ? (
              <ShieldAlert className="w-3.5 h-3.5 text-rose-400 shrink-0 mt-0.5" />
            ) : (
              <CheckCircle2 className="w-3.5 h-3.5 text-cyan-400 shrink-0 mt-0.5" />
            )}
            <span className="break-all">{installMsg.text}</span>
          </div>
        )}
      </div>
    </div>
  );
};
