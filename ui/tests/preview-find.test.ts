import assert from "node:assert/strict";
import { test } from "node:test";

import { EMPTY_QUERY, findRanges, type FindQuery } from "../src/find-ranges.ts";

function query(text: string, extra: Partial<FindQuery> = {}): FindQuery {
  return { ...EMPTY_QUERY, text, ...extra };
}

test("a plain query finds every match and ignores case", () => {
  assert.deepEqual(findRanges(["Rust and rust"], query("rust")), [
    { block: 0, from: 0, to: 4 },
    { block: 0, from: 9, to: 13 },
  ]);
});

test("match case keeps only the exact spelling", () => {
  assert.deepEqual(findRanges(["Rust and rust"], query("rust", { caseSensitive: true })), [
    { block: 0, from: 9, to: 13 },
  ]);
});

test("by word skips matches inside a longer word", () => {
  const found = findRanges(["cat concat cat_x (cat)"], query("cat", { wholeWord: true }));
  assert.deepEqual(found, [
    { block: 0, from: 0, to: 3 },
    { block: 0, from: 18, to: 21 },
  ]);
});

test("a regexp query uses the pattern", () => {
  assert.deepEqual(findRanges(["a1 b22"], query("\\d+", { regexp: true })), [
    { block: 0, from: 1, to: 2 },
    { block: 0, from: 4, to: 6 },
  ]);
});

test("an invalid regexp gives null", () => {
  assert.equal(findRanges(["text"], query("(", { regexp: true })), null);
});

test("an empty query and empty regexp matches give no ranges", () => {
  assert.deepEqual(findRanges(["text"], query("")), []);
  assert.deepEqual(findRanges(["text"], query("x*", { regexp: true })), [{ block: 0, from: 2, to: 3 }]);
  assert.deepEqual(findRanges(["abc"], query("z*", { regexp: true })), []);
});

test("a match does not cross two blocks", () => {
  assert.deepEqual(findRanges(["first", "second"], query("tsec")), []);
});

test("blocks are numbered", () => {
  assert.deepEqual(findRanges(["no", "a to", "to"], query("to")), [
    { block: 1, from: 2, to: 4 },
    { block: 2, from: 0, to: 2 },
  ]);
});

test("Cyrillic text matches without case and by word", () => {
  assert.deepEqual(findRanges(["Заметка и заметки"], query("заметка", { wholeWord: true })), [
    { block: 0, from: 0, to: 7 },
  ]);
  assert.equal(findRanges(["Заметка и заметки"], query("заметк"))?.length, 2);
});
