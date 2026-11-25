import type { ApplicationPreferences, CustomShortcutBinding } from "../types.ts";

export interface ShortcutKeyInfo {
  keyCode: number;
  label: string;
  isFunctionKey?: boolean;
}

const CODE_TO_CARBON: Record<string, ShortcutKeyInfo> = {
  KeyA: { keyCode: 0x00, label: "A" },
  KeyS: { keyCode: 0x01, label: "S" },
  KeyD: { keyCode: 0x02, label: "D" },
  KeyF: { keyCode: 0x03, label: "F" },
  KeyH: { keyCode: 0x04, label: "H" },
  KeyG: { keyCode: 0x05, label: "G" },
  KeyZ: { keyCode: 0x06, label: "Z" },
  KeyX: { keyCode: 0x07, label: "X" },
  KeyC: { keyCode: 0x08, label: "C" },
  KeyV: { keyCode: 0x09, label: "V" },
  KeyB: { keyCode: 0x0b, label: "B" },
  KeyQ: { keyCode: 0x0c, label: "Q" },
  KeyW: { keyCode: 0x0d, label: "W" },
  KeyE: { keyCode: 0x0e, label: "E" },
  KeyR: { keyCode: 0x0f, label: "R" },
  KeyY: { keyCode: 0x10, label: "Y" },
  KeyT: { keyCode: 0x11, label: "T" },
  Digit1: { keyCode: 0x12, label: "1" },
  Digit2: { keyCode: 0x13, label: "2" },
  Digit3: { keyCode: 0x14, label: "3" },
  Digit4: { keyCode: 0x15, label: "4" },
  Digit6: { keyCode: 0x16, label: "6" },
  Digit5: { keyCode: 0x17, label: "5" },
  Equal: { keyCode: 0x18, label: "=" },
  Digit9: { keyCode: 0x19, label: "9" },
  Digit7: { keyCode: 0x1a, label: "7" },
  Minus: { keyCode: 0x1b, label: "-" },
  Digit8: { keyCode: 0x1c, label: "8" },
  Digit0: { keyCode: 0x1d, label: "0" },
  BracketRight: { keyCode: 0x1e, label: "]" },
  KeyO: { keyCode: 0x1f, label: "O" },
  KeyU: { keyCode: 0x20, label: "U" },
  BracketLeft: { keyCode: 0x21, label: "[" },
  KeyI: { keyCode: 0x22, label: "I" },
  KeyP: { keyCode: 0x23, label: "P" },
  Enter: { keyCode: 0x24, label: "↩" },
  Return: { keyCode: 0x24, label: "↩" },
  KeyL: { keyCode: 0x25, label: "L" },
  KeyJ: { keyCode: 0x26, label: "J" },
  Quote: { keyCode: 0x27, label: "'" },
  KeyK: { keyCode: 0x28, label: "K" },
  Semicolon: { keyCode: 0x29, label: ";" },
  Backslash: { keyCode: 0x2a, label: "\\" },
  Comma: { keyCode: 0x2b, label: "," },
  Slash: { keyCode: 0x2c, label: "/" },
  KeyN: { keyCode: 0x2d, label: "N" },
  KeyM: { keyCode: 0x2e, label: "M" },
  Period: { keyCode: 0x2f, label: "." },
  Tab: { keyCode: 0x30, label: "⇥" },
  Space: { keyCode: 0x31, label: "Space" },
  Backquote: { keyCode: 0x32, label: "`" },
  Backspace: { keyCode: 0x33, label: "⌫" },
  Delete: { keyCode: 0x75, label: "⌦" },
  ForwardDelete: { keyCode: 0x75, label: "⌦" },
  ArrowLeft: { keyCode: 0x7b, label: "←" },
  ArrowRight: { keyCode: 0x7c, label: "→" },
  ArrowDown: { keyCode: 0x7d, label: "↓" },
  ArrowUp: { keyCode: 0x7e, label: "↑" },
  Home: { keyCode: 0x73, label: "Home" },
  End: { keyCode: 0x77, label: "End" },
  PageUp: { keyCode: 0x74, label: "PgUp" },
  PageDown: { keyCode: 0x79, label: "PgDn" },
  F1: { keyCode: 0x7a, label: "F1", isFunctionKey: true },
  F2: { keyCode: 0x78, label: "F2", isFunctionKey: true },
  F3: { keyCode: 0x63, label: "F3", isFunctionKey: true },
  F4: { keyCode: 0x76, label: "F4", isFunctionKey: true },
  F5: { keyCode: 0x60, label: "F5", isFunctionKey: true },
  F6: { keyCode: 0x61, label: "F6", isFunctionKey: true },
  F7: { keyCode: 0x62, label: "F7", isFunctionKey: true },
  F8: { keyCode: 0x64, label: "F8", isFunctionKey: true },
  F9: { keyCode: 0x65, label: "F9", isFunctionKey: true },
  F10: { keyCode: 0x6d, label: "F10", isFunctionKey: true },
  F11: { keyCode: 0x67, label: "F11", isFunctionKey: true },
  F12: { keyCode: 0x6f, label: "F12", isFunctionKey: true },
  F13: { keyCode: 0x69, label: "F13", isFunctionKey: true },
  F14: { keyCode: 0x6b, label: "F14", isFunctionKey: true },
  F15: { keyCode: 0x71, label: "F15", isFunctionKey: true },
  F16: { keyCode: 0x6a, label: "F16", isFunctionKey: true },
  F17: { keyCode: 0x40, label: "F17", isFunctionKey: true },
  F18: { keyCode: 0x4f, label: "F18", isFunctionKey: true },
  F19: { keyCode: 0x50, label: "F19", isFunctionKey: true },
  F20: { keyCode: 0x5a, label: "F20", isFunctionKey: true },
};

const MODIFIER_CODES = new Set([
  "MetaLeft",
  "MetaRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "CapsLock",
]);

export type ShortcutCaptureResult =
  | { kind: "cancel" }
  | { kind: "ignore" }
  | { kind: "error"; message: string }
  | { kind: "captured"; binding: CustomShortcutBinding };

export function formatShortcutLabel(modifiers: number, keyLabel: string): string {
  const parts: string[] = [];
  if (modifiers & 1) parts.push("⌘");
  if (modifiers & 8) parts.push("⌃");
  if (modifiers & 4) parts.push("⌥");
  if (modifiers & 2) parts.push("⇧");
  parts.push(keyLabel);
  return parts.join("");
}

export function captureShortcutEvent(e: {
  code: string;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
}): ShortcutCaptureResult {
  if (e.code === "Escape") {
    return { kind: "cancel" };
  }
  if (MODIFIER_CODES.has(e.code)) {
    return { kind: "ignore" };
  }
  const key = CODE_TO_CARBON[e.code];
  if (!key) {
    return { kind: "error", message: `Unsupported key (${e.code})` };
  }
  const modifiers =
    (e.metaKey ? 1 : 0) |
    (e.shiftKey ? 2 : 0) |
    (e.altKey ? 4 : 0) |
    (e.ctrlKey ? 8 : 0);
  if (modifiers === 0) {
    return { kind: "error", message: "Include ⌘, ⌥, or ⌃ (e.g. ⌘⇧S)" };
  }
  if (!key.isFunctionKey && (modifiers & (1 | 4 | 8)) === 0) {
    return { kind: "error", message: "Include ⌘, ⌥, or ⌃ with non-function keys" };
  }
  return {
    kind: "captured",
    binding: {
      key_code: key.keyCode,
      modifiers,
      label: formatShortcutLabel(modifiers, key.label),
    },
  };
}

export function effectiveMicShortcutLabel(
  prefs: Pick<ApplicationPreferences, "mic_mute_shortcut" | "mic_mute_shortcut_custom">,
): string {
  return prefs.mic_mute_shortcut_custom?.label ?? "⌘⇧M";
}
