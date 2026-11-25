import { showHUD } from "@raycast/api";
import { runStatPulseCli } from "./statpulse";

export default async function Command() {
  try {
    const out = await runStatPulseCli(["audio", "mute-mic"]);
    await showHUD(out || "Toggled Microphone Mute");
  } catch (err) {
    await showHUD(`Error: ${String(err)}`);
  }
}
