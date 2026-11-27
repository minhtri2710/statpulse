import test from "node:test";
import assert from "node:assert/strict";
import {
  captureShortcutEvent,
  effectiveMicShortcutLabel,
  formatShortcutLabel,
} from "./shortcutKeymap.ts";

test("captures standard macOS shortcut combinations with Carbon keycodes", () => {
  const res = captureShortcutEvent({
    code: "KeyS",
    metaKey: true,
    shiftKey: true,
    altKey: false,
    ctrlKey: false,
  });
  assert.deepEqual(res, {
    kind: "captured",
    binding: { key_code: 0x01, modifiers: 3, label: "⌘⇧S" },
  });

  const allMods = captureShortcutEvent({
    code: "Space",
    metaKey: true,
    shiftKey: true,
    altKey: true,
    ctrlKey: true,
  });
  assert.deepEqual(allMods, {
    kind: "captured",
    binding: { key_code: 0x31, modifiers: 15, label: "⌘⌃⌥⇧Space" },
  });
  assert.equal(formatShortcutLabel(15, "Space"), "⌘⌃⌥⇧Space");
});

test("handles Escape cancel, lone modifiers, missing modifiers, and Shift-only rules", () => {
  assert.deepEqual(
    captureShortcutEvent({ code: "Escape", metaKey: false, shiftKey: false, altKey: false, ctrlKey: false }),
    { kind: "cancel" },
  );
  assert.deepEqual(
    captureShortcutEvent({ code: "MetaLeft", metaKey: true, shiftKey: false, altKey: false, ctrlKey: false }),
    { kind: "ignore" },
  );
  assert.equal(
    captureShortcutEvent({ code: "KeyA", metaKey: false, shiftKey: false, altKey: false, ctrlKey: false }).kind,
    "error",
  );
  assert.equal(
    captureShortcutEvent({ code: "F1", metaKey: false, shiftKey: false, altKey: false, ctrlKey: false }).kind,
    "error",
  );
  assert.equal(
    captureShortcutEvent({ code: "KeyA", metaKey: false, shiftKey: true, altKey: false, ctrlKey: false }).kind,
    "error",
  );
  assert.equal(
    captureShortcutEvent({ code: "Numpad0", metaKey: true, shiftKey: false, altKey: false, ctrlKey: false }).kind,
    "error",
  );

  const shiftF5 = captureShortcutEvent({
    code: "F5",
    metaKey: false,
    shiftKey: true,
    altKey: false,
    ctrlKey: false,
  });
  assert.deepEqual(shiftF5, {
    kind: "captured",
    binding: { key_code: 0x60, modifiers: 2, label: "⇧F5" },
  });
});

test("resolves effective mic shortcut label when enabled or disabled", () => {
  assert.equal(
    effectiveMicShortcutLabel({ mic_mute_shortcut: false, mic_mute_shortcut_custom: null }),
    null,
  );
  assert.equal(
    effectiveMicShortcutLabel({ mic_mute_shortcut: true, mic_mute_shortcut_custom: null }),
    "⌘⇧M",
  );
  assert.equal(
    effectiveMicShortcutLabel({
      mic_mute_shortcut: true,
      mic_mute_shortcut_custom: { key_code: 0x7a, modifiers: 2, label: "⇧F1" },
    }),
    "⇧F1",
  );
});
