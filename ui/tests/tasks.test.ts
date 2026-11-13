// Checks for the task boxes (`src/tasks.ts`): which lines have a box and how
// a click changes it. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { setTaskBox, taskBox } from "../src/tasks.ts";

test("a box comes after any list marker", () => {
  assert.deepEqual(taskBox("- [ ] buy milk"), { at: 3, done: false });
  assert.deepEqual(taskBox("  * [x] nested"), { at: 5, done: true });
  assert.deepEqual(taskBox("12. [X] numbered"), { at: 5, done: true });
  assert.deepEqual(taskBox("> - [ ] in a quote"), { at: 5, done: false });
});

test("a line without a list marker has no box", () => {
  assert.equal(taskBox("[ ] no marker"), null);
  assert.equal(taskBox("- a plain item"), null);
  assert.equal(taskBox("-[ ] no space"), null);
  assert.equal(taskBox("- [y] not a check"), null);
});

test("only the character in the brackets changes", () => {
  assert.equal(setTaskBox("  - [ ] step [ ] two", true), "  - [x] step [ ] two");
  assert.equal(setTaskBox("- [X] done", false), "- [ ] done");
  assert.equal(setTaskBox("- [x] done", true), "- [x] done");
  assert.equal(setTaskBox("just text", true), null);
});
