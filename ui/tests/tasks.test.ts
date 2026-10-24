// Checks for the task boxes (`src/tasks.ts`): which lines have a box and how
// a click changes it. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { setTaskBox, taskBox } from "../src/tasks.ts";

test("a box comes after any list marker", () => {
  assert.deepEqual(taskBox("- [ ] купить молоко"), { at: 3, done: false });
  assert.deepEqual(taskBox("  * [x] вложенная"), { at: 5, done: true });
  assert.deepEqual(taskBox("12. [X] по номеру"), { at: 5, done: true });
  assert.deepEqual(taskBox("> - [ ] в цитате"), { at: 5, done: false });
});

test("a line without a list marker has no box", () => {
  assert.equal(taskBox("[ ] без маркера"), null);
  assert.equal(taskBox("- обычный пункт"), null);
  assert.equal(taskBox("-[ ] без пробела"), null);
  assert.equal(taskBox("- [y] не галочка"), null);
});

test("only the character in the brackets changes", () => {
  assert.equal(setTaskBox("  - [ ] шаг [ ] два", true), "  - [x] шаг [ ] два");
  assert.equal(setTaskBox("- [X] готово", false), "- [ ] готово");
  assert.equal(setTaskBox("- [x] готово", true), "- [x] готово");
  assert.equal(setTaskBox("просто текст", true), null);
});
