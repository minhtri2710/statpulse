import test from "node:test";
import assert from "node:assert/strict";
import { captureShortcutEvent, effectiveMicShortcutLabel } from "./shortcutKeymap.ts";

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

  const optSpace = captureShortcutEvent({
    code: "Space",
    metaKey: false,
    shiftKey: false,
    altKey: true,
    ctrlKey: false,
  });
  assert.deepEqual(optSpace, {
    kind: "captured",
    binding: { key_code: 0x31, modifiers: 4, label: "⌥Space" },
  });
});

test("handles Escape cancel, lone modifiers, and Shift-only rejections", () => {
  assert.deepEqual(
    captureShortcutEvent({ code: "Escape", metaKey: false, shiftKey: false, altKey: false, ctrlKey: false }),
    { kind: "cancel" },
  );
  assert.deepEqual(
    captureShortcutEvent({ code: "MetaLeft", metaKey: true, shiftKey: false, altKey: false, ctrlKey: false }),
    { kind: "ignore" },
  );
  const shiftOnlyLetter = captureShortcutEvent({
    code: "KeyA",
    metaKey: false,
    shiftKey: true,
    altKey: false,
    ctrlKey: false,
  });
  assert.equal(shiftOnlyLetter.kind, "error");

  const shiftF1 = captureShortcutEvent({
    code: "F1",
    metaKey: false,
    shiftKey: true,
    altKey: false,
    ctrlKey: false,
  });
  assert.deepEqual(shiftF1, {
    kind: "captured",
    binding: { key_code: 0x7a, modifiers: 2, label: "⇧F1" },
  });
});

test("resolves effective mic shortcut label with fallback", () => {
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
