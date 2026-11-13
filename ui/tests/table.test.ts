// Tests for the table model (`src/table.ts`). Node runs them as is: it strips
// the types itself, and the module has no imports. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  columnAt,
  isDelimiter,
  moveColumns,
  moveRows,
  parseTable,
  runCommand,
  serializeTable,
  setAlign,
  sortRows,
  textWidth,
  transpose,
} from "../src/table.ts";

test("formats a table and aligns the columns", () => {
  const text = serializeTable(
    [
      ["Who", "Lessons"],
      ["Ann", "7"],
    ],
    [null, "right"],
  );
  assert.equal(text, ["| Who | Lessons |", "| --- | ------: |", "| Ann |       7 |"].join("\n"));
});

test("centers text and keeps the indent of a table in a list", () => {
  const text = serializeTable([["a", "b"], ["ccccc", "d"]], ["center"], "  ");
  assert.equal(text, ["  |   a   | b   |", "  | :---: | --- |", "  | ccccc | d   |"].join("\n"));
});

test("wide characters take two cells", () => {
  assert.equal(textWidth("表"), 2);
  assert.equal(textWidth("\u00e9"), 1);
  assert.equal(textWidth("e\u0301"), 1);
  const text = serializeTable([["表格", "x"], ["abcd", "y"]], []);
  assert.equal(text.split("\n")[0], "| 表格 | x   |");
});

test("a table of only a header has no delimiter", () => {
  const table = parseTable("| Who | Lessons", 10)!;
  assert.equal(table.delimiter, false);
  assert.deepEqual(
    table.rows[0].map((cell) => cell.text),
    ["Who", "Lessons"],
  );
  assert.deepEqual(table.lines, [0]);
});

test("parsing: cell positions, alignment, an escaped pipe", () => {
  const source = "| a | b \\| c |\n|:--|--:|\n| 1 |   |";
  const table = parseTable(source, 100)!;
  assert.equal(table.delimiter, true);
  assert.deepEqual(table.aligns, ["left", "right"]);
  assert.equal(table.rows[0][1].text, "b \\| c");
  assert.equal(source.slice(table.rows[0][0].from - 100, table.rows[0][0].to - 100), "a");
  assert.deepEqual(table.lines, [0, 2]);
  // An empty cell: the range is inside the spaces.
  assert.equal(table.rows[1][1].from, table.rows[1][1].to);
});

test("a delimiter differs from a text row", () => {
  assert.equal(isDelimiter("| --- | :-: |"), true);
  assert.equal(isDelimiter("--- | ---"), true);
  assert.equal(isDelimiter("| a | b |"), false);
  assert.equal(isDelimiter("|  |"), false);
});

test("the column under the cursor is counted by pipes", () => {
  const line = "| a | b |";
  assert.equal(columnAt(line, 0), 0);
  assert.equal(columnAt(line, 2), 0);
  assert.equal(columnAt(line, 5), 1);
  // After the closing pipe: past the last cell.
  assert.equal(columnAt(line, 9), 2);
  assert.equal(columnAt("a | b", 4), 1);
  assert.equal(columnAt("| a \\| b | c", 10), 1);
});

test("rows move, the header stays in place", () => {
  const rows = [["h"], ["1"], ["2"], ["3"]];
  assert.deepEqual(moveRows(rows, 2, 3, -1), [["h"], ["2"], ["3"], ["1"]]);
  assert.equal(moveRows(rows, 1, 1, -1), null);
  assert.equal(moveRows(rows, 3, 3, 1), null);
  assert.equal(moveRows(rows, 0, 0, 1), null);
});

test("columns move together with their alignment", () => {
  const moved = moveColumns([["a", "b", "c"]], ["left"], 0, 0, 1)!;
  assert.deepEqual(moved.rows, [["b", "a", "c"]]);
  assert.deepEqual(moved.aligns, [null, "left", null]);
  assert.equal(moveColumns([["a", "b"]], [], 1, 1, 1), null);
});

test("the same alignment again removes it", () => {
  assert.deepEqual(setAlign([], 2, 1, 1, "center"), [null, "center"]);
  assert.deepEqual(setAlign([null, "center"], 2, 1, 1, "center"), [null, null]);
});

test("sorting: numbers as numbers, empty cells last, the header first", () => {
  const rows = [["Name", "Sum"], ["Bob", "10"], ["", "2"], ["Ann", ""], ["Vera", "2,5"]];
  assert.deepEqual(
    sortRows(rows, 1, false).map((row) => row[1]),
    ["Sum", "2", "2,5", "10", ""],
  );
  assert.deepEqual(
    sortRows(rows, 1, true).map((row) => row[1]),
    ["Sum", "10", "2,5", "2", ""],
  );
  assert.deepEqual(
    sortRows(rows, 0, false).map((row) => row[0]),
    ["Name", "Ann", "Bob", "Vera", ""],
  );
});

test("transposing", () => {
  assert.deepEqual(transpose([["a", "b"], ["1", "2"], ["3"]]), [
    ["a", "1", "3"],
    ["b", "2", ""],
  ]);
});

test("toolbar actions return the new place of the selection", () => {
  const rows = [["h1", "h2"], ["a", "b"], ["c", "d"]];
  const cell = (row: number, column: number) => ({ top: row, left: column, bottom: row, right: column });

  const down = runCommand("rowDown", rows, [], cell(1, 0))!;
  assert.deepEqual(down.rows[2], ["a", "b"]);
  assert.deepEqual(down.rect, cell(2, 0));

  const added = runCommand("rowAdd", rows, [], cell(0, 1))!;
  assert.deepEqual(added.rows[1], ["", ""]);
  assert.deepEqual(added.rect, cell(1, 1));

  const removed = runCommand("columnRemove", rows, [], cell(1, 1))!;
  assert.deepEqual(removed.rows[0], ["h1"]);
  assert.deepEqual(removed.rect, cell(1, 0));

  // The header and the last column stay.
  assert.equal(runCommand("rowRemove", rows, [], cell(0, 0)), null);
  assert.equal(runCommand("columnRemove", rows, [], { top: 0, left: 0, bottom: 0, right: 1 }), null);
  assert.equal(runCommand("rowUp", rows, [], cell(1, 0)), null);
});
