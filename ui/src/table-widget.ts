/**
 * The drawn table in the editor, as in Obsidian Live Preview.
 *
 * The table stays a table while you edit it. A click opens the cell in its own
 * small CodeMirror editor, with the cell markup in it. Each key goes to the
 * note at once: the cell editor writes its text into the cell range. Tab,
 * Enter and the arrows go from cell to cell, and the markup of the whole table
 * is formatted on each move (as Advanced Tables does). ⌘Z in a cell undoes in
 * the note: there is one history, not one for each cell.
 *
 * A mouse drag does not open a cell: it selects text in a cell or a rectangle
 * of cells, because people read and copy tables more often than they edit
 * them. ⌘C copies the rectangle as tab-separated text.
 */

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

import { highlightStyle } from "./highlight";
import { icon, type IconName } from "./icons";
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

/** Asks every drawn table to build its DOM again: a closed cell shows its new text. */
const redrawTables = StateEffect.define<void>();

/** The redraw counter: the table widgets compare it in `eq`. */
export const tableStamp = StateField.define<number>({
  create: () => 0,
  update: (value, tr) => (tr.effects.some((effect) => effect.is(redrawTables)) ? value + 1 : value),
});

/** A table the user asked to show as markup (the `</>` button). */
const showTableSource = StateEffect.define<{ from: number; to: number }>();

/**
 * The table that is currently shown as markup.
 *
 * The cursor does not reveal a table: it steps over it, and cells open in
 * their own editors. So "show the markup" is a deliberate action, and this
 * field is the only place to keep it. It ends when the cursor leaves the table.
 */
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

/** Where the caret goes when a cell opens. */
type Place = "start" | "end" | "all" | { x: number; y: number };

/**
 * The cell that must be open. It outlives the DOM: a move to another cell
 * formats the table, the widget is built again, and the new DOM opens the cell
 * from here.
 */
let active: (At & { from: number; place: Place }) | null = null;

/** An open cell editor. */
interface CellEditor {
  view: EditorView;
  main: EditorView;
  box: HTMLElement;
  at: At;
  /** The editor goes away on purpose: its blur must not close the cell again. */
  closing: boolean;
}

/** The open cell editor; there is at most one in the window. */
let editing: CellEditor | null = null;

/** The cell editor in each table DOM: `destroy` must free it, even after it lost `editing`. */
const editors = new WeakMap<HTMLElement, CellEditor>();

/** The model of each drawn table: handlers read it, because it changes while a cell is typed in. */
const models = new WeakMap<HTMLElement, Table>();

/** The selection to put into the rebuilt table after a toolbar action. */
let moved: { from: number; rect: Rect } | null = null;

/** A drawn table. `table` is the ready `<table>`; `live-preview.ts` builds it. */
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
    // Compare the position too: an edit above the table does not change it but
    // moves it, and the model holds document positions.
    return (
      other.source === this.source &&
      other.model.from === this.model.from &&
      other.stamp === this.stamp
    );
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "cm-table";
    // This mark finds the table again after the widget is rebuilt.
    box.dataset.from = String(this.model.from);
    models.set(box, this.model);

    // A copy: CodeMirror can show the same widget again.
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

      // Do not intercept the press: while the mouse stays in the cell, normal
      // text selection works there. CodeMirror does not interfere: it skips
      // events inside the widget because `ignoreEvent` returned `true`.
      cell.onmousedown = (event) => {
        if (event.button !== 0 || inCellEditor(event)) return;
        const model = models.get(box)!;
        if (event.shiftKey) {
          // Extend the previous selection, as in any spreadsheet.
          event.preventDefault();
          drag.extend(box, model, at);
        } else {
          drag.start(box, model, at);
        }
      };

      // The mouse entered another cell with the button down: the user selects a
      // rectangle, not text, so we handle the selection from here.
      cell.onmouseenter = () => drag.to(box, models.get(box)!, at);

      cell.onmouseup = (event) => {
        if (event.button !== 0 || inCellEditor(event)) return;
        // A drag across cells or a text selection inside is not a click. Check
        // the mouse movement, not the selection: the browser does not clear the
        // old selection on a press in the widget, so any click after it would
        // look like the end of a drag.
        if (drag.moved || draggedFrom(event)) return;
        openAt(view, box, at, { x: event.clientX, y: event.clientY });
      };
    }

    for (const handle of table.querySelectorAll<HTMLElement>("[data-handle]")) {
      const kind = handle.dataset.handle as Handle;
      const at = Number(handle.dataset.index);

      // A handle is not text: there is nothing to select.
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
    // A toolbar action rebuilt the table: the selection follows the moved
    // rows or columns, so the next press moves them further.
    if (moved?.from === model.from) {
      const { rect } = moved;
      moved = null;
      select(box, model, { row: rect.top, column: rect.left }, { row: rect.bottom, column: rect.right });
    }
    paint(box, model);

    if (active?.from === model.from) openEditor(view, box, active, active.place);
    return box;
  }

  /**
   * Keeps the DOM while a cell is typed in. Each key changes the note, and a
   * new DOM would lose the cell editor with its cursor. A change of the table
   * shape (an undo that brings a row back) builds the table again; the cell
   * opens again from `active`.
   */
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

  /** The browser and the cell editor own the events here, not the note editor. */
  ignoreEvent(): boolean {
    return true;
  }

  /** The selection classes and the cell editor change the DOM; that is not a reason to rebuild. */
  ignoreMutation(): boolean {
    return true;
  }
}

function inCellEditor(event: Event): boolean {
  return (event.target as HTMLElement).closest(".cm-cell-editor") !== null;
}

/**
 * Opens a cell. If another cell of this table is open, the move goes through
 * the note: the table is formatted and built again, and the new DOM opens the
 * cell.
 */
function openAt(main: EditorView, box: HTMLElement, at: At, place: Place): void {
  const model = models.get(box)!;
  clearCells();
  if (editing?.box === box) {
    moveTo(editing, at, place);
    return;
  }
  if (editing) {
    // A cell of another table is open. Its formatting can move this table,
    // and it builds all tables again: the cell opens in the new DOM.
    const changes = closeCell(editing, null);
    active = { from: changes ? changes.mapPos(model.from) : model.from, ...at, place };
    main.dispatch({ effects: redrawTables.of() });
    return;
  }
  active = { from: model.from, ...at, place };
  openEditor(main, box, at, place);
}

/** Puts a CodeMirror editor with the cell markup into the cell. */
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

  editing = cell;
  editors.set(box, cell);
  active = { from: model.from, row, column, place };
  td.classList.add("is-editing");
  td.replaceChildren(cell.view.dom);
  // The toolbar acts on the open cell now.
  refreshTools(box, model);

  // The widget DOM is not in the document yet when `toDOM` runs. Focus and the
  // caret by coordinates need it there; a microtask comes after CodeMirror
  // puts the widget in. `requestAnimationFrame` would not do: it does not fire
  // in a background window.
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

/**
 * Writes the cell text into the note. The whole cell range is replaced: the
 * note keeps the cell trimmed, so offsets inside the cell editor and inside
 * the note do not match at the edges.
 */
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
  // A short row has no such cell yet: the row gets it when the table is
  // written again.
  const rows = cellsOf(model);
  rows[cell.at.row][cell.at.column] = text;
  cell.main.dispatch({
    changes: { from: model.from, to: model.to, insert: serializeTable(rows, model.aligns, model.indent) },
    userEvent: "input",
  });
}

/** The note changed under the open cell (undo, a sync): the cell editor shows the new text. */
function syncEditor(cell: CellEditor, model: Table): void {
  const text = model.rows[cell.at.row]?.[cell.at.column]?.text ?? "";
  const doc = cell.view.state.doc.toString();
  if (doc.trim() === text) return;
  cell.view.dispatch({
    changes: { from: 0, to: doc.length, insert: text },
    annotations: Transaction.remote.of(true),
  });
}

/**
 * Formats the table and closes the cell. `then` runs on the note editor after
 * that: it puts the cursor where the user goes.
 *
 * The formatting is not a separate undo step: ⌘Z after typing undoes the text,
 * not the spaces around it.
 */
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
  // The dispatch below removes the cell DOM with the browser selection in it.
  // WebKit then moves the selection to some text node below the table, and
  // the note editor takes it as a click there: the cursor jumped many lines.
  const selection = document.getSelection();
  if (selection && cell.view.dom.contains(selection.anchorNode)) selection.removeAllRanges();
  cell.main.dispatch({
    changes,
    effects: redrawTables.of(),
    // Only the padding changed: not a step for ⌘Z. A new or removed row is.
    annotations: rows ? [] : Transaction.addToHistory.of(false),
    userEvent: "input.table",
  });
  return changes;
}

/** Goes to another cell of the same table. */
function moveTo(cell: CellEditor, at: At, place: Place, rows?: string[][]): void {
  closeCell(cell, { ...at, place }, rows);
}

/** Leaves the table: the note editor gets the cursor below it, or above it. */
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

/** The table that starts at `from`, read again from the note. */
function parseTableAt(main: EditorView, from: number): Table | null {
  const { doc } = main.state;
  let last = doc.lineAt(from).number;
  while (last < doc.lines && doc.line(last + 1).text.trim() !== "") last += 1;
  return parseTable(doc.sliceString(from, doc.line(last).to), from);
}

/** The cell grid with the text of the open cell, which can differ from the note by spaces. */
function gridOf(cell: CellEditor): { model: Table; rows: string[][] } | null {
  const model = models.get(cell.box);
  if (!model) return null;
  const rows = cellsOf(model);
  rows[cell.at.row][cell.at.column] = cell.view.state.doc.toString().trim();
  return { model, rows };
}

/** Tab: the next cell in reading order; after the last one, a new row. */
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

/** ⇧Tab: the previous cell in reading order. */
function previousCell(cell: CellEditor): boolean {
  const model = models.get(cell.box);
  if (!model) return true;
  const { row, column } = cell.at;
  if (column > 0) moveTo(cell, { row, column: column - 1 }, "all");
  else if (row > 0) moveTo(cell, { row: row - 1, column: model.columns - 1 }, "all");
  return true;
}

/**
 * Enter: the same column one row below; after the last row, a new row.
 * Enter in an empty last row removes it and leaves the table, as a list ends.
 */
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

/** An arrow at the edge of the cell text goes to the neighbor cell. */
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

  // Up and down leave the cell only from its first or last visual line.
  const up = direction === "up";
  const edge = view.moveToLineBoundary(range, !up).head;
  if (edge !== (up ? 0 : view.state.doc.length)) return false;
  const next = row + (up ? -1 : 1);
  if (next < 0) leave(cell, "above");
  else if (next >= model.rows.length) leave(cell, "below");
  else moveTo(cell, { row: next, column }, up ? "end" : "start");
  return true;
}

/**
 * Text that comes into a cell must not break the table: a pipe would split
 * the cell, a line break the table. Pasted text is fixed the same way.
 */
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
  // The cursor goes after the inserted text, as it would without the fix.
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
        // A line break inside a cell: a real one would end the table.
        { key: "Shift-Enter", run: (view) => (view.dispatch(view.state.replaceSelection("<br>")), true) },
        { key: "Mod-Enter", run: () => (leave(cell, "below"), true) },
        { key: "Escape", run: () => (leave(cell, "after"), true) },
        { key: "ArrowUp", run: () => arrow(cell, "up") },
        { key: "ArrowDown", run: () => arrow(cell, "down") },
        { key: "ArrowLeft", run: () => arrow(cell, "left") },
        { key: "ArrowRight", run: () => arrow(cell, "right") },
        // One history: the cell edits are in the note, and so is the undo.
        { key: "Mod-z", run: () => undo(cell.main) },
        { key: "Mod-Shift-z", run: () => redo(cell.main) },
        { key: "Mod-y", run: () => redo(cell.main) },
        { key: "Mod-a", run: selectAll },
      ]),
    ),
    keymap.of(defaultKeymap),
    cellInput,
    drawSelection(),
    // A class through the facet: CodeMirror rewrites the class of its DOM itself.
    EditorView.editorAttributes.of({ class: "cm-cell-editor" }),
    EditorView.lineWrapping,
    markdown({ base: markdownLanguage }),
    syntaxHighlighting(highlightStyle),
    EditorView.domEventHandlers({
      // A click in the note or in another window closes the cell. A move to
      // another cell closes it on purpose first, and then this does nothing.
      blur: () => {
        if (!cell.closing && editing === cell) closeCell(cell, null);
        return false;
      },
    }),
  ];
}

/**
 * The selection is a rectangle of cells from one corner to the other (`Rect`).
 *
 * A rectangle, not "a row or a column": people select not only to act, but
 * also while reading a part of the table. A whole row or column is the same
 * rectangle at full width or height, so it needs no separate selection kind.
 */

/** A handle: row, column, or the corner that clears the selection. */
type Handle = "row" | "column" | "corner";

function rectOf(box: HTMLElement): Rect | null {
  const parts = box.dataset.selection?.split(",").map(Number);
  if (!parts || parts.length !== 4 || parts.some(Number.isNaN)) return null;
  return { top: parts[0], left: parts[1], bottom: parts[2], right: parts[3] };
}

/**
 * The table with the latest selection: ⌘C copies its rectangle.
 *
 * A selection stays drawn after the mouse leaves, because people read with it.
 * So a note can have several highlighted tables, but only one "current" table.
 */
let chosen: HTMLElement | null = null;

/** Sets the selection by two corners, in any order. */
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

/** A band: a whole row or a whole column; `null` for the corner. */
function band(model: Table, kind: Handle, at: number): [At, At] | null {
  if (kind === "row") return [{ row: at, column: 0 }, { row: at, column: model.columns - 1 }];
  if (kind === "column") {
    return [{ row: 0, column: at }, { row: model.rows.length - 1, column: at }];
  }
  return null;
}

/**
 * Paints the selection.
 *
 * It uses a class on each cell, not a selector on the widget attribute: CSS
 * cannot compare a cell attribute with an ancestor attribute. The rectangle
 * edges also get a border: it shows where the selection ends when neighbor
 * cells have the same color.
 */
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

  // A handle lights up when the selection touches its row or column, and
  // brighter when it covers all of it: this shows "part of a row" versus "a row".
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

/** Whether the selection covers the whole band, not a part of it. */
function whole(model: Table, rect: Rect, kind: Handle): boolean {
  return kind === "row"
    ? rect.left === 0 && rect.right === model.columns - 1
    : rect.top === 0 && rect.bottom === model.rows.length - 1;
}

/** Whether the node is inside a live preview widget, where the browser owns selection. */
function inWidget(node: globalThis.Node | null): boolean {
  const dom = node instanceof HTMLElement ? node : node?.parentElement;
  return Boolean(dom?.closest(".cm-code-block, .cm-table, .cm-diagram"));
}

/**
 * Mouse drag state: the start cell and whether a drag is active.
 *
 * One instance serves the window, not one per widget: an edit rebuilds the
 * widget in the middle of a press, and state tied to its DOM would die with it.
 * A drag works in only one table at a time; `table` remembers which one.
 */
class Drag {
  /** The mark (`data-from`) of the table being dragged in. */
  private table = "";
  /** The corner the rectangle grows from. */
  private anchor: At | null = null;
  /** The button is down: without this check, hover alone would select. */
  private pressed = false;
  /** The band a handle drag started from: rows and columns have their own anchor. */
  private bandKind: Handle | null = null;
  private bandAt = 0;
  /** The mouse left the pressed cell: this is a selection, not a click. */
  moved = false;

  start(box: HTMLElement, model: Table, at: At): void {
    this.table = box.dataset.from ?? "";
    this.anchor = at;
    this.bandKind = null;
    this.pressed = true;
    this.moved = false;
    // Clear the old rectangle at once: if the user then selects text in this
    // cell, the old selection would stay lit, and there would be two
    // highlights. A click opens this cell; a drag draws a new rectangle.
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

    // From here we own the selection: `user-select: none` stops the browser
    // selection, and `clear` removes one that started before.
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

  /** Bands from the start band to the one under the mouse. */
  private toBandRange(box: HTMLElement, model: Table, kind: Handle, at: number): void {
    const first = band(model, kind, Math.min(this.bandAt, at));
    const last = band(model, kind, Math.max(this.bandAt, at));
    if (!first || !last) return;
    select(box, model, first[0], last[1]);
  }

  /** Whether this is the table where the drag started. */
  private mine(box: HTMLElement): boolean {
    return box.dataset.from === this.table;
  }

  /**
   * Removes a text highlight that the browser started.
   *
   * During a rectangle drag we also listen to `selectionchange`: `user-select`
   * does not stop every selection already in progress, and it can creep into
   * the next cell letter by letter, which looks like dirt over the clean
   * rectangle. Removing the selection fires the same event, but the selection
   * is collapsed by then, so there is no loop.
   */
  private readonly clear = (): void => {
    if (!this.moved) return;
    const selection = document.getSelection();
    if (selection && !selection.isCollapsed) selection.removeAllRanges();
  };

  /**
   * Listens for button release on the whole document: users often release the
   * mouse outside the table, and the drag must stop then too.
   */
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
        // `moved` stays set until this release is handled: the cell `mouseup`
        // uses it to tell the end of a drag from a click.
        setTimeout(() => (this.moved = false));
      },
      { once: true },
    );
  }
}

const drag = new Drag();

/** The rectangle corner it grows from. */
function corner(rect: Rect | null): At | null {
  return rect ? { row: rect.top, column: rect.left } : null;
}

/**
 * With cells selected, ⌘C copies them, not the document line under the cursor.
 *
 * The listener is on the document, not on the widget: the copy event goes to
 * the focused element, which is the editor, and without interception
 * CodeMirror would copy the whole table markup. `capture` makes ours run first.
 *
 * Columns are separated by tabs and rows by line breaks: any spreadsheet on
 * the other side expects that. The copied text is the drawn text: users copy
 * what they see, not `**asterisks**`.
 */
function onCopy(event: ClipboardEvent): void {
  // An open cell copies its own text.
  if (document.activeElement?.closest(".cm-cell-editor")) return;
  // Selected text is copied instead of the rectangle. But CodeMirror does not
  // see a selection inside a widget: it would copy the document line under the
  // cursor, and ⌘X would also delete it. So the event must not reach the
  // editor; the browser copies the highlighted text itself.
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

/** A toolbar button, or a gap between button groups. */
type Tool = { command: Command | "source"; icon: IconName; title: Key } | "gap";

/**
 * Icons only: the toolbar floats above the table and must stay narrow. The
 * set is the commands of Advanced Tables: structure, alignment, moves, sort.
 */
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
  { command: "rowUp", icon: "rowUp", title: "editor.table.rowUp" },
  { command: "rowDown", icon: "rowDown", title: "editor.table.rowDown" },
  { command: "columnLeft", icon: "columnLeft", title: "editor.table.columnLeft" },
  { command: "columnRight", icon: "columnRight", title: "editor.table.columnRight" },
  "gap",
  { command: "sortAsc", icon: "sortAsc", title: "editor.table.sortAsc" },
  { command: "sortDesc", icon: "sortDesc", title: "editor.table.sortDesc" },
  { command: "transpose", icon: "transpose", title: "editor.table.transpose" },
  "gap",
  { command: "source", icon: "code", title: "editor.table.source" },
];

/** The table actions; "show the markup" is not one of them. */
export const COMMANDS = TOOLS.flatMap((tool) =>
  tool === "gap" || tool.command === "source" ? [] : [tool.command],
);

/** The toolbar for both table forms: the drawn table and the markup. */
function toolbar(
  run: (command: Command | "source") => void,
  disabled: (command: Command | "source") => boolean,
  markup = false,
): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "cm-table-tools";

  // The markup is already shown: the last group ("show the markup") has
  // nothing to do there.
  for (const tool of markup ? TOOLS.slice(0, -2) : TOOLS) {
    if (tool === "gap") {
      const gap = document.createElement("span");
      gap.className = "cm-table-tools-gap";
      panel.append(gap);
      continue;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.title = t(tool.title);
    button.ariaLabel = button.title;
    button.append(icon(tool.icon));
    button.dataset.command = tool.command;
    button.disabled = disabled(tool.command);
    // The press must not take the focus: an open cell would close itself.
    button.onmousedown = (event) => event.preventDefault();
    button.onclick = () => run(tool.command);
    panel.append(button);
  }

  return panel;
}

/** The target of a toolbar action: the selection, or else the open cell. */
function targetOf(box: HTMLElement): Rect | null {
  const rect = rectOf(box);
  if (rect) return rect;
  if (editing?.box !== box) return null;
  const { row, column } = editing.at;
  return { top: row, left: column, bottom: row, right: column };
}

/**
 * Updates what is possible now: each action is tried on the target.
 *
 * Without a target only "add" works, at the table edge; for the rest it is
 * unclear what to act on.
 */
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

/**
 * Runs a toolbar action on the drawn table.
 *
 * After "add" the new cell opens: the user adds a row to type into it. After
 * the other actions the selection follows the moved cells, and an open cell
 * stays open where its text went.
 */
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

  // Without a target, add at the edge: users expect that more than the middle.
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

/**
 * The toolbar above the table markup, while the cursor is in it. Its actions
 * work on the cursor cell. `off` lists the actions that are not possible
 * there: the widget is rebuilt only when this list changes.
 */
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

/** Clears the cell selection: the highlight and the buttons that need it. */
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

/** Esc clears the cell selection, like any other selection. */
function onKey(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !chosen?.isConnected) return;
  clearCells();
}

/**
 * A new document selection clears the cell selection.
 *
 * There are two selections for different things: cells are selected with the
 * mouse inside the widget, text by the editor itself. A cursor move does not
 * matter: a table selection stays after the mouse leaves, because people read
 * with it. But when a document selection appears (⌘A, a mouse drag), the
 * second highlight becomes false: something else is selected now.
 */
const cellsFollowSelection = EditorView.updateListener.of((update) => {
  if (update.selectionSet && !update.state.selection.main.empty) clearCells();
});

/**
 * The note editor must not take a browser selection inside a drawn table.
 *
 * The table and the open cell editor live inside the note DOM, so the note
 * editor sees their browser selection too and reads it as its own (a `select`
 * without a subtype). It is not a cursor in the note: a click in a cell moved
 * the note cursor to the table edge, and a selection in the cell editor, whose
 * DOM belongs to another view, mapped to a random position.
 */
const ignoreTableSelection = EditorState.transactionFilter.of((tr) => {
  if (tr.docChanged || tr.annotation(Transaction.userEvent) !== "select") return tr;
  return inTable(document.getSelection()?.anchorNode ?? null) ? [] : tr;
});

function inTable(node: globalThis.Node | null): boolean {
  const dom = node instanceof HTMLElement ? node : node?.parentElement;
  return Boolean(dom?.closest(".cm-table"));
}

/**
 * The table state and the window listeners.
 *
 * Copy and Esc listen on the whole document: the event goes to the focused
 * element, which is the editor, not the widget. Adding the same handler again
 * does nothing, so a new state (every note) does not multiply them. Cut uses
 * the same handler: for a widget or cell selection ⌘X works as ⌘C, or the
 * editor would delete the line under the cursor.
 */
export function tableWidgets(): Extension {
  document.addEventListener("copy", onCopy, true);
  document.addEventListener("cut", onCopy, true);
  document.addEventListener("keydown", onKey);
  return [tableStamp, sourceTable, cellsFollowSelection, ignoreTableSelection];
}
