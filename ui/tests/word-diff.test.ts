// Checks the diff of an agent proposal (`src/word-diff.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { splitChange } from "../src/word-diff.ts";

test("only the changed words are marked", () => {
  assert.deepEqual(splitChange("took 14 weeks", "took 12 weeks"), ["took ", "14", "12", " weeks"]);
});

test("an addition after a whole word does not mark the word", () => {
  assert.deepEqual(splitChange("still hard", "still hard: very"), [
    "still hard",
    "",
    ": very",
    "",
  ]);
});

test("a change inside a word marks the whole word", () => {
  assert.deepEqual(splitChange("a separate topic", "a separate topics"), ["a separate ", "topic", "topics", ""]);
});

test("texts without a common part are marked whole", () => {
  assert.deepEqual(splitChange("yes", "no"), ["", "yes", "no", ""]);
});
