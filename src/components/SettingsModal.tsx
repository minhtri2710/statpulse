import React, { useRef, useState } from "react";
import {
  ApplicationPreferenceChange,
  ApplicationPreferences,
  ConfigurationBundle,
  FanActuationStatus,
} from "../types";
import {
  ShieldCheck,
  ShieldAlert,
  Clock,
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
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { checkForUpdates, ReleaseInfo, CURRENT_VERSION } from "../services/updater";

interface SettingsModalProps {
  preferences: ApplicationPreferences;
  fanActuationStatus: FanActuationStatus;
  onUpdatePreferences: (change: ApplicationPreferenceChange) => void;
  onShowUpdateModal: (release: ReleaseInfo) => void;
  onHelperInstalled?: () => Promise<void> | void;
  onBundleImported?: (bundle: ConfigurationBundle) => void;
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
  const [installMsg, setInstallMsg] = useState<string | null>(null);

  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateStatusMsg, setUpdateStatusMsg] = useState<string | null>(null);

  const [showImportDrawer, setShowImportDrawer] = useState(false);
  const [importText, setImportText] = useState("");
  const [backupMsg, setBackupMsg] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const handleToggleAutostart = () => {
    onUpdatePreferences({
      type: "set_launch_at_login",
      value: !preferences.launch_at_login,
    });
  };

  const handleFanActuationAction = async () => {
    setInstalling(true);
    try {
      setInstallMsg(
        fanActuationStatus === "not_registered"
          ? "Installing Fan actuation helper..."
          : "Repairing Fan actuation helper...",
      );
      const result = await invoke<FanActuationStatus>("install_fan_actuation_helper");
      setInstallMsg(`Fan actuation helper status: ${result}`);
      if (onHelperInstalled) {
        await onHelperInstalled();
      }
    } catch (err: unknown) {
      setInstallMsg(`Error: ${String(err)}`);
    } finally {
      setInstalling(false);
    }
  };

  const handleCheckForUpdates = async () => {
    setCheckingUpdate(true);
    setUpdateStatusMsg(null);
    try {
      const res = await checkForUpdates();
      if (res.error) {
        setUpdateStatusMsg(`Check failed: ${res.error}`);
      } else if (res.hasUpdate && res.latestRelease) {
        setUpdateStatusMsg(`New version v${res.latestRelease.version} is available!`);
        onShowUpdateModal(res.latestRelease);
      } else {
        setUpdateStatusMsg(`StatPulse is up to date! (v${CURRENT_VERSION})`);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setUpdateStatusMsg(`Error: ${msg}`);
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
      const applied = await invoke<ConfigurationBundle>("import_configuration_bundle", {
        bundle: parsed,
      });
      onBundleImported?.(applied);
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

      {/* Temperature Unit */}
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
            onClick={() => onUpdatePreferences({ type: "set_temperature_unit", value: "celsius" })}
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
            onClick={() => onUpdatePreferences({ type: "set_temperature_unit", value: "fahrenheit" })}
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

      {/* Launch at Login (Autostart) */}
      <div className="glass-card p-3 rounded-xl flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Power className="w-4 h-4 text-emerald-400" />
          <div>
            <div className="text-xs font-semibold text-white">Launch at Login</div>
            <div className="text-[10px] text-slate-400">Auto start StatPulse when macOS turns on</div>
          </div>
        </div>

        <button
          type="button"
          onClick={handleToggleAutostart}
          className="text-slate-300 hover:text-white transition-colors"
          aria-label="Toggle Launch at Login"
        >
          {preferences.launch_at_login ? (
            <ToggleRight className="w-7 h-7 text-cyan-400" />
          ) : (
            <ToggleLeft className="w-7 h-7 text-slate-600" />
          )}
        </button>
      </div>

      {/* Polling Interval */}
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

      {/* Profile Backup & Share */}
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

      {/* Software Updates */}
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

      {/* Helper Status & Installation */}
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
                    ? "Helper needs repair; System Auto is active"
                    : "Required for manual fan speed modification"}
              </div>
            </div>
          </div>

          {fanActuationStatus === "ready" ? (
            <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-300 text-xs font-semibold">
              <ShieldCheck className="w-3.5 h-3.5" />
              <span>Active</span>
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
              installMsg.toLowerCase().startsWith("error")
                ? "bg-rose-950/40 border-rose-500/30 text-rose-200"
                : "bg-slate-900/60 border-white/10 text-slate-300"
            }`}
          >
            {installMsg.toLowerCase().startsWith("error") ? (
              <ShieldAlert className="w-3.5 h-3.5 text-rose-400 shrink-0 mt-0.5" />
            ) : (
              <CheckCircle2 className="w-3.5 h-3.5 text-cyan-400 shrink-0 mt-0.5" />
            )}
            <span className="break-all">{installMsg}</span>
          </div>
        )}
      </div>
    </div>
  );
};
