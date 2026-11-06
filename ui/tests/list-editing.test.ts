// Tests for the list keys (`src/list-editing.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { blockOf, moveItem, renumber, shiftItems } from "../src/list-editing.ts";

const lines = (text: string) => text.split("\n");

test("an item block holds its nested items and continuation lines", () => {
  const text = lines("- a\n  - a1\n\n    more a1\n- b");
  assert.deepEqual(blockOf(text, 0), [0, 4]);
  assert.deepEqual(blockOf(text, 1), [1, 4]);
  assert.deepEqual(blockOf(text, 4), [4, 5]);
});

test("a block does not take a blank line before the next item", () => {
  assert.deepEqual(blockOf(lines("- a\n\n- b"), 0), [0, 1]);
});

test("Tab nests an item under the item above, with its children", () => {
  const next = shiftItems(lines("- a\n- b\n  - b1\n- c"), 1, 1, 1);
  assert.deepEqual(next, lines("- a\n  - b\n    - b1\n- c"));
});

test("Tab goes to the text column of the item above", () => {
  const next = shiftItems(lines("1. one\n2. two"), 1, 1, 1);
  assert.deepEqual(next, lines("1. one\n   1. two"));
});

test("Tab on the first item of a list changes nothing", () => {
  assert.equal(shiftItems(lines("- a\n- b"), 0, 0, 1), null);
  assert.equal(shiftItems(lines("- a\n  - b"), 1, 1, 1), null);
});

test("Tab on text that is no item changes nothing", () => {
  assert.equal(shiftItems(lines("text\n- b"), 0, 0, 1), null);
});

test("two selected siblings nest by the same step", () => {
  const next = shiftItems(lines("- a\n- b\n- c"), 1, 2, 1);
  assert.deepEqual(next, lines("- a\n  - b\n  - c"));
});

test("⇧Tab puts an item at the level of its parent", () => {
  const next = shiftItems(lines("- a\n  - b\n    - b1"), 1, 1, -1);
  assert.deepEqual(next, lines("- a\n- b\n  - b1"));
});

test("⇧Tab on a top-level item changes nothing", () => {
  assert.equal(shiftItems(lines("- a"), 0, 0, -1), null);
});

test("a tab indent counts as four columns and becomes spaces", () => {
  const next = shiftItems(lines("- a\n\t- b"), 1, 1, -1);
  assert.deepEqual(next, lines("- a\n- b"));
});

test("an item moves up over its sibling with all of its children", () => {
  const moved = moveItem(lines("- a\n  - a1\n- b\n  - b1"), 2, -1);
  assert.deepEqual(moved, { lines: lines("- b\n  - b1\n- a\n  - a1"), line: 0 });
});

test("an item moves down over the whole next sibling", () => {
  const moved = moveItem(lines("- a\n- b\n  - b1\n- c"), 0, 1);
  assert.deepEqual(moved, { lines: lines("- b\n  - b1\n- a\n- c"), line: 2 });
});

test("at the edge of its list an item passes one line", () => {
  const moved = moveItem(lines("- a\n  - child\n- b"), 1, -1);
  assert.deepEqual(moved, { lines: lines("  - child\n- a\n- b"), line: 0 });
  assert.equal(moveItem(lines("- a"), 0, -1), null);
  assert.equal(moveItem(lines("- a"), 0, 1), null);
});

test("a move keeps the numbers of an ordered list in order", () => {
  const moved = moveItem(lines("1. one\n2. two\n3. three"), 2, -1);
  assert.deepEqual(moved?.lines, lines("1. one\n2. three\n3. two"));
});

test("renumbering keeps the first number and each level counts alone", () => {
  const text = lines("3. c\n1. d\n   1. x\n   5. y\n7) e\n\ntext\n1. new list\n1. again");
  assert.deepEqual(
    renumber(text, 0, 0),
    lines("3. c\n4. d\n   1. x\n   2. y\n5) e\n\ntext\n1. new list\n1. again"),
  );
});
