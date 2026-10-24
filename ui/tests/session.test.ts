// The autosave queue (`src/session.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { SaveQueue } from "../src/session.ts";

const tick = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("only the last edit is written after the pause", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 20);

  saves.schedule("а");
  saves.schedule("аб");
  assert.ok(saves.pending);
  await tick(40);

  assert.deepEqual(written, ["аб"]);
  assert.ok(!saves.pending);
});

test("writes run one at a time, in order", async () => {
  const log: string[] = [];
  const saves = new SaveQueue(async (text) => {
    log.push(`начало ${text}`);
    await tick(10);
    log.push(`конец ${text}`);
  }, 1000);

  void saves.save("1");
  await saves.save("2");

  assert.deepEqual(log, ["начало 1", "конец 1", "начало 2", "конец 2"]);
});

test("flush writes a pending edit at once", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 1000);

  saves.schedule("старое");
  await saves.flush(() => "из редактора");

  assert.deepEqual(written, ["из редактора"]);
  assert.ok(!saves.pending);
});

test("flush without an edit waits for the write in progress", async () => {
  let done = false;
  const saves = new SaveQueue(async () => {
    await tick(10);
    done = true;
  }, 1000);

  void saves.save("текст");
  await saves.flush(() => "не нужен");

  assert.ok(done);
});

test("cancel drops a pending edit of a deleted note", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 10);

  saves.schedule("удалённая");
  saves.cancel();
  await tick(30);

  assert.deepEqual(written, []);
});
