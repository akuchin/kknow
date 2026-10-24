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

test("запись сочетания: событие → клавиша", () => {
  assert.equal(keyFromEvent(press("KeyN", { meta: true, shift: true }), true), "Mod-Shift-n");
  assert.equal(keyFromEvent(press("KeyN", { ctrl: true }), true), "Ctrl-n");
  assert.equal(keyFromEvent(press("KeyN", { ctrl: true }), false), "Mod-n");
  assert.equal(keyFromEvent(press("Backslash", { meta: true }), true), "Mod-\\");
  assert.equal(keyFromEvent(press("Digit5", { alt: true }), true), "Alt-5");
  assert.equal(keyFromEvent(press("ArrowLeft", { meta: true, alt: true }), true), "Mod-Alt-ArrowLeft");
  assert.equal(keyFromEvent(press("MetaLeft", { meta: true }), true), null, "один модификатор — ещё не клавиша");
  assert.equal(keyFromEvent(press("ShiftRight", { shift: true }), true), null);
});

test("клавише нужен модификатор, кроме F1…F24", () => {
  assert.ok(canBind("Mod-j", true));
  assert.ok(canBind("Alt-Shift-ArrowUp", true));
  assert.ok(canBind("F5", true));
  assert.ok(!canBind("j", true));
  assert.ok(!canBind("Shift-j", true));
});

test("изменённые клавиши ложатся поверх умолчаний", () => {
  useKeymap({ "note.new": ["Mod-Shift-n"], "submit": ["Mod-Enter"], "note.save": ["Mod-j", "Super-j", 5] as never }, false);
  try {
    assert.deepEqual(keysOf("note.new"), ["Mod-Shift-n"]);
    assert.deepEqual(keysOf("submit"), ["Enter"], "Enter и Esc не переназначить");
    assert.deepEqual(keysOf("note.save"), ["Mod-j"], "неверные клавиши отброшены");
    assert.deepEqual(keysOf("note.find"), ["Mod-f"]);
    assert.ok(!rebindable().includes("edit.copy"));
  } finally {
    useKeymap({}, false);
  }
});

test("конфликты: чужое действие, система, хранилище, редактор", () => {
  assert.deepEqual(conflictOf("Mod-n", "note.save", [], true), { kind: "action", action: "note.new" });
  assert.equal(conflictOf("Mod-n", "note.new", [], true), null, "своя клавиша — не конфликт");
  assert.deepEqual(conflictOf("Mod-c", "note.new", [], true), { kind: "system" });
  assert.deepEqual(conflictOf("Mod-q", "note.new", [], true), { kind: "system" });
  assert.deepEqual(conflictOf("Mod-3", "note.new", [], true), { kind: "vault" });
  assert.deepEqual(conflictOf("Mod-d", "note.new", ["Mod-d"], true), { kind: "editor" });
  // ⌘I у редактора — «выделить узел», но курсив его перекрывает.
  assert.equal(conflictOf("Mod-i", "format.italic", ["Mod-i"], true), null);
  assert.equal(conflictOf("Mod-j", "note.new", ["Mod-d"], true), null);
});

test("клавиша уходит от другого действия, умолчание выпадает из списка", () => {
  try {
    const moved = bind("note.save", "Mod-n", true);
    assert.deepEqual(moved, { "note.save": ["Mod-n"], "note.new": [] });

    useKeymap(moved, false);
    assert.deepEqual(keysOf("note.new"), []);
    assert.deepEqual(bind("note.new", "Mod-s", true), { "note.save": ["Mod-n"], "note.new": ["Mod-s"] });
    assert.deepEqual(unbind("note.save"), { "note.new": [] });

    useKeymap({ "note.new": ["Mod-j"] }, false);
    assert.deepEqual(bind("note.new", "Mod-n", true), {}, "вернулась клавиша по умолчанию");
  } finally {
    useKeymap({}, false);
  }
});

test("незнакомое действие из файла не теряется", () => {
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
