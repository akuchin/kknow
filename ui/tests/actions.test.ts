// Checks for the action keys (`src/actions.ts`): how a key reads on each
// system and which events press it. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { formatKey, matchesKey } from "../src/actions.ts";

/** A key press as the browser gives it; only the fields that matter here. */
function press(code: string, mods: Partial<Record<"meta" | "ctrl" | "alt" | "shift", boolean>> = {}) {
  return {
    code,
    metaKey: Boolean(mods.meta),
    ctrlKey: Boolean(mods.ctrl),
    altKey: Boolean(mods.alt),
    shiftKey: Boolean(mods.shift),
  } as KeyboardEvent;
}

test("macOS: значки в порядке Apple", () => {
  assert.equal(formatKey("Mod-Shift-b", true), "⇧⌘B");
  assert.equal(formatKey("Mod-Alt-ArrowLeft", true), "⌥⌘←");
  assert.equal(formatKey("Mod-\\", true), "⌘\\");
  assert.equal(formatKey("Mod--", true), "⌘−");
  assert.equal(formatKey("Mod-,", true), "⌘,");
  assert.equal(formatKey("Enter", true), "↵");
  assert.equal(formatKey("Shift", true), "⇧");
});

test("Windows и Linux: Ctrl и плюсы", () => {
  assert.equal(formatKey("Mod-Shift-b", false), "Ctrl+Shift+B");
  assert.equal(formatKey("Mod-Alt-ArrowLeft", false), "Ctrl+Alt+←");
  assert.equal(formatKey("Mod-=", false), "Ctrl+=");
  assert.equal(formatKey("Mod-1", false), "Ctrl+1");
  assert.equal(formatKey("Escape", false), "Esc");
  assert.equal(formatKey("Enter", false), "Enter");
  assert.equal(formatKey("Shift", false), "Shift");
});

test("Mod — это ⌘ на macOS и Ctrl на других системах", () => {
  assert.ok(matchesKey(press("KeyN", { meta: true }), "Mod-n", true));
  assert.ok(!matchesKey(press("KeyN", { ctrl: true }), "Mod-n", true));
  assert.ok(matchesKey(press("KeyN", { ctrl: true }), "Mod-n", false));
  assert.ok(!matchesKey(press("KeyN", { meta: true }), "Mod-n", false));
});

test("модификаторы совпадают точно", () => {
  assert.ok(matchesKey(press("KeyS", { meta: true }), "Mod-s", true));
  assert.ok(!matchesKey(press("KeyS", { meta: true, shift: true }), "Mod-s", true));
  assert.ok(matchesKey(press("KeyS", { meta: true, shift: true }), "Mod-Shift-s", true));
});

test("клавиша по месту, а не по букве раскладки", () => {
  assert.ok(matchesKey(press("Backslash", { meta: true }), "Mod-\\", true));
  assert.ok(matchesKey(press("Minus", { meta: true }), "Mod--", true));
  assert.ok(matchesKey(press("NumpadEnter"), "Enter", true));
});
