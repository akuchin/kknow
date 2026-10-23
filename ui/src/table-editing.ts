/**
 * Table editing in the markup, as the Advanced Tables plugin for Obsidian does
 * it.
 *
 * Usually a table is drawn, and its cells open in place (`table-widget.ts`).
 * The markup is for the `</>` button, for a ⌘F match in a table and for the
 * "side by side" mode, where the editor shows plain Markdown. There the keys
 * work as in a spreadsheet:
 * - Tab goes to the next cell, ⇧Tab to the previous one;
 * - Enter goes to the next row and adds one at the end;
 * - ⌘Enter leaves the table;
 * - ⌥⇧ and an arrow move the row or the column.
 *
 * Each of these formats the whole table: pads the columns and fixes the
 * separator line. So the user types without thinking about pipes and spaces,
 * and the `.md` stays readable in any program.
 *
 * A table also starts here: type `| Name`, press Tab, and the separator
 * appears. The CodeMirror parser does not see a table without a separator, so
 * this module finds a table by its text, as the plugin does: lines that start
 * with a pipe.
 */

import { syntaxTree } from "@codemirror/language";
import { Prec, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

import {
  ROW,
  cellsOf,
  columnAt,
  insertColumn,
  insertRow,
  parseTable,
  runCommand,
  serializeTable,
  type Align,
  type Command,
  type Rect,
  type Table,
} from "./table";

/** Where the cursor is in the table. */
interface Focus {
  /** The row in `Table.rows`; `-1` is the separator line. */
  row: number;
  /** Equals `table.columns` when the cursor is after the closing pipe. */
  column: number;
  /** The cursor offset in the cell text. */
  offset: number;
}

/**
 * Blocks where a line with a pipe is text, not a table. Only blocks: inline
 * code in a cell is still in the table.
 */
const NOT_TABLE = new Set(["FencedCode", "CodeBlock", "HTMLBlock", "CommentBlock"]);

/** The table around `pos`, or `null` if `pos` is not in a table. */
export function tableAt(state: EditorState, pos: number): Table | null {
  const { doc } = state;
  const line = doc.lineAt(pos);

  for (let node = syntaxTree(state).resolveInner(pos, -1); node; node = node.parent!) {
    if (node.name === "Table") {
      // The parser node can start or end in the middle of a line; the table
      // is whole lines.
      const from = doc.lineAt(node.from).from;
      const to = doc.lineAt(node.to).to;
      return parseTable(doc.sliceString(from, to), from);
    }
    if (NOT_TABLE.has(node.name)) return null;
    if (!node.parent) break;
  }

  if (!ROW.test(line.text)) return null;
  let first = line.number;
  let last = line.number;
  while (first > 1 && ROW.test(doc.line(first - 1).text)) first -= 1;
  while (last < doc.lines && ROW.test(doc.line(last + 1).text)) last += 1;

  const from = doc.line(first).from;
  return parseTable(doc.sliceString(from, doc.line(last).to), from);
}

function focusOf(state: EditorState, table: Table, pos: number): Focus {
  const line = state.doc.lineAt(pos);
  const index = line.number - state.doc.lineAt(table.from).number;
  const column = columnAt(line.text, pos - line.from);

  if (table.delimiter && index === 1) return { row: -1, column, offset: 0 };

  const row = Math.max(0, table.lines.indexOf(index));
  const cell = table.rows[row]?.[column];
  const offset = cell ? Math.min(Math.max(0, pos - cell.from), cell.text.length) : 0;
  // A short row has fewer cells than the table: the cursor after its last
  // pipe is in a missing cell, not past the table.
  return { row, column: cell || column < table.columns ? column : table.columns, offset };
}

/** The table at the main cursor with the cursor place in it. */
function context(state: EditorState): { table: Table; focus: Focus } | null {
  const pos = state.selection.main.head;
  const table = tableAt(state, pos);
  return table ? { table, focus: focusOf(state, table, pos) } : null;
}

/**
 * The start column of a Tab run: Enter goes back to it, as a spreadsheet does
 * after you fill a row with Tab. It lives until the cursor leaves the place
 * where the last Tab or Enter put it.
 */
let run: { from: number; row: number; column: number; start: number } | null = null;

function runStart(table: Table, focus: Focus): number | null {
  if (!run || run.from !== table.from || run.row !== focus.row || run.column !== focus.column) {
    return null;
  }
  return run.start;
}

/** How to put the cursor into the target cell. */
type Place = "select" | "offset";

/**
 * Writes the formatted table and puts the cursor into a cell.
 *
 * Only the changed part of the text goes into the transaction: when only one
 * cell changed, the undo history and the other cursors see a small edit, not
 * a new table.
 *
 * A column past the table end is the place for a new column: the row gets a
 * space after the closing pipe, and the cursor stands there. What the user
 * types there becomes a cell on the next Tab.
 */
function write(
  view: EditorView,
  table: Table,
  rows: string[][],
  aligns: Align[],
  target: { row: number; column: number },
  place: Place,
  offset = 0,
): void {
  let text = serializeTable(rows, aligns, table.indent);
  const written = parseTable(text, table.from)!;
  const row = Math.min(target.row, written.rows.length - 1);
  let anchor: number;
  let head: number;

  if (target.column >= written.columns) {
    const lines = text.split("\n");
    const index = written.lines[row];
    lines[index] += " ";
    text = lines.join("\n");
    head = anchor = table.from + lines.slice(0, index + 1).join("\n").length;
  } else {
    const cell = written.rows[row][target.column];
    if (place === "select" && cell.text !== "") {
      anchor = cell.from;
      head = cell.to;
    } else {
      head = anchor = cell.from + Math.min(offset, cell.text.length);
    }
  }

  const old = view.state.doc.sliceString(table.from, table.to);
  view.dispatch({
    changes: smallestChange(old, text, table.from),
    selection: { anchor, head },
    scrollIntoView: true,
    userEvent: "input.table",
  });
}

/** The change from `old` to `next` without the common start and end. */
export function smallestChange(old: string, next: string, from: number) {
  let start = 0;
  while (start < old.length && start < next.length && old[start] === next[start]) start += 1;
  let end = 0;
  while (
    end < old.length - start &&
    end < next.length - start &&
    old[old.length - 1 - end] === next[next.length - 1 - end]
  ) {
    end += 1;
  }
  return { from: from + start, to: from + old.length - end, insert: next.slice(start, next.length - end) };
}

/** Tab: the next cell. Past the last column it prepares a new column. */
export function nextCell(view: EditorView): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;

  let rows = cellsOf(table);
  let aligns = table.aligns;
  let target: { row: number; column: number };

  if (focus.row === -1) {
    // From the separator line to the first body row.
    if (rows.length < 2) rows = insertRow(rows, 0);
    target = { row: 1, column: 0 };
  } else {
    let column = focus.column;
    // A second Tab past the table end: the user wants a new column even if
    // it is empty.
    if (column >= table.columns) {
      ({ rows, aligns } = insertColumn(rows, aligns, table.columns - 1));
      column = table.columns;
    }
    target = { row: focus.row, column: column + 1 };
  }

  const start = runStart(table, focus) ?? (focus.column < table.columns ? focus.column : 0);
  write(view, table, rows, aligns, target, "select");
  run = { from: table.from, ...target, start };
  return true;
}

/** ⇧Tab: the previous cell; from the row start, the end of the row above. */
export function previousCell(view: EditorView): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;
  const last = table.columns - 1;

  let target = { row: focus.row, column: Math.min(focus.column, last + 1) - 1 };
  if (focus.row === -1) target = { row: 0, column: last };
  else if (target.column < 0) target = focus.row === 0 ? { row: 0, column: 0 } : { row: focus.row - 1, column: last };

  write(view, table, cellsOf(table), table.aligns, target, "select");
  run = null;
  return true;
}

/**
 * Enter: the same column one row below; at the table end, a new row.
 *
 * Enter in an empty last row leaves the table and removes that row: this is
 * how a list ends in any editor. Advanced Tables has no such exit, only a
 * separate command, but without it Enter at the end of a table never stops.
 */
export function nextRow(view: EditorView): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;

  let rows = cellsOf(table);
  const last = rows.length - 1;
  if (focus.row === last && last >= 1 && rows[last].every((cell) => cell === "")) {
    write(view, table, rows.slice(0, -1), table.aligns, { row: 0, column: 0 }, "offset");
    return leaveTable(view);
  }

  const row = focus.row <= 0 ? 1 : focus.row + 1;
  if (row > last) rows = insertRow(rows, last);

  const start = runStart(table, focus) ?? (focus.column < table.columns ? focus.column : 0);
  const target = { row, column: start };
  write(view, table, rows, table.aligns, target, "select");
  run = { from: table.from, ...target, start };
  return true;
}

/**
 * ⌘Enter: formats the table and puts the cursor below it.
 *
 * Between the table and the cursor there is always an empty line. By the GFM
 * rules a text line right below a table is one more row of it, even without
 * pipes.
 */
export function escapeTable(view: EditorView): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;
  write(view, table, cellsOf(table), table.aligns, { row: Math.max(0, focus.row), column: 0 }, "offset");
  return leaveTable(view);
}

function leaveTable(view: EditorView): boolean {
  const { state } = view;
  const table = tableAt(state, state.selection.main.head);
  if (table) leaveBelow(view, table.to);
  run = null;
  return true;
}

/**
 * Puts the cursor below the table that ends at `end`. The drawn table uses it
 * too: ⌘Enter in a cell leaves the table the same way.
 */
export function leaveBelow(view: EditorView, end: number): void {
  const { doc } = view.state;
  const last = doc.lineAt(end);
  const next = last.number < doc.lines ? doc.line(last.number + 1) : null;
  const after = next && next.number < doc.lines ? doc.line(next.number + 1) : null;

  if (next && next.text.trim() === "" && after && after.text.trim() === "") {
    view.dispatch({ selection: { anchor: after.from }, scrollIntoView: true });
  } else if (next && next.text.trim() === "") {
    // Text right below: one more empty line keeps the cursor line apart from it.
    view.dispatch({
      changes: { from: next.to, insert: after ? "\n\n" : "\n" },
      selection: { anchor: next.to + 1 },
      scrollIntoView: true,
    });
  } else {
    view.dispatch({
      changes: { from: last.to, insert: "\n\n" },
      selection: { anchor: last.to + 2 },
      scrollIntoView: true,
    });
  }
}

/** The cursor cell as an action target. */
function cursorRect(table: Table, focus: Focus): Rect {
  const row = Math.max(0, focus.row);
  const column = Math.min(focus.column, table.columns - 1);
  return { top: row, left: column, bottom: row, right: column };
}

/** Whether an action is possible at the cursor: the toolbar dims the others. */
export function canRunAtCursor(state: EditorState, command: Command): boolean {
  const found = context(state);
  if (!found) return false;
  const { table, focus } = found;
  return runCommand(command, cellsOf(table), table.aligns, cursorRect(table, focus)) !== null;
}

/** Runs a table action on the cursor cell and keeps the cursor in the same cell. */
export function runAtCursor(view: EditorView, command: Command): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;

  const edited = runCommand(command, cellsOf(table), table.aligns, cursorRect(table, focus));
  if (!edited) return true;
  write(view, table, edited.rows, edited.aligns, { row: edited.rect.top, column: edited.rect.left }, "offset", focus.offset);
  run = null;
  return true;
}

function command(name: Command) {
  return (view: EditorView) => runAtCursor(view, name);
}

/**
 * The table keys. The highest precedence: the markdown keymap also binds
 * Enter (list continuation), and in a table ours must win. Outside a table
 * each command returns `false`, and the key does what it did before.
 */
export function tableEditing(): Extension {
  return Prec.highest(
    keymap.of([
      { key: "Tab", run: nextCell, shift: previousCell },
      { key: "Enter", run: nextRow },
      { key: "Mod-Enter", run: escapeTable },
      { key: "Alt-Shift-ArrowUp", run: command("rowUp") },
      { key: "Alt-Shift-ArrowDown", run: command("rowDown") },
      { key: "Alt-Shift-ArrowLeft", run: command("columnLeft") },
      { key: "Alt-Shift-ArrowRight", run: command("columnRight") },
    ]),
  );
}
