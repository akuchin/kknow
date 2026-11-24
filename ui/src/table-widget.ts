import { redo, selectAll, undo, defaultKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting } from "@codemirror/language";
import {
  EditorSelection,
  EditorState,
  type ChangeSet,
  Prec,
  StateEffect,
  StateField,
  Transaction,
  type Extension,
} from "@codemirror/state";
import { EditorView, WidgetType, drawSelection, keymap } from "@codemirror/view";

import { agentNotes, formatNote, previewAnchor } from "./agent-notes";
import { editorMenu } from "./editor-menu";
import { highlightStyle } from "./highlight";
import { icon, type IconName } from "./icons";
import { withShortcut, type Action } from "./actions";
import { t, type Key } from "./i18n";
import { draggedFrom } from "./pointer";
import {
  cellsOf,
  insertRow,
  parseTable,
  removeRows,
  runCommand,
  serializeTable,
  type Command,
  type Rect,
  type Table,
} from "./table";
import { leaveBelow, runAtCursor, smallestChange } from "./table-editing";

const redrawTables = StateEffect.define<void>();

export const tableStamp = StateField.define<number>({
  create: () => 0,
  update: (value, tr) => (tr.effects.some((effect) => effect.is(redrawTables)) ? value + 1 : value),
});

const showTableSource = StateEffect.define<{ from: number; to: number }>();

export const sourceTable = StateField.define<{ from: number; to: number } | null>({
  create: () => null,
  update(value, tr) {
    let next = value;

    if (next) {
      next = { from: tr.changes.mapPos(next.from, -1), to: tr.changes.mapPos(next.to, 1) };
      const cursor = tr.state.selection.main;
      if (cursor.from < next.from || cursor.to > next.to) next = null;
    }

    for (const effect of tr.effects) {
      if (effect.is(showTableSource)) next = effect.value;
    }
    return next;
  },
});

interface At {
  row: number;
  column: number;
}

type Place = "start" | "end" | "all" | { x: number; y: number };

let active: (At & { from: number; place: Place }) | null = null;

interface CellEditor {
  view: EditorView;
  main: EditorView;
  box: HTMLElement;
  at: At;
  closing: boolean;
}

let editing: CellEditor | null = null;

const editors = new WeakMap<HTMLElement, CellEditor>();

const models = new WeakMap<HTMLElement, Table>();

let moved: { from: number; rect: Rect } | null = null;

export class TableWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly model: Table,
    private readonly table: HTMLElement,
    private readonly stamp: number,
  ) {
    super();
  }

  eq(other: TableWidget): boolean {
    return (
      other.source === this.source &&
      other.model.from === this.model.from &&
      other.stamp === this.stamp
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "cm-table";
    box.dataset.from = String(this.model.from);
    models.set(box, this.model);

    const table = this.table.cloneNode(true) as HTMLElement;
    box.append(
      toolbar(
        (command) => runOnTable(view, box, command),
        () => false,
      ),
      table,
    );

    for (const cell of table.querySelectorAll<HTMLElement>("[data-row]")) {
      const at = { row: Number(cell.dataset.row), column: Number(cell.dataset.column) };

      cell.onmousedown = (event) => {
        if (event.button !== 0 || inCellEditor(event)) return;
        const model = models.get(box)!;
        if (event.shiftKey) {
          event.preventDefault();
          drag.extend(box, model, at);
        } else {
          drag.start(box, model, at);
        }
      };

      cell.onmouseenter = () => drag.to(box, models.get(box)!, at);

      cell.onmouseup = (event) => {
        if (event.button !== 0 || inCellEditor(event)) return;
        if (drag.moved || draggedFrom(event)) return;
        openAt(view, box, at, { x: event.clientX, y: event.clientY });
      };
    }

    for (const handle of table.querySelectorAll<HTMLElement>("[data-handle]")) {
      const kind = handle.dataset.handle as Handle;
      const at = Number(handle.dataset.index);

      handle.onmousedown = (event) => {
        event.preventDefault();
        if (event.button !== 0) return;
        const model = models.get(box)!;
        if (event.shiftKey) drag.extendBand(box, model, kind, at);
        else drag.startBand(box, model, kind, at);
      };

      handle.onmouseenter = () => drag.toBand(box, models.get(box)!, kind, at);
    }

    const model = this.model;
    if (moved?.from === model.from) {
      const { rect } = moved;
      moved = null;
      select(box, model, { row: rect.top, column: rect.left }, { row: rect.bottom, column: rect.right });
    }
    paint(box, model);

    if (active?.from === model.from) openEditor(view, box, active, active.place);
    return box;
  }

  updateDOM(dom: HTMLElement): boolean {
    const old = models.get(dom);
    if (!editing || editing.box !== dom || !old) return false;
    if (
      old.from !== this.model.from ||
      old.rows.length !== this.model.rows.length ||
      old.columns !== this.model.columns
    ) {
      return false;
    }
    models.set(dom, this.model);
    syncEditor(editing, this.model);
    return true;
  }

  destroy(dom: HTMLElement): void {
    const cell = editors.get(dom);
    if (!cell) return;
    cell.closing = true;
    cell.view.destroy();
    editors.delete(dom);
    if (editing === cell) editing = null;
  }

  ignoreEvent(): boolean {
    return true;
  }

  ignoreMutation(): boolean {
    return true;
  }
}

export function cellOf(node: globalThis.Node): HTMLElement | null {
  const element = node instanceof HTMLElement ? node : node.parentElement;
  if (!element || element.closest(".cm-cell-editor")) return null;
  const cell = element.closest<HTMLElement>(".cm-table [data-row]");
  return cell?.dataset.column === undefined ? null : cell;
}

export function addNoteInCell(
  main: EditorView,
  cell: Element,
  words: string,
  index: number,
  total: number,
  text: string,
): boolean {
  const box = cell.closest<HTMLElement>(".cm-table");
  const model = box && models.get(box);
  const { row, column } = (cell as HTMLElement).dataset;
  const target = model?.rows[Number(row)]?.[Number(column)];
  if (!target) return false;

  const to = previewAnchor(main.state, words, index, total, target);
  if (to === null) return false;
  main.dispatch({
    changes: { from: to, insert: formatNote(words, text).replace(/\\?\|/g, "\\|") },
    userEvent: "input",
  });
  return true;
}

function inCellEditor(event: Event): boolean {
  return (event.target as HTMLElement).closest(".cm-cell-editor") !== null;
}

function openAt(main: EditorView, box: HTMLElement, at: At, place: Place): void {
  const model = models.get(box)!;
  clearCells();
  if (editing?.box === box) {
    moveTo(editing, at, place);
    return;
  }
  if (editing) {
    const changes = closeCell(editing, null);
    active = { from: changes ? changes.mapPos(model.from) : model.from, ...at, place };
    main.dispatch({ effects: redrawTables.of() });
    return;
  }
  active = { from: model.from, ...at, place };
  openEditor(main, box, at, place);
}

function openEditor(main: EditorView, box: HTMLElement, at: At, place: Place): void {
  const model = models.get(box)!;
  const row = Math.min(Math.max(0, at.row), model.rows.length - 1);
  const column = Math.min(Math.max(0, at.column), model.columns - 1);
  const td = box.querySelector<HTMLElement>(`[data-row="${row}"][data-column="${column}"]`);
  if (!td) return;

  const cell: CellEditor = { view: null!, main, box, at: { row, column }, closing: false };
  cell.view = new EditorView({
    state: EditorState.create({
      doc: model.rows[row]?.[column]?.text ?? "",
      extensions: cellExtensions(cell),
    }),
    dispatchTransactions: (trs, view) => {
      view.update(trs);
      if (trs.some((tr) => tr.docChanged && !tr.annotation(Transaction.remote))) forward(cell);
    },
  });
  closeOnLeave(cell);

  editing = cell;
  editors.set(box, cell);
  active = { from: model.from, row, column, place };
  td.classList.add("is-editing");
  td.replaceChildren(cell.view.dom);
  refreshTools(box, model);

  queueMicrotask(() => {
    if (editing !== cell || !cell.view.dom.isConnected) return;
    cell.view.focus();
    const { doc } = cell.view.state;
    let selection = EditorSelection.single(doc.length);
    if (place === "start") selection = EditorSelection.single(0);
    if (place === "all") selection = EditorSelection.single(0, doc.length);
    if (typeof place === "object") {
      const pos = cell.view.posAtCoords(place);
      if (pos !== null) selection = EditorSelection.single(pos);
    }
    cell.view.dispatch({ selection, scrollIntoView: true });
  });
}

function forward(cell: CellEditor): void {
  const model = models.get(cell.box);
  if (!model) return;
  const text = cell.view.state.doc.toString().trim();
  const target = model.rows[cell.at.row]?.[cell.at.column];

  if (target) {
    if (target.text === text) return;
    cell.main.dispatch({
      changes: { from: target.from, to: target.to, insert: text },
      userEvent: "input",
    });
    return;
  }
  const rows = cellsOf(model);
  rows[cell.at.row][cell.at.column] = text;
  cell.main.dispatch({
    changes: { from: model.from, to: model.to, insert: serializeTable(rows, model.aligns, model.indent) },
    userEvent: "input",
  });
}

function syncEditor(cell: CellEditor, model: Table): void {
  const text = model.rows[cell.at.row]?.[cell.at.column]?.text ?? "";
  const doc = cell.view.state.doc.toString();
  if (doc.trim() === text) return;
  cell.view.dispatch({
    changes: { from: 0, to: doc.length, insert: text },
    annotations: Transaction.remote.of(true),
  });
}

function closeCell(
  cell: CellEditor,
  next: (At & { place: Place }) | null,
  rows?: string[][],
): ChangeSet | null {
  const model = models.get(cell.box);
  cell.closing = true;
  editing = null;
  active = next && model ? { from: model.from, ...next } : null;
  if (!model) return null;

  const grid = rows ?? cellsOf(model);
  const text = serializeTable(grid, model.aligns, model.indent);
  const old = cell.main.state.doc.sliceString(model.from, model.to);
  const changes = cell.main.state.changes(smallestChange(old, text, model.from));
  const selection = document.getSelection();
  if (selection && cell.view.dom.contains(selection.anchorNode)) selection.removeAllRanges();
  cell.main.dispatch({
    changes,
    effects: redrawTables.of(),
    annotations: rows ? [] : Transaction.addToHistory.of(false),
    userEvent: "input.table",
  });
  return changes;
}

function moveTo(cell: CellEditor, at: At, place: Place, rows?: string[][]): void {
  closeCell(cell, { ...at, place }, rows);
}

function leave(cell: CellEditor, where: "above" | "below" | "after", rows?: string[][]): void {
  const { main } = cell;
  const from = models.get(cell.box)?.from ?? 0;
  closeCell(cell, null, rows);

  const table = parseTableAt(main, from);
  if (where === "below" && table) {
    leaveBelow(main, table.to);
  } else if (where === "above") {
    main.dispatch({ selection: { anchor: Math.max(0, from - 1) }, scrollIntoView: true });
  } else if (table) {
    main.dispatch({ selection: { anchor: table.to } });
  }
  main.focus();
}

function parseTableAt(main: EditorView, from: number): Table | null {
  const { doc } = main.state;
  let last = doc.lineAt(from).number;
  while (last < doc.lines && doc.line(last + 1).text.trim() !== "") last += 1;
  return parseTable(doc.sliceString(from, doc.line(last).to), from);
}

function gridOf(cell: CellEditor): { model: Table; rows: string[][] } | null {
  const model = models.get(cell.box);
  if (!model) return null;
  const rows = cellsOf(model);
  rows[cell.at.row][cell.at.column] = cell.view.state.doc.toString().trim();
  return { model, rows };
}

function nextCell(cell: CellEditor): boolean {
  const found = gridOf(cell);
  if (!found) return true;
  const { model, rows } = found;
  const { row, column } = cell.at;

  if (column < model.columns - 1) {
    moveTo(cell, { row, column: column + 1 }, "all");
  } else if (row < rows.length - 1) {
    moveTo(cell, { row: row + 1, column: 0 }, "all");
  } else {
    moveTo(cell, { row: row + 1, column: 0 }, "all", insertRow(rows, row));
  }
  return true;
}

function previousCell(cell: CellEditor): boolean {
  const model = models.get(cell.box);
  if (!model) return true;
  const { row, column } = cell.at;
  if (column > 0) moveTo(cell, { row, column: column - 1 }, "all");
  else if (row > 0) moveTo(cell, { row: row - 1, column: model.columns - 1 }, "all");
  return true;
}

function nextRow(cell: CellEditor): boolean {
  const found = gridOf(cell);
  if (!found) return true;
  const { rows } = found;
  const { row, column } = cell.at;
  const last = rows.length - 1;

  if (row === last && row >= 1 && rows[row].every((text) => text === "")) {
    leave(cell, "below", removeRows(rows, row, row));
  } else if (row < last) {
    moveTo(cell, { row: row + 1, column }, "end");
  } else {
    moveTo(cell, { row: row + 1, column }, "end", insertRow(rows, row));
  }
  return true;
}

function arrow(cell: CellEditor, direction: "up" | "down" | "left" | "right"): boolean {
  const { view } = cell;
  const range = view.state.selection.main;
  if (!range.empty) return false;
  const model = models.get(cell.box);
  if (!model) return false;
  const { row, column } = cell.at;

  if (direction === "left" || direction === "right") {
    const forward = direction === "right";
    if (range.head !== (forward ? view.state.doc.length : 0)) return false;
    const next = column + (forward ? 1 : -1);
    if (next < 0 || next >= model.columns) return false;
    moveTo(cell, { row, column: next }, forward ? "start" : "end");
    return true;
  }

  const up = direction === "up";
  const edge = view.moveToLineBoundary(range, !up).head;
  if (edge !== (up ? 0 : view.state.doc.length)) return false;
  const next = row + (up ? -1 : 1);
  if (next < 0) leave(cell, "above");
  else if (next >= model.rows.length) leave(cell, "below");
  else moveTo(cell, { row: next, column }, up ? "end" : "start");
  return true;
}

const cellInput = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || tr.annotation(Transaction.remote)) return tr;
  let unsafe = false;
  tr.changes.iterChanges((_fromA, _toA, _fromB, _toB, inserted) => {
    if (/(^|[^\\])\||\n/.test(inserted.toString())) unsafe = true;
  });
  if (!unsafe) return tr;

  const specs: { from: number; to: number; insert: string }[] = [];
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    specs.push({
      from: fromA,
      to: toA,
      insert: inserted.toString().replace(/\s*\n\s*/g, " ").replace(/\\?\|/g, "\\|"),
    });
  });
  const changes = tr.startState.changes(specs);
  return {
    changes,
    selection: tr.startState.selection.map(changes, 1),
    userEvent: tr.annotation(Transaction.userEvent),
  };
});

function cellExtensions(cell: CellEditor): Extension {
  const run = (fn: (cell: CellEditor) => boolean) => () => fn(cell);
  return [
    Prec.highest(
      keymap.of([
        { key: "Tab", run: run(nextCell), shift: run(previousCell) },
        { key: "Enter", run: run(nextRow) },
        { key: "Shift-Enter", run: (view) => (view.dispatch(view.state.replaceSelection("<br>")), true) },
        { key: "Mod-Enter", run: () => (leave(cell, "below"), true) },
        { key: "Escape", run: () => (leave(cell, "after"), true) },
        { key: "ArrowUp", run: () => arrow(cell, "up") },
        { key: "ArrowDown", run: () => arrow(cell, "down") },
        { key: "ArrowLeft", run: () => arrow(cell, "left") },
        { key: "ArrowRight", run: () => arrow(cell, "right") },
        { key: "Mod-z", run: () => undo(cell.main) },
        { key: "Mod-Shift-z", run: () => redo(cell.main) },
        { key: "Mod-y", run: () => redo(cell.main) },
        { key: "Mod-a", run: selectAll },
      ]),
    ),
    keymap.of(defaultKeymap),
    cellInput,
    drawSelection(),
    EditorView.editorAttributes.of({ class: "cm-cell-editor" }),
    EditorView.lineWrapping,
    markdown({ base: markdownLanguage }),
    syntaxHighlighting(highlightStyle),
    agentNotes(),
    editorMenu(),
  ];
}

function closeOnLeave(cell: CellEditor): void {
  cell.view.dom.addEventListener("focusout", () => {
    queueMicrotask(() => {
      if (cell.closing || editing !== cell) return;
      if (document.hasFocus() && cell.view.dom.contains(document.activeElement)) return;
      if (document.activeElement?.closest(".menu")) return;
      closeCell(cell, null);
    });
  });
}

type Handle = "row" | "column" | "corner";

function rectOf(box: HTMLElement): Rect | null {
  const parts = box.dataset.selection?.split(",").map(Number);
  if (!parts || parts.length !== 4 || parts.some(Number.isNaN)) return null;
  return { top: parts[0], left: parts[1], bottom: parts[2], right: parts[3] };
}

let chosen: HTMLElement | null = null;

function select(box: HTMLElement, model: Table, from: At | null, to?: At): void {
  chosen = from ? box : null;

  if (!from) {
    delete box.dataset.selection;
  } else {
    const other = to ?? from;
    box.dataset.selection = [
      Math.min(from.row, other.row),
      Math.min(from.column, other.column),
      Math.max(from.row, other.row),
      Math.max(from.column, other.column),
    ].join(",");
  }
  paint(box, model);
}

function band(model: Table, kind: Handle, at: number): [At, At] | null {
  if (kind === "row") return [{ row: at, column: 0 }, { row: at, column: model.columns - 1 }];
  if (kind === "column") {
    return [{ row: 0, column: at }, { row: model.rows.length - 1, column: at }];
  }
  return null;
}

function paint(box: HTMLElement, model: Table): void {
  const rect = rectOf(box);

  for (const cell of box.querySelectorAll<HTMLElement>("[data-row]")) {
    const row = Number(cell.dataset.row);
    const column = Number(cell.dataset.column);
    const mine =
      rect !== null &&
      row >= rect.top &&
      row <= rect.bottom &&
      column >= rect.left &&
      column <= rect.right;

    cell.classList.toggle("is-selected", mine);
    cell.classList.toggle("edge-top", mine && row === rect!.top);
    cell.classList.toggle("edge-bottom", mine && row === rect!.bottom);
    cell.classList.toggle("edge-left", mine && column === rect!.left);
    cell.classList.toggle("edge-right", mine && column === rect!.right);
  }

  for (const handle of box.querySelectorAll<HTMLElement>("[data-handle]")) {
    const at = Number(handle.dataset.index);
    const kind = handle.dataset.handle as Handle;
    const touched =
      rect !== null &&
      (kind === "row"
        ? at >= rect.top && at <= rect.bottom
        : kind === "column" && at >= rect.left && at <= rect.right);

    handle.classList.toggle("is-touched", touched);
    handle.classList.toggle("is-selected", touched && whole(model, rect!, kind));
  }

  refreshTools(box, model);
}

function whole(model: Table, rect: Rect, kind: Handle): boolean {
  return kind === "row"
    ? rect.left === 0 && rect.right === model.columns - 1
    : rect.top === 0 && rect.bottom === model.rows.length - 1;
}

function inWidget(node: globalThis.Node | null): boolean {
  const dom = node instanceof HTMLElement ? node : node?.parentElement;
  return Boolean(dom?.closest(".cm-code-block, .cm-table, .cm-diagram"));
}

class Drag {
  private table = "";
  private anchor: At | null = null;
  private pressed = false;
  private bandKind: Handle | null = null;
  private bandAt = 0;
  moved = false;

  start(box: HTMLElement, model: Table, at: At): void {
    this.table = box.dataset.from ?? "";
    this.anchor = at;
    this.bandKind = null;
    this.pressed = true;
    this.moved = false;
    select(box, model, null);
    this.listen(model);
  }

  extend(box: HTMLElement, model: Table, at: At): void {
    const from = (this.mine(box) ? this.anchor : null) ?? corner(rectOf(box)) ?? at;
    this.table = box.dataset.from ?? "";
    this.anchor = from;
    this.bandKind = null;
    this.pressed = true;
    this.moved = true;
    select(box, model, from, at);
    this.listen(model);
  }

  to(box: HTMLElement, model: Table, at: At): void {
    if (!this.pressed || !this.anchor || this.bandKind || !this.mine(box)) return;
    if (at.row === this.anchor.row && at.column === this.anchor.column) return;

    this.moved = true;
    box.dataset.dragging = "true";
    this.clear();
    select(box, model, this.anchor, at);
  }

  startBand(box: HTMLElement, model: Table, kind: Handle, at: number): void {
    const edges = band(model, kind, at);
    this.table = box.dataset.from ?? "";
    this.anchor = edges?.[0] ?? null;
    this.bandKind = edges ? kind : null;
    this.bandAt = at;
    this.pressed = true;
    this.moved = true;
    select(box, model, edges?.[0] ?? null, edges?.[1]);
    this.listen(model);
  }

  extendBand(box: HTMLElement, model: Table, kind: Handle, at: number): void {
    if (this.bandKind !== kind || !this.mine(box)) {
      this.startBand(box, model, kind, at);
      return;
    }
    this.toBandRange(box, model, kind, at);
    this.pressed = true;
    this.listen(model);
  }

  toBand(box: HTMLElement, model: Table, kind: Handle, at: number): void {
    if (!this.pressed || this.bandKind !== kind || !this.mine(box)) return;
    this.toBandRange(box, model, kind, at);
  }

  private toBandRange(box: HTMLElement, model: Table, kind: Handle, at: number): void {
    const first = band(model, kind, Math.min(this.bandAt, at));
    const last = band(model, kind, Math.max(this.bandAt, at));
    if (!first || !last) return;
    select(box, model, first[0], last[1]);
  }

  private mine(box: HTMLElement): boolean {
    return box.dataset.from === this.table;
  }

  private readonly clear = (): void => {
    if (!this.moved) return;
    const selection = document.getSelection();
    if (selection && !selection.isCollapsed) selection.removeAllRanges();
  };

  private listen(model: Table): void {
    document.addEventListener("selectionchange", this.clear);
    document.addEventListener(
      "mouseup",
      () => {
        this.pressed = false;
        const box = document.querySelector<HTMLElement>(`.cm-table[data-from="${model.from}"]`);
        delete box?.dataset.dragging;
        this.clear();
        document.removeEventListener("selectionchange", this.clear);
        setTimeout(() => (this.moved = false));
      },
      { once: true },
    );
  }
}

const drag = new Drag();

function corner(rect: Rect | null): At | null {
  return rect ? { row: rect.top, column: rect.left } : null;
}

function onCopy(event: ClipboardEvent): void {
  if (document.activeElement?.closest(".cm-cell-editor")) return;
  const selection = document.getSelection();
  if (selection?.isCollapsed === false) {
    if (inWidget(selection.anchorNode)) event.stopPropagation();
    return;
  }

  const box = chosen;
  const rect = box?.isConnected ? rectOf(box) : null;
  if (!box || !rect) return;

  const lines: string[] = [];
  for (let row = rect.top; row <= rect.bottom; row += 1) {
    const cells: string[] = [];
    for (let column = rect.left; column <= rect.right; column += 1) {
      const cell = box.querySelector<HTMLElement>(`[data-row="${row}"][data-column="${column}"]`);
      cells.push(cell?.textContent?.trim() ?? "");
    }
    lines.push(cells.join("\t"));
  }

  event.clipboardData?.setData("text/plain", lines.join("\n"));
  event.preventDefault();
  event.stopPropagation();
}

type Tool = { command: Command | "source"; icon: IconName; title: Key; action?: Action } | "gap";

const TOOLS: Tool[] = [
  { command: "rowAdd", icon: "rowAdd", title: "editor.table.rowAdd" },
  { command: "rowRemove", icon: "rowRemove", title: "editor.table.rowRemove" },
  { command: "columnAdd", icon: "columnAdd", title: "editor.table.columnAdd" },
  { command: "columnRemove", icon: "columnRemove", title: "editor.table.columnRemove" },
  "gap",
  { command: "alignLeft", icon: "alignLeft", title: "editor.table.alignLeft" },
  { command: "alignCenter", icon: "alignCenter", title: "editor.table.alignCenter" },
  { command: "alignRight", icon: "alignRight", title: "editor.table.alignRight" },
  "gap",
  { command: "rowUp", icon: "rowUp", title: "editor.table.rowUp", action: "table.rowUp" },
  { command: "rowDown", icon: "rowDown", title: "editor.table.rowDown", action: "table.rowDown" },
  {
    command: "columnLeft",
    icon: "columnLeft",
    title: "editor.table.columnLeft",
    action: "table.columnLeft",
  },
  {
    command: "columnRight",
    icon: "columnRight",
    title: "editor.table.columnRight",
    action: "table.columnRight",
  },
  "gap",
  { command: "sortAsc", icon: "sortAsc", title: "editor.table.sortAsc" },
  { command: "sortDesc", icon: "sortDesc", title: "editor.table.sortDesc" },
  { command: "transpose", icon: "transpose", title: "editor.table.transpose" },
  "gap",
  { command: "source", icon: "code", title: "editor.table.source" },
];

export const COMMANDS = TOOLS.flatMap((tool) =>
  tool === "gap" || tool.command === "source" ? [] : [tool.command],
);

function toolbar(
  run: (command: Command | "source") => void,
  disabled: (command: Command | "source") => boolean,
  markup = false,
): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "cm-table-tools";

  for (const tool of markup ? TOOLS.slice(0, -2) : TOOLS) {
    if (tool === "gap") {
      const gap = document.createElement("span");
      gap.className = "cm-table-tools-gap";
      panel.append(gap);
      continue;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.ariaLabel = t(tool.title);
    button.title = tool.action ? withShortcut(t(tool.title), tool.action) : t(tool.title);
    button.append(icon(tool.icon));
    button.dataset.command = tool.command;
    button.disabled = disabled(tool.command);
    button.onmousedown = (event) => event.preventDefault();
    button.onclick = () => run(tool.command);
    panel.append(button);
  }

  return panel;
}

function targetOf(box: HTMLElement): Rect | null {
  const rect = rectOf(box);
  if (rect) return rect;
  if (editing?.box !== box) return null;
  const { row, column } = editing.at;
  return { top: row, left: column, bottom: row, right: column };
}

function refreshTools(box: HTMLElement, model: Table): void {
  const rect = targetOf(box);
  const cells = cellsOf(model);

  for (const button of box.querySelectorAll<HTMLButtonElement>(".cm-table-tools button")) {
    const command = button.dataset.command as Command | "source";
    if (command === "source") continue;
    button.disabled = rect
      ? runCommand(command, cells, model.aligns, rect) === null
      : command !== "rowAdd" && command !== "columnAdd";
  }
}

function runOnTable(main: EditorView, box: HTMLElement, command: Command | "source"): void {
  const model = models.get(box)!;

  if (command === "source") {
    if (editing?.box === box) closeCell(editing, null);
    main.dispatch({
      effects: showTableSource.of({ from: model.from, to: model.to }),
      selection: { anchor: model.from },
    });
    main.focus();
    return;
  }

  const last = { row: model.rows.length - 1, column: model.columns - 1 };
  const rect = targetOf(box) ?? { top: last.row, left: last.column, bottom: last.row, right: last.column };
  const open = editing?.box === box ? editing : null;
  const rows = open ? gridOf(open)!.rows : cellsOf(model);

  const edited = runCommand(command, rows, model.aligns, rect);
  if (!edited) return;
  const at = { row: edited.rect.top, column: edited.rect.left };
  const adds = command === "rowAdd" || command === "columnAdd";

  if (open) {
    open.closing = true;
    editing = null;
  }
  if (adds || open) {
    active = { from: model.from, ...at, place: "end" };
  }
  if (!adds && rectOf(box)) moved = { from: model.from, rect: edited.rect };
  clearCells();

  const text = serializeTable(edited.rows, edited.aligns, model.indent);
  const old = main.state.doc.sliceString(model.from, model.to);
  main.dispatch({
    changes: smallestChange(old, text, model.from),
    effects: redrawTables.of(),
    userEvent: "input.table",
  });
}

export class TableToolsWidget extends WidgetType {
  constructor(private readonly off: string) {
    super();
  }

  eq(other: TableToolsWidget): boolean {
    return other.off === this.off;
  }

  toDOM(view: EditorView): HTMLElement {
    const off = this.off.split(",");
    const panel = toolbar(
      (command) => {
        if (command !== "source") runAtCursor(view, command);
        view.focus();
      },
      (command) => off.includes(command),
      true,
    );
    panel.classList.add("is-source");
    return panel;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function clearCells(): void {
  if (!chosen?.isConnected) {
    chosen = null;
    return;
  }

  delete chosen.dataset.selection;
  for (const marked of chosen.querySelectorAll(".is-selected, .is-touched")) {
    marked.classList.remove("is-selected", "is-touched", "edge-top", "edge-bottom", "edge-left", "edge-right");
  }
  const model = models.get(chosen);
  if (model) refreshTools(chosen, model);
  chosen = null;
}

function onKey(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !chosen?.isConnected) return;
  clearCells();
}

const cellsFollowSelection = EditorView.updateListener.of((update) => {
  if (update.selectionSet && !update.state.selection.main.empty) clearCells();
});

const ignoreTableSelection = EditorState.transactionFilter.of((tr) => {
  if (tr.docChanged || tr.annotation(Transaction.userEvent) !== "select") return tr;
  return inTable(document.getSelection()?.anchorNode ?? null) ? [] : tr;
});

function inTable(node: globalThis.Node | null): boolean {
  const dom = node instanceof HTMLElement ? node : node?.parentElement;
  return Boolean(dom?.closest(".cm-table"));
}

export function tableWidgets(): Extension {
  document.addEventListener("copy", onCopy, true);
  document.addEventListener("cut", onCopy, true);
  document.addEventListener("keydown", onKey);
  return [tableStamp, sourceTable, cellsFollowSelection, ignoreTableSelection];
}
