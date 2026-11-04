// Checks the diff of an agent proposal (`src/word-diff.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { splitChange } from "../src/word-diff.ts";

test("only the changed words are marked", () => {
  assert.deepEqual(splitChange("занял 14 недель", "занял 12 недель"), ["занял ", "14", "12", " недель"]);
});

test("an addition after a whole word does not mark the word", () => {
  assert.deepEqual(splitChange("даются тяжело", "даются тяжело: очень"), [
    "даются тяжело",
    "",
    ": очень",
    "",
  ]);
});

test("a change inside a word marks the whole word", () => {
  assert.deepEqual(splitChange("отдельная тема", "отдельная темка"), ["отдельная ", "тема", "темка", ""]);
});

test("texts without a common part are marked whole", () => {
  assert.deepEqual(splitChange("да", "нет"), ["", "да", "нет", ""]);
});
