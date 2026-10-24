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
  const open = note("Заметка");
  state.setNote(open);
  state.setVault(vault("личное"));
  state.setMode("read");
  assert.equal(state.note, open);
  assert.equal(state.vault?.id, "личное");
  assert.equal(state.mode, "read");
});

test("a setter calls the handlers of its event with the new value", () => {
  const state = createState();
  const seen: (string | null)[] = [];
  state.on("note", (value) => seen.push(value?.id ?? null));

  state.setNote(note("а"));
  state.setNote(null);
  // Other events do not reach the "note" handlers.
  state.setVault(vault("б"));
  assert.deepEqual(seen, ["а", null]);
});

test("the handler sees the new value in the store", () => {
  const state = createState();
  let inside: string | undefined;
  state.on("vault", () => (inside = state.vault?.id));
  state.setVault(vault("работа"));
  assert.equal(inside, "работа");
});

test("handlers run in the order they subscribed", () => {
  const state = createState();
  const order: string[] = [];
  state.on("mode", () => order.push("дерево"));
  state.on("mode", () => order.push("команды"));
  state.setMode("edit");
  assert.deepEqual(order, ["дерево", "команды"]);
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
    order.push("первый");
    off();
  });
  state.on("settings", () => order.push("второй"));

  state.setSettings(null);
  state.setSettings(null);
  assert.deepEqual(order, ["первый", "второй", "второй"]);
});

test("stores from createState do not share values", () => {
  const one = createState();
  const two = createState();
  one.setVaults([{ id: "а" } as never]);
  assert.equal(two.vaults.length, 0);
});
