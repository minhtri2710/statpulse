import { showHUD } from "@raycast/api";
import { runStatPulseCli } from "./statpulse";

export default async function Command() {
  try {
    const out = await runStatPulseCli(["cleanup-orphans"]);
    await showHUD(out || "Cleaned orphaned dev servers");
  } catch (err) {
    await showHUD(`Error: ${String(err)}`);
  }
}
