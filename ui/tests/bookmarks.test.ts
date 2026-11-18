// Checks for the next and previous bookmark (`src/bookmarks.ts`).
// Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { adjacentBookmark } from "../src/bookmarks.ts";

test("next and previous follow the order of the lines", () => {
  const lines = [3, 10, 20];
  const next = [1, 3, 5, 10, 15].map((cursor) => adjacentBookmark(lines, cursor, 1));
  assert.deepEqual(next, [3, 10, 10, 20, 20]);
  const previous = [5, 10, 25].map((cursor) => adjacentBookmark(lines, cursor, -1));
  assert.deepEqual(previous, [3, 3, 20]);
});

test("after the last bookmark the next one is the first, and the other way round", () => {
  const lines = [3, 10, 20];
  assert.equal(adjacentBookmark(lines, 20, 1), 3);
  assert.equal(adjacentBookmark(lines, 25, 1), 3);
  assert.equal(adjacentBookmark(lines, 3, -1), 20);
  assert.equal(adjacentBookmark(lines, 1, -1), 20);
});

test("one bookmark: both directions go to it", () => {
  for (const cursor of [1, 7, 9]) {
    assert.equal(adjacentBookmark([7], cursor, 1), 7);
    assert.equal(adjacentBookmark([7], cursor, -1), 7);
  }
});

test("no bookmarks: nothing to go to", () => {
  assert.equal(adjacentBookmark([], 1, 1), null);
  assert.equal(adjacentBookmark([], 1, -1), null);
});
