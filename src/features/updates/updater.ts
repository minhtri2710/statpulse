import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { version as CURRENT_VERSION } from "../../../package.json";
import type { UpdateOffer } from "../../types";

export { CURRENT_VERSION };

/// Native authority for update checks: resolves the official latest release,
/// verifies tag/asset/digest, and returns null when the build is current.
export async function checkForAppUpdate(): Promise<UpdateOffer | null> {
  return await invoke<UpdateOffer | null>("check_for_app_update");
}

export async function prepareAutoInstall(version: string): Promise<void> {
  await invoke("prepare_app_update", { version });
}

export async function installPreparedUpdate(version: string): Promise<void> {
  await invoke("install_prepared_app_update", { version });
}

export async function openReleasePage(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}

export async function discardPreparedUpdate(version: string): Promise<void> {
  await invoke("discard_prepared_app_update", { version });
}
