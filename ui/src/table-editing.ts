import { syntaxTree } from "@codemirror/language";
import { Prec, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

import { bindings } from "./actions";
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

interface Focus {
  row: number;
  column: number;
  offset: number;
}

const NOT_TABLE = new Set(["FencedCode", "CodeBlock", "HTMLBlock", "CommentBlock"]);

export function tableAt(state: EditorState, pos: number): Table | null {
  const { doc } = state;
  const line = doc.lineAt(pos);

  for (let node = syntaxTree(state).resolveInner(pos, -1); node; node = node.parent!) {
    if (node.name === "Table") {
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
  return { row, column: cell || column < table.columns ? column : table.columns, offset };
}

function context(state: EditorState): { table: Table; focus: Focus } | null {
  const pos = state.selection.main.head;
  const table = tableAt(state, pos);
  return table ? { table, focus: focusOf(state, table, pos) } : null;
}

let run: { from: number; row: number; column: number; start: number } | null = null;

function runStart(table: Table, focus: Focus): number | null {
  if (!run || run.from !== table.from || run.row !== focus.row || run.column !== focus.column) {
    return null;
  }
  return run.start;
}

type Place = "select" | "offset";

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

export function nextCell(view: EditorView): boolean {
  const found = context(view.state);
  if (!found) return false;
  const { table, focus } = found;

  let rows = cellsOf(table);
  let aligns = table.aligns;
  let target: { row: number; column: number };

  if (focus.row === -1) {
    if (rows.length < 2) rows = insertRow(rows, 0);
    target = { row: 1, column: 0 };
  } else {
    let column = focus.column;
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

export function leaveBelow(view: EditorView, end: number): void {
  const { doc } = view.state;
  const last = doc.lineAt(end);
  const next = last.number < doc.lines ? doc.line(last.number + 1) : null;
  const after = next && next.number < doc.lines ? doc.line(next.number + 1) : null;

  if (next && next.text.trim() === "" && after && after.text.trim() === "") {
    view.dispatch({ selection: { anchor: after.from }, scrollIntoView: true });
  } else if (next && next.text.trim() === "") {
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

function cursorRect(table: Table, focus: Focus): Rect {
  const row = Math.max(0, focus.row);
  const column = Math.min(focus.column, table.columns - 1);
  return { top: row, left: column, bottom: row, right: column };
}

export function canRunAtCursor(state: EditorState, command: Command): boolean {
  const found = context(state);
  if (!found) return false;
  const { table, focus } = found;
  return runCommand(command, cellsOf(table), table.aligns, cursorRect(table, focus)) !== null;
}

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

export function tableEditing(): Extension {
  return Prec.highest(
    keymap.of([
      { key: "Tab", run: nextCell, shift: previousCell },
      { key: "Enter", run: nextRow },
      { key: "Mod-Enter", run: escapeTable },
      ...bindings("table.rowUp", command("rowUp")),
      ...bindings("table.rowDown", command("rowDown")),
      ...bindings("table.columnLeft", command("columnLeft")),
      ...bindings("table.columnRight", command("columnRight")),
    ]),
  );
}
