import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export function findStatPulseBinary(): string {
  const candidates = [
    "/Applications/StatPulse.app/Contents/MacOS/statpulse",
    "/opt/homebrew/bin/statpulse",
    "/usr/local/bin/statpulse",
  ];
  for (const p of candidates) {
    if (existsSync(p)) return p;
  }
  return "/Applications/StatPulse.app/Contents/MacOS/statpulse";
}

export async function runStatPulseCli(args: string[]): Promise<string> {
  const bin = findStatPulseBinary();
  const { stdout } = await execFileAsync(bin, args, { timeout: 8000 });
  return stdout.trim();
}
