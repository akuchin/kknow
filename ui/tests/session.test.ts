// The autosave queue (`src/session.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { SaveQueue } from "../src/session.ts";

const tick = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("only the last edit is written after the pause", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 20);

  saves.schedule("a");
  saves.schedule("ab");
  assert.ok(saves.pending);
  await tick(40);

  assert.deepEqual(written, ["ab"]);
  assert.ok(!saves.pending);
});

test("writes run one at a time, in order", async () => {
  const log: string[] = [];
  const saves = new SaveQueue(async (text) => {
    log.push(`start ${text}`);
    await tick(10);
    log.push(`end ${text}`);
  }, 1000);

  void saves.save("1");
  await saves.save("2");

  assert.deepEqual(log, ["start 1", "end 1", "start 2", "end 2"]);
});

test("flush writes a pending edit at once", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 1000);

  saves.schedule("old");
  await saves.flush(() => "from the editor");

  assert.deepEqual(written, ["from the editor"]);
  assert.ok(!saves.pending);
});

test("flush without an edit waits for the write in progress", async () => {
  let done = false;
  const saves = new SaveQueue(async () => {
    await tick(10);
    done = true;
  }, 1000);

  void saves.save("text");
  await saves.flush(() => "not needed");

  assert.ok(done);
});

test("cancel drops a pending edit of a deleted note", async () => {
  const written: string[] = [];
  const saves = new SaveQueue(async (text) => void written.push(text), 10);

  saves.schedule("deleted");
  saves.cancel();
  await tick(30);

  assert.deepEqual(written, []);
});
