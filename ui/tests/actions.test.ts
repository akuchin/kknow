// Checks for the action keys (`src/actions.ts`): how a key reads on each
// system and which events press it. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  bind,
  canBind,
  conflictOf,
  formatKey,
  keyFromEvent,
  keysOf,
  matchesKey,
  rebindable,
  unbind,
  useKeymap,
} from "../src/actions.ts";

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

test("macOS: symbols in the Apple order", () => {
  assert.equal(formatKey("Mod-Shift-b", true), "⇧⌘B");
  assert.equal(formatKey("Mod-Alt-ArrowLeft", true), "⌥⌘←");
  assert.equal(formatKey("Mod-\\", true), "⌘\\");
  assert.equal(formatKey("Mod--", true), "⌘−");
  assert.equal(formatKey("Mod-,", true), "⌘,");
  assert.equal(formatKey("Enter", true), "↵");
  assert.equal(formatKey("Shift", true), "⇧");
});

test("Windows and Linux: Ctrl and plus signs", () => {
  assert.equal(formatKey("Mod-Shift-b", false), "Ctrl+Shift+B");
  assert.equal(formatKey("Mod-Alt-ArrowLeft", false), "Ctrl+Alt+←");
  assert.equal(formatKey("Mod-=", false), "Ctrl+=");
  assert.equal(formatKey("Mod-1", false), "Ctrl+1");
  assert.equal(formatKey("Escape", false), "Esc");
  assert.equal(formatKey("Enter", false), "Enter");
  assert.equal(formatKey("Shift", false), "Shift");
});

test("Mod is ⌘ on macOS and Ctrl on other systems", () => {
  assert.ok(matchesKey(press("KeyN", { meta: true }), "Mod-n", true));
  assert.ok(!matchesKey(press("KeyN", { ctrl: true }), "Mod-n", true));
  assert.ok(matchesKey(press("KeyN", { ctrl: true }), "Mod-n", false));
  assert.ok(!matchesKey(press("KeyN", { meta: true }), "Mod-n", false));
});

test("modifiers match exactly", () => {
  assert.ok(matchesKey(press("KeyS", { meta: true }), "Mod-s", true));
  assert.ok(!matchesKey(press("KeyS", { meta: true, shift: true }), "Mod-s", true));
  assert.ok(matchesKey(press("KeyS", { meta: true, shift: true }), "Mod-Shift-s", true));
});

test("a key by its place, not by the layout letter", () => {
  assert.ok(matchesKey(press("Backslash", { meta: true }), "Mod-\\", true));
  assert.ok(matchesKey(press("Minus", { meta: true }), "Mod--", true));
  assert.ok(matchesKey(press("NumpadEnter"), "Enter", true));
});

test("recording a shortcut: event → key", () => {
  assert.equal(keyFromEvent(press("KeyN", { meta: true, shift: true }), true), "Mod-Shift-n");
  assert.equal(keyFromEvent(press("KeyN", { ctrl: true }), true), "Ctrl-n");
  assert.equal(keyFromEvent(press("KeyN", { ctrl: true }), false), "Mod-n");
  assert.equal(keyFromEvent(press("Backslash", { meta: true }), true), "Mod-\\");
  assert.equal(keyFromEvent(press("Digit5", { alt: true }), true), "Alt-5");
  assert.equal(keyFromEvent(press("ArrowLeft", { meta: true, alt: true }), true), "Mod-Alt-ArrowLeft");
  assert.equal(keyFromEvent(press("MetaLeft", { meta: true }), true), null, "a modifier alone is not a key yet");
  assert.equal(keyFromEvent(press("ShiftRight", { shift: true }), true), null);
});

test("a key needs a modifier, except F1…F24", () => {
  assert.ok(canBind("Mod-j", true));
  assert.ok(canBind("Alt-Shift-ArrowUp", true));
  assert.ok(canBind("F5", true));
  assert.ok(!canBind("j", true));
  assert.ok(!canBind("Shift-j", true));
});

test("changed keys go over the defaults", () => {
  useKeymap({ "note.new": ["Mod-Shift-n"], "submit": ["Mod-Enter"], "note.save": ["Mod-j", "Super-j", 5] as never }, false);
  try {
    assert.deepEqual(keysOf("note.new"), ["Mod-Shift-n"]);
    assert.deepEqual(keysOf("submit"), ["Enter"], "Enter and Esc cannot be rebound");
    assert.deepEqual(keysOf("note.save"), ["Mod-j"], "invalid keys are dropped");
    assert.deepEqual(keysOf("note.find"), ["Mod-f"]);
    assert.ok(!rebindable().includes("edit.copy"));
  } finally {
    useKeymap({}, false);
  }
});

test("conflicts: another action, the system, a vault, the editor", () => {
  assert.deepEqual(conflictOf("Mod-n", "note.save", [], true), { kind: "action", action: "note.new" });
  assert.equal(conflictOf("Mod-n", "note.new", [], true), null, "its own key is not a conflict");
  assert.deepEqual(conflictOf("Mod-c", "note.new", [], true), { kind: "system" });
  assert.deepEqual(conflictOf("Mod-q", "note.new", [], true), { kind: "system" });
  assert.deepEqual(conflictOf("Mod-3", "note.new", [], true), { kind: "vault" });
  assert.deepEqual(conflictOf("Mod-d", "note.new", ["Mod-d"], true), { kind: "editor" });
  // The editor has ⌘I as "select node", but italic overrides it.
  assert.equal(conflictOf("Mod-i", "format.italic", ["Mod-i"], true), null);
  assert.equal(conflictOf("Mod-j", "note.new", ["Mod-d"], true), null);
});

test("a key moves from another action, and a default drops out of the list", () => {
  try {
    const moved = bind("note.save", "Mod-n", true);
    assert.deepEqual(moved, { "note.save": ["Mod-n"], "note.new": [] });

    useKeymap(moved, false);
    assert.deepEqual(keysOf("note.new"), []);
    assert.deepEqual(bind("note.new", "Mod-s", true), { "note.save": ["Mod-n"], "note.new": ["Mod-s"] });
    assert.deepEqual(unbind("note.save"), { "note.new": [] });

    useKeymap({ "note.new": ["Mod-j"] }, false);
    assert.deepEqual(bind("note.new", "Mod-n", true), {}, "the default key is back");
  } finally {
    useKeymap({}, false);
  }
});

test("an unknown action from the file is not lost", () => {
  try {
    useKeymap({ "future.action": ["Mod-j"] }, false);
    assert.deepEqual(bind("note.new", "Mod-Shift-n", true), {
      "future.action": ["Mod-j"],
      "note.new": ["Mod-Shift-n"],
    });
  } finally {
    useKeymap({}, false);
  }
});

test("bookmark keys are free: no other action uses them", () => {
  assert.deepEqual(keysOf("bookmark.next"), ["Mod-Alt-ArrowDown"]);
  assert.deepEqual(keysOf("bookmark.previous"), ["Mod-Alt-ArrowUp"]);
  assert.equal(conflictOf("Mod-Alt-ArrowDown", "bookmark.next", [], true), null);
  assert.equal(conflictOf("Mod-Alt-ArrowUp", "bookmark.previous", [], true), null);
});
