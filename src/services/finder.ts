import { invoke } from "@tauri-apps/api/core";

export function revealInFinder(path: string): Promise<void> {
  return invoke("reveal_in_finder", { path });
}
