// The store of the main window (`src/window/state.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import type { NoteView, VaultInfo } from "../src/api.ts";
import { createState } from "../src/window/state.ts";

const note = (id: string): NoteView => ({ id }) as NoteView;
const vault = (id: string): VaultInfo => ({ id }) as VaultInfo;

test("a new store has the empty values", () => {
  const state = createState();
  assert.equal(state.note, null);
  assert.equal(state.vault, null);
  assert.deepEqual(state.vaults, []);
  assert.deepEqual(state.files, []);
  assert.equal(state.mode, "split");
  assert.equal(state.settings, null);
});

test("a setter keeps the value and the getter returns it", () => {
  const state = createState();
  const open = note("Note");
  state.setNote(open);
  state.setVault(vault("personal"));
  state.setMode("read");
  assert.equal(state.note, open);
  assert.equal(state.vault?.id, "personal");
  assert.equal(state.mode, "read");
});

test("a setter calls the handlers of its event with the new value", () => {
  const state = createState();
  const seen: (string | null)[] = [];
  state.on("note", (value) => seen.push(value?.id ?? null));

  state.setNote(note("a"));
  state.setNote(null);
  // Other events do not reach the "note" handlers.
  state.setVault(vault("b"));
  assert.deepEqual(seen, ["a", null]);
});

test("the handler sees the new value in the store", () => {
  const state = createState();
  let inside: string | undefined;
  state.on("vault", () => (inside = state.vault?.id));
  state.setVault(vault("work"));
  assert.equal(inside, "work");
});

test("handlers run in the order they subscribed", () => {
  const state = createState();
  const order: string[] = [];
  state.on("mode", () => order.push("tree"));
  state.on("mode", () => order.push("commands"));
  state.setMode("edit");
  assert.deepEqual(order, ["tree", "commands"]);
});

test("a setter emits also when the value is the same", () => {
  const state = createState();
  let calls = 0;
  state.on("mode", () => calls++);
  state.setMode("split");
  state.setMode("split");
  assert.equal(calls, 2);
});

test("the returned function unsubscribes", () => {
  const state = createState();
  let calls = 0;
  const off = state.on("files", () => calls++);
  state.setFiles([]);
  off();
  state.setFiles([]);
  assert.equal(calls, 1);
});

test("a handler can unsubscribe during the event", () => {
  const state = createState();
  const order: string[] = [];
  const off = state.on("settings", () => {
    order.push("first");
    off();
  });
  state.on("settings", () => order.push("second"));

  state.setSettings(null);
  state.setSettings(null);
  assert.deepEqual(order, ["first", "second", "second"]);
});

test("stores from createState do not share values", () => {
  const one = createState();
  const two = createState();
  one.setVaults([{ id: "a" } as never]);
  assert.equal(two.vaults.length, 0);
});
