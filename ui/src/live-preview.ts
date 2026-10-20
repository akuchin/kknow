/**
 * Live preview: the markup is drawn right in the editor.
 *
 * There are two rules. Inline markup (`**bold**`, `` `code` ``, links) is
 * revealed per element: the asterisks show only while the cursor or selection
 * touches that element, and its neighbors in the paragraph stay drawn
 * (`touched`). Headings and blocks are revealed by the cursor line
 * (`revealed`): there the element is the line, so blocks need no separate
 * toggle; it is enough to put the cursor in the block.
 *
 * Before, everything was revealed by line. But a markdown paragraph is one
 * line, so "reveal the line" meant "reveal the paragraph": touching it with the
 * cursor showed all its markup at once, and the text reflowed under the hand.
 *
 * The second reason to reveal a block is note search (⌘F): otherwise a match
 * hidden in a widget would not be visible. See `found`.
 *
 * A state field provides the decorations, not a view plugin: CodeMirror
 * requires this for anything that changes block layout, or it cannot compute
 * heights. The cost is a rebuild over the whole document, not only the visible
 * part; for a note this is cheap.
 */

import { syntaxTree } from "@codemirror/language";
import { getSearchQuery, searchPanelOpen } from "@codemirror/search";
import {
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type Range,
  type Transaction,
} from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";

import { addAgentNote } from "./agent-notes";
import { DIAGRAM_CLASS, DIAGRAM_LANG, renderDiagrams } from "./diagram";
import { paintCode } from "./highlight";
import { icon, type IconName } from "./icons";
import { imageElement, isImage, paintImages } from "./images";
import { t, type Key } from "./i18n";
import {
  cellsOf,
  insertColumn,
  insertRow,
  parseTable,
  removeColumns,
  removeRows,
  serializeTable,
  toCellSource,
  type Align,
  type Table,
} from "./table";

/** Syntax tree and node types, taken from CodeMirror itself. */
type Tree = ReturnType<typeof syntaxTree>;
type Node = ReturnType<Tree["resolveInner"]>;

/** Hides markup characters. */
const hidden = Decoration.replace({});

/** The visible part of a link, both `[[this]]` and `[this](url)`. */
const link = Decoration.mark({ class: "cm-live-link" });

const inlineCode = Decoration.mark({ class: "cm-inline-code" });

/** A code block line while it is edited; open and close are the fence lines. */
const codeLine = Decoration.line({ class: "cm-code-line" });
const codeOpen = Decoration.line({ class: "cm-code-open" });
const codeClose = Decoration.line({ class: "cm-code-close" });

/**
 * The markdown grammar has no wikilinks, so we find them with a text search,
 * not in the syntax tree.
 */
const WIKILINK = /\[\[([^\]\n]+)\]\]/g;

/** An embedded image `![[file.png|caption]]`, as the parser returns it. */
const WIKI_IMAGE = /^!\[\[[^\]|\n]+\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)\s*(\|[^\]\n]*)?\]\]$/i;

/** The theme changed: mermaid takes colors from its config, not from CSS. */
export const themeChanged = StateEffect.define<void>();

/** Whether to draw markup: with the preview open, the editor shows plain markdown. */
export const liveEnabled = StateEffect.define<boolean>();

/**
 * Whether live preview is on.
 *
 * This is a state field, not a `Compartment` with an extension: a field added
 * through a compartment at runtime gives no decorations, because CodeMirror
 * collects them when it creates the state. It is also simpler: live preview off
 * is an empty decoration set, and the rest of the code does not know about it.
 */
const enabled = StateField.define<boolean>({
  create: () => true,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(liveEnabled)) return effect.value;
    }
    return value;
  },
});

/**
 * Diagram generation number. CodeMirror treats a widget with the same source as
 * unchanged and keeps the drawn SVG; on a theme change that is wrong.
 */
const generation = StateField.define<number>({
  create: () => 0,
  update: (value, tr) =>
    tr.effects.some((effect) => effect.is(themeChanged)) ? value + 1 : value,
});

/** A table the user asked to show as markup (the `</>` button). */
export const showTableSource = StateEffect.define<{ from: number; to: number }>();

/**
 * The table that is currently shown as markup.
 *
 * It needs its own state because a table, unlike a code block, is not revealed
 * by the cursor: the cursor steps over it. So "show the markup" is a deliberate
 * action, and this field is the only place to keep it.
 */
const sourceTable = StateField.define<{ from: number; to: number } | null>({
  create: () => null,
  update(value, tr) {
    let next = value;

    if (next) {
      next = { from: tr.changes.mapPos(next.from, -1), to: tr.changes.mapPos(next.to, 1) };
      // The cursor left the table, so it is drawn as a table again.
      const cursor = tr.state.selection.main;
      if (cursor.from < next.from || cursor.to > next.to) next = null;
    }

    for (const effect of tr.effects) {
      if (effect.is(showTableSource)) next = effect.value;
    }
    return next;
  },
});

interface Live {
  decorations: DecorationSet;
  /** Table ranges: the cursor steps over them instead of going inside. */
  atomic: DecorationSet;
}

const live = StateField.define<Live>({
  create: (state) => build(state),
  update(value, tr) {
    const redraw = tr.effects.some((effect) => effect.is(themeChanged));
    const switched = tr.startState.field(enabled) !== tr.state.field(enabled);
    const source = tr.startState.field(sourceTable) !== tr.state.field(sourceTable);
    // Parsing is lazy: when a note opens, the tree can be incomplete, and the
    // rest is built in the background. Each step comes as a transaction without
    // edits or cursor moves. Without this check the end of the note would have
    // no widgets until something moves the cursor.
    const parsed = syntaxTree(tr.startState) !== syntaxTree(tr.state);
    // A cursor move changes the set of revealed lines.
    return tr.docChanged || tr.selection || redraw || switched || source || parsed || searchChanged(tr)
      ? build(tr.state)
      : value;
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value) => value.decorations),
    EditorView.atomicRanges.of((view) => view.state.field(field).atomic),
  ],
});

/** Note search changed: the set of revealed blocks depends on it too. */
function searchChanged(tr: Transaction): boolean {
  if (searchPanelOpen(tr.startState) !== searchPanelOpen(tr.state)) return true;
  return !getSearchQuery(tr.startState).eq(getSearchQuery(tr.state));
}

/** A diagram in place of a ```` ```mermaid ```` block. */
class DiagramWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly generation: number,
  ) {
    super();
  }

  eq(other: DiagramWidget): boolean {
    return other.source === this.source && other.generation === this.generation;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "cm-diagram";

    // Same markup as the core outputs for the preview, so the same code draws it.
    const block = document.createElement("pre");
    block.className = DIAGRAM_CLASS;
    block.textContent = this.source;
    box.append(block);

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "cm-diagram-edit";
    edit.title = t("editor.diagram.source");
    edit.ariaLabel = edit.title;
    edit.append(icon("code"));
    edit.onclick = () => editSource(view, box);
    box.append(edit);

    void renderDiagrams(box);
    // The button or a double click opens the source. A single click does not,
    // on purpose: a diagram is a picture, and a random click must not break it into text.
    box.ondblclick = () => editSource(view, box);
    return box;
  }

  /** A picture is not text: no reason to place the cursor or select in it. */
  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * An image in place of `![caption](file)` or `![[file.png]]`. A click opens it
 * over the whole window; the `</>` button opens the source, as for a diagram:
 * an image is not text, and a random click must not turn it into markup.
 */
class ImageWidget extends WidgetType {
  constructor(
    private readonly target: string,
    private readonly caption: string,
  ) {
    super();
  }

  eq(other: ImageWidget): boolean {
    return other.target === this.target && other.caption === this.caption;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("span");
    box.className = "cm-image";
    box.append(imageElement(this.target, this.caption));

    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "cm-diagram-edit";
    edit.title = t("editor.diagram.source");
    edit.ariaLabel = edit.title;
    edit.append(icon("code"));
    edit.onclick = () => {
      view.dispatch({ selection: { anchor: view.posAtDOM(box) } });
      view.focus();
    };
    box.append(edit);

    // The line height is known only after the image loads: without a new
    // measure, CodeMirror would miss clicks below it.
    paintImages(box, () => view.requestMeasure());
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * A code block widget: the same `<pre>` as in the preview, but one click opens
 * it for editing.
 *
 * Dragging belongs to the browser: people read and copy from a block more often
 * than they edit it. The click that ends a drag does not open editing, or it
 * would destroy the new selection (`click` also fires after a drag).
 */
class CodeWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly language: string,
  ) {
    super();
  }

  eq(other: CodeWidget): boolean {
    return other.source === this.source && other.language === this.language;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("pre");
    box.className = "cm-code-block";
    paintCode(box, this.source, this.language);
    box.title = t("editor.code.edit");
    box.onclick = (event) => {
      // A drag selects text, it does not ask for editing. The selection cannot
      // tell us this: the browser does not clear the old selection on a press in
      // a non-editable island, so after the first selection any click would
      // look like the end of a drag.
      if (draggedFrom(event)) return;
      editSource(view, box, clickedOffset(box, event));
    };
    return box;
  }

  /** The browser handles selection and clicks inside the block, not CodeMirror. */
  ignoreEvent(): boolean {
    return true;
  }
}

/**
 * Shows the block source: puts the cursor inside, and the general rule does the
 * rest (the cursor line shows as is). The first block line is the fence, so the
 * cursor goes to the second line, at `offset`, where the user clicked.
 */
function editSource(view: EditorView, dom: HTMLElement, offset = 0): void {
  const { doc } = view.state;
  const fence = doc.lineAt(view.posAtDOM(dom));
  const body = doc.line(Math.min(fence.number + 1, doc.lines));

  view.dispatch({ selection: { anchor: Math.min(body.from + offset, doc.length) } });
  view.focus();
}

/**
 * The source character under the click.
 *
 * The widget text equals the block body in the document, so the offset from
 * the click maps one to one. A click outside the text goes to the block start.
 */
function clickedOffset(dom: HTMLElement, event: MouseEvent): number {
  const range = document.caretRangeFromPoint?.(event.clientX, event.clientY);
  const node = range?.startContainer;
  if (!range || !(node instanceof Text) || !dom.contains(node)) return 0;

  // A highlighted block is a chain of `span`s, not one text node: add all the
  // text before the node to the offset inside it.
  let offset = range.startOffset;
  const walker = document.createTreeWalker(dom, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text && text !== node; text = walker.nextNode()) {
    offset += (text as Text).length;
  }
  return offset;
}

/**
 * The point of the last mouse press: the distance to it tells a click from the
 * end of a drag. One listener serves the document, as for copy.
 */
let pressedAt: { x: number; y: number } | null = null;

function onPress(event: MouseEvent): void {
  pressedAt = { x: event.clientX, y: event.clientY };
}

/** The mouse moved away from the press point: a drag, not a click. */
function draggedFrom(event: MouseEvent): boolean {
  if (!pressedAt) return false;
  return Math.abs(event.clientX - pressedAt.x) > 4 || Math.abs(event.clientY - pressedAt.y) > 4;
}

/**
 * A table: always drawn as a table and edited cell by cell.
 *
 * A press turns a cell into an input with its markup: the same "source under
 * the cursor" rule, but with its own cursor, not the editor one. So one click
 * does not turn the table into markup. A drag does not open editing: it selects
 * text in a cell or a rectangle of cells.
 */
class TableWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly model: Table,
    private readonly table: HTMLElement,
  ) {
    super();
  }

  eq(other: TableWidget): boolean {
    // Compare the position too: an edit above the table does not change it but
    // moves it, and the model holds document positions.
    return other.source === this.source && other.model.from === this.model.from;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("div");
    box.className = "cm-table";
    // This mark finds the cell again after the widget is rebuilt.
    box.dataset.from = String(this.model.from);

    // A copy: CodeMirror can show the same widget again.
    const table = this.table.cloneNode(true) as HTMLElement;
    box.append(tools(view, this.model, this.table, box), table);

    const model = this.model;

    for (const cell of table.querySelectorAll<HTMLElement>("[data-row]")) {
      const at = { row: Number(cell.dataset.row), column: Number(cell.dataset.column) };

      // Do not intercept the press: while the mouse stays in the cell, normal
      // text selection works there, because people read tables more than edit
      // them. CodeMirror does not interfere: it skips events inside the widget
      // because `ignoreEvent` returned `true`.
      cell.onmousedown = (event) => {
        if (event.button !== 0 || inInput(event)) return;
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
      cell.onmouseenter = () => drag.to(box, model, at);

      // The cell opens on button release, not on `click`: an edit of the
      // neighbor cell can rebuild the widget first, and then press and release
      // do not happen on the same element.
      cell.onmouseup = (event) => {
        if (event.button !== 0 || inInput(event)) return;
        // A drag across cells or a text selection inside is not a click. Check
        // the mouse movement, not the selection: the browser does not clear the
        // old selection on a press in the widget, so any click after it would
        // look like the end of a drag.
        if (drag.moved || draggedFrom(event)) return;
        openCell(view, model, this.table, box, cell);
      };
    }

    for (const handle of table.querySelectorAll<HTMLElement>("[data-handle]")) {
      const kind = handle.dataset.handle as Handle;
      const at = Number(handle.dataset.index);

      // A handle is not text: there is nothing to select. Focus must not leave
      // the open input, or it closes itself and rebuilds the widget before the press.
      handle.onmousedown = (event) => {
        event.preventDefault();
        if (event.button !== 0) return;
        // Close the open input first: its text must go into the document.
        // This can rebuild the widget; then the new DOM sets the selection.
        if (closeCell(view, model, this.table, box)) {
          repeat(view, model.from, kind, at, event.shiftKey);
          return;
        }
        if (event.shiftKey) drag.extendBand(box, model, kind, at);
        else drag.startBand(box, model, kind, at);
      };

      handle.onmouseenter = () => drag.toBand(box, model, kind, at);
    }

    paint(box, this.model);
    return box;
  }

  /** Edits happen in our own inputs; CodeMirror must not know about them. */
  ignoreEvent(): boolean {
    return true;
  }

  ignoreMutation(): boolean {
    return true;
  }
}

/** Opens a cell for editing: an input with its source replaces the markup. */
function openCell(
  view: EditorView,
  model: Table,
  pristine: HTMLElement,
  box: HTMLElement,
  cell: HTMLElement,
): void {
  const row = Number(cell.dataset.row);
  const column = Number(cell.dataset.column);

  // The previous input can change the document. Then the widget is rebuilt,
  // and the cell must open in the new DOM, not in this one.
  if (closeCell(view, model, pristine, box)) {
    focusCell(view, model.from, row, column);
    return;
  }

  // The open cell is also selected: the toolbar buttons act on the selection.
  select(box, model, { row, column });

  // The input is multiline: a long cell in one line scrolls sideways and is
  // edited blind. Here the text wraps at the column width, as in the drawn
  // cell, and the input height fits the text.
  const input = document.createElement("textarea");
  input.className = "cm-table-input";
  input.rows = 1;
  input.value = model.rows[row]?.[column]?.text ?? "";

  const fit = () => {
    input.style.height = "0";
    input.style.height = `${input.scrollHeight}px`;
  };
  input.oninput = fit;
  input.onblur = () => void closeCell(view, model, pristine, box);
  input.onkeydown = (event) => onCellKey(event, view, model, pristine, box, row, column);

  cell.replaceChildren(input);
  // The height uses `scrollHeight`, which exists only for an input in the document.
  fit();
  input.focus();
  // Cursor at the end: people append more often than they replace everything.
  input.setSelectionRange(input.value.length, input.value.length);
}

/**
 * Closes the open input: puts the markup back into the cell and, if the text
 * changed, writes it to the document.
 *
 * `true` means the document changed: the widget is rebuilt, and references to
 * its DOM are no longer valid.
 */
function closeCell(
  view: EditorView,
  model: Table,
  pristine: HTMLElement,
  box: HTMLElement,
): boolean {
  const input = box.querySelector<HTMLTextAreaElement>(".cm-table-input");
  const cell = input?.parentElement;
  if (!input || !cell) return false;

  const before = model.rows[Number(cell.dataset.row)]?.[Number(cell.dataset.column)];
  const next = toCellSource(input.value);

  // Removing the input fires blur, which would bring us here a second time.
  input.onblur = null;
  restoreCell(pristine, cell);

  if (!before || next === before.text) return false;
  view.dispatch({ changes: { from: before.from, to: before.to, insert: next } });
  return true;
}

/** Puts the drawn markup back into the cell from the pristine copy. */
function restoreCell(pristine: HTMLElement, cell: HTMLElement): void {
  const source = pristine.querySelector<HTMLElement>(
    `[data-row="${cell.dataset.row}"][data-column="${cell.dataset.column}"]`,
  );
  cell.replaceChildren(...[...(source?.childNodes ?? [])].map((node) => node.cloneNode(true)));
}

function onCellKey(
  event: KeyboardEvent,
  view: EditorView,
  model: Table,
  pristine: HTMLElement,
  box: HTMLElement,
  row: number,
  column: number,
): void {
  const input = event.currentTarget as HTMLTextAreaElement;

  if (event.key === "Escape") {
    event.preventDefault();
    // Cancel the edit: remove the input and write nothing to the document.
    input.onblur = null;
    if (input.parentElement) restoreCell(pristine, input.parentElement);
    view.focus();
    return;
  }

  if (event.key === "Enter") {
    event.preventDefault();
    closeCell(view, model, pristine, box);
    view.focus();
    return;
  }

  if (event.key === "Tab") {
    event.preventDefault();
    const next = neighbour(model, row, column, event.shiftKey ? -1 : 1);
    closeCell(view, model, pristine, box);
    focusCell(view, model.from, next.row, next.column);
  }
}

/** The neighbor cell in reading order; past the row end, the next row. */
function neighbour(
  model: Table,
  row: number,
  column: number,
  step: number,
): { row: number; column: number } {
  const at = row * model.columns + column + step;
  const total = model.rows.length * model.columns;
  const wrapped = ((at % total) + total) % total;

  return { row: Math.floor(wrapped / model.columns), column: wrapped % model.columns };
}

/**
 * Opens a cell after the widget was rebuilt.
 *
 * References to the old DOM are dead by now, so the cell is found again by its
 * marks, and a fake mouse event lets the normal handler do the rest.
 *
 * This is synchronous: CodeMirror rebuilds the DOM inside `dispatch`.
 * `requestAnimationFrame` looks tempting, but it never fires in a background
 * window, and Tab navigation would silently break there.
 */
function focusCell(view: EditorView, from: number, row: number, column: number): void {
  const cell = view.dom.querySelector<HTMLElement>(
    `.cm-table[data-from="${from}"] [data-row="${row}"][data-column="${column}"]`,
  );
  cell?.dispatchEvent(new MouseEvent("mouseup"));
}

interface At {
  row: number;
  column: number;
}

/**
 * The selection: a rectangle of cells from one corner to the other.
 *
 * A rectangle, not "a row or a column": people select not only to act, but
 * also while reading a part of the table. A whole row or column is the same
 * rectangle at full width or height, so it needs no separate selection kind.
 */
interface Rect {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

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

function inInput(event: MouseEvent): boolean {
  return (event.target as HTMLElement).closest(".cm-table-input") !== null;
}

/**
 * Mouse drag state: the start cell and whether a drag is active.
 *
 * One instance serves the window, not one per widget: a cell edit rebuilds the
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
    // highlights. A click selects this cell again; a drag draws a new rectangle.
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
  // A cell is being edited: the copy applies to it.
  if (document.activeElement?.classList.contains("cm-table-input")) return;
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

/**
 * Repeats a handle press in the rebuilt widget.
 *
 * After a document edit, references to the old DOM are dead, so the handle is
 * found again by its marks, and a fake mouse event lets the normal handler do
 * the rest.
 */
function repeat(view: EditorView, from: number, kind: Handle, at: number, shift: boolean): void {
  const handle = view.dom.querySelector<HTMLElement>(
    `.cm-table[data-from="${from}"] [data-handle="${kind}"][data-index="${at}"]`,
  );
  handle?.dispatchEvent(new MouseEvent("mousedown", { shiftKey: shift }));
}

/** The action of a button in the toolbar above the table. */
type Command = "row+" | "row-" | "column+" | "column-" | "source";

/** Icons only: the toolbar floats above the table and must stay narrow. */
const TOOLS: { command: Command; icon: IconName; title: Key }[] = [
  { command: "row+", icon: "rowAdd", title: "editor.table.rowAdd" },
  { command: "row-", icon: "rowRemove", title: "editor.table.rowRemove" },
  { command: "column+", icon: "columnAdd", title: "editor.table.columnAdd" },
  { command: "column-", icon: "columnRemove", title: "editor.table.columnRemove" },
  { command: "source", icon: "code", title: "editor.table.source" },
];

function tools(
  view: EditorView,
  model: Table,
  pristine: HTMLElement,
  box: HTMLElement,
): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "cm-table-tools";

  for (const tool of TOOLS) {
    const button = document.createElement("button");
    button.type = "button";
    button.title = t(tool.title);
    button.ariaLabel = button.title;
    button.append(icon(tool.icon));
    button.dataset.command = tool.command;
    // Without this, the press would take focus from the cell, the input would
    // close itself and rebuild the widget, and the click would never arrive.
    button.onmousedown = (event) => event.preventDefault();
    button.onclick = () => run(view, model, pristine, box, tool.command);
    panel.append(button);
  }

  return panel;
}

/**
 * Updates what can be removed now.
 *
 * Removal takes the whole selection, however many rows or columns it covers.
 * Without a selection it is unclear what to remove. The header and the last
 * column are never removed: without them the table is not a table.
 */
function refreshTools(box: HTMLElement, model: Table): void {
  const rect = rectOf(box);

  for (const button of box.querySelectorAll<HTMLButtonElement>(".cm-table-tools button")) {
    if (button.dataset.command === "row-") {
      // Only the header is selected: nothing to remove.
      button.disabled = rect === null || rect.bottom < 1;
    }
    if (button.dataset.command === "column-") {
      const width = rect === null ? 0 : rect.right - rect.left + 1;
      button.disabled = rect === null || model.columns - width < 1;
    }
  }
}

function run(
  view: EditorView,
  model: Table,
  pristine: HTMLElement,
  box: HTMLElement,
  command: Command,
): void {
  if (command === "source") {
    closeCell(view, model, pristine, box);
    view.dispatch({
      effects: showTableSource.of({ from: model.from, to: model.to }),
      selection: { anchor: model.from },
    });
    view.focus();
    return;
  }

  // An open input is part of the table too: take its text into the model, or
  // the cell edit would be lost in the rebuild.
  const cells = cellsOf(model);
  const input = box.querySelector<HTMLTextAreaElement>(".cm-table-input");
  const open = input?.parentElement;
  if (input && open) {
    const row = Number(open.dataset.row);
    if (cells[row]) cells[row][Number(open.dataset.column)] = toCellSource(input.value);
  }

  // Without a selection, add at the edge: users expect that more than the
  // middle. Add after the selection; remove the whole selection.
  const rect = rectOf(box) ?? {
    top: model.rows.length - 1,
    left: model.columns - 1,
    bottom: model.rows.length - 1,
    right: model.columns - 1,
  };

  const next: { rows: string[][]; aligns: Align[] } =
    command === "row+"
      ? { rows: insertRow(cells, rect.bottom), aligns: model.aligns }
      : command === "row-"
        ? { rows: removeRows(cells, rect.top, rect.bottom), aligns: model.aligns }
        : command === "column+"
          ? insertColumn(cells, model.aligns, rect.right)
          : removeColumns(cells, model.aligns, rect.left, rect.right);

  view.dispatch({
    changes: { from: model.from, to: model.to, insert: serializeTable(next.rows, next.aligns) },
  });

  // The new cell is ready for typing; after a removal the user picks a cell again.
  if (command === "row+") {
    focusCell(view, model.from, Math.max(1, rect.bottom + 1), rect.left);
  }
  if (command === "column+") focusCell(view, model.from, rect.top, rect.right + 1);
}

/**
 * Whether the cursor or selection touches the element, bounds inclusive.
 *
 * Inclusive, so the user can add a letter at the end of a bold word without
 * revealing it by hand, and the cursor never lands inside hidden characters:
 * it reveals the element as it approaches. A selection reveals everything it
 * touches, ⌘A too: this shows exactly what gets copied.
 */
function touched(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((range) => range.from <= to && range.to >= from);
}

/**
 * Whether to show a heading or block as is: yes if the cursor or a single-line
 * selection touches its lines.
 *
 * A multiline selection does not reveal a block. It means an action on a
 * chunk (copy, delete, replace), not an edit, so there is nothing to reveal.
 * Before, ⌘A turned diagrams and code blocks into raw text, and the user saw
 * hashes and pipes instead of the note.
 */
function revealed(state: EditorState, from: number, to: number): boolean {
  const first = state.doc.lineAt(from).from;
  const last = state.doc.lineAt(to).to;

  return state.selection.ranges.some(
    (range) =>
      range.from <= last &&
      range.to >= first &&
      state.doc.lineAt(range.from).from === state.doc.lineAt(range.to).from,
  );
}

/**
 * Whether the selection keeps a revealed block open: yes if the range is fully
 * inside the block, even across several lines.
 *
 * The "multiline selection does not reveal" rule does not fit here: it is
 * about ⌘A and drags through a block, while selecting several code lines inside
 * the block is an edit. Without this exception the block collapsed into a
 * widget in the middle of a drag, together with the selection and edit point.
 */
function heldOpen(state: EditorState, from: number, to: number): boolean {
  const first = state.doc.lineAt(from).from;
  const last = state.doc.lineAt(to).to;

  return state.selection.ranges.some((range) => range.from >= first && range.to <= last);
}

/**
 * Whether note search found anything inside the range.
 *
 * This is for blocks: lines hidden in a widget get no decorations, so a match
 * in a code block or table would be invisible until the user reaches it with
 * "next". So a block with a match reveals itself, as it does for the cursor.
 */
function found(state: EditorState, from: number, to: number): boolean {
  // A closed panel leaves the query in the state: without this check, blocks
  // would stay revealed after the search ends.
  if (!searchPanelOpen(state)) return false;

  const query = getSearchQuery(state);
  if (!query.valid) return false;
  return !query.getCursor(state, from, to).next().done;
}

function build(state: EditorState): Live {
  // Live preview off means no decorations: the text shows as it is in the file.
  if (!state.field(enabled)) return { decorations: Decoration.none, atomic: Decoration.none };

  const marks: Range<Decoration>[] = [];
  const tables: Range<Decoration>[] = [];
  /** Ranges shown fully as source: nothing is decorated there. */
  const raw: [number, number][] = [];
  const tree = syntaxTree(state);
  const age = state.field(generation);
  const opened = state.field(sourceTable);

  tree.iterate({
    enter: (node) => {
      if (node.name === "FencedCode") {
        if (
          revealed(state, node.from, node.to) ||
          heldOpen(state, node.from, node.to) ||
          found(state, node.from, node.to)
        ) {
          addCodeLines(state, node.from, node.to, marks);
        } else {
          addBlock(state, node.node, age, marks);
        }
        // No need to go inside the block: it has no markup.
        return false;
      }

      if (node.name === "Table") {
        // A table is always drawn as a table: cells are edited in place.
        // Show the markup only on the button request or when ⌘F finds a match.
        const asked = opened !== null && opened.from <= node.from && opened.to >= node.to;
        if (asked || found(state, node.from, node.to)) {
          raw.push([node.from, node.to]);
        } else {
          addTable(state, node.from, node.to, marks, tables);
        }
        return false;
      }
      if (node.name === "Image") {
        // The parser also treats `![[diagram.png]]` as an image, but without a
        // URL: `addWikilinks` draws it, and the brackets inside need no hiding.
        if (WIKI_IMAGE.test(state.doc.sliceString(node.from, node.to))) return false;
        // A hidden image skips its children: the widget already shows the caption.
        return !addImage(state, node.node, marks);
      }
      // Marks hide based on their owner: `StrongEmphasis` for `**`, `InlineCode`
      // for a backtick, `Link` for brackets. A cursor inside the owner shows its
      // markup; the rest of the paragraph stays drawn.
      const owner = node.node.parent;

      switch (node.name) {
        case "HeaderMark": {
          // A heading is revealed by line: the line is the heading.
          if (revealed(state, node.from, node.to)) break;
          // Hashes exist only in `## Text` headings; in underlined headings the
          // same mark covers a whole line, and it must not be hidden.
          if (!owner?.name.startsWith("ATXHeading")) break;
          let to = node.to;
          // The space after the hashes is markup too: otherwise the text shifts right.
          while (state.doc.sliceString(to, to + 1) === " ") to += 1;
          marks.push(hidden.range(node.from, to));
          break;
        }
        case "EmphasisMark":
        case "CodeMark":
          if (owner && touched(state, owner.from, owner.to)) break;
          marks.push(hidden.range(node.from, node.to));
          break;
        case "InlineCode":
          // The background stays with visible backticks: it is still code.
          marks.push(inlineCode.range(node.from, node.to));
          break;
        case "LinkMark":
        case "URL":
          // An image has the same marks, but they must not hide:
          // `![caption](file)` would leave a stub `!caption`.
          if (owner?.name !== "Link") break;
          if (touched(state, owner.from, owner.to)) break;
          marks.push(hidden.range(node.from, node.to));
          break;
        case "Link":
          marks.push(link.range(node.from, node.to));
          break;
        case "Comment":
          // `<!-- @agent … -->`: the words are highlighted, the comment is hidden.
          addAgentNote(state, node.from, node.to, marks);
          break;
      }
      return true;
    },
  });

  addWikilinks(state, tree, raw, marks);
  return {
    // Decorations come from two passes, so they need sorting.
    decorations: Decoration.set(marks, true),
    atomic: Decoration.set(tables, true),
  };
}

/**
 * `![caption](file)`: a widget replaces the markup. While the cursor touches
 * it, the markup shows with the image below it, so the user sees what changes.
 * Returns whether the markup is hidden.
 */
function addImage(state: EditorState, node: Node, marks: Range<Decoration>[]): boolean {
  const [open, close] = node.getChildren("LinkMark");
  const url = node.getChild("URL");
  if (!open || !close || !url) return false;

  const caption = state.doc.sliceString(open.to, close.from);
  const target = state.doc.sliceString(url.from, url.to).replace(/^<(.*)>$/, "$1");
  return placeImage(state, node.from, node.to, target, caption, marks);
}

function placeImage(
  state: EditorState,
  from: number,
  to: number,
  target: string,
  caption: string,
  marks: Range<Decoration>[],
): boolean {
  const widget = new ImageWidget(target, caption);
  if (touched(state, from, to)) {
    marks.push(Decoration.widget({ widget, side: 1 }).range(to));
    return false;
  }
  marks.push(Decoration.replace({ widget }).range(from, to));
  return true;
}

/** A whole block: mermaid as a picture, the rest as the same `<pre>` as in the preview. */
function addBlock(state: EditorState, node: Node, age: number, marks: Range<Decoration>[]): void {
  const info = node.getChild("CodeInfo");
  const named = info ? state.doc.sliceString(info.from, info.to).trim() : "";
  const language = named.split(/\s+/)[0].toLowerCase();

  const body = node.getChild("CodeText");
  // The line break before the closing fence is not part of the code.
  const source = body ? state.doc.sliceString(body.from, body.to).replace(/\n$/, "") : "";

  const widget =
    language === DIAGRAM_LANG
      ? new DiagramWidget(source, age)
      : new CodeWidget(source, language);

  // A block decoration must cover whole lines.
  const from = state.doc.lineAt(node.from).from;
  const to = state.doc.lineAt(node.to).to;
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
}

/**
 * A code block with the cursor in it: the background and border stay, but the
 * text is normal.
 *
 * Line decorations, not a widget: the edit must change the real text. Line
 * wrapping is on here: CodeMirror computes caret coordinates from the content,
 * and a line with its own scroll would put the caret in the wrong place.
 */
function addCodeLines(
  state: EditorState,
  from: number,
  to: number,
  marks: Range<Decoration>[],
): void {
  const first = state.doc.lineAt(from).number;
  const last = state.doc.lineAt(to).number;

  for (let number = first; number <= last; number += 1) {
    const line = state.doc.line(number);
    marks.push(codeLine.range(line.from));
    if (number === first) marks.push(codeOpen.range(line.from));
    if (number === last) marks.push(codeClose.range(line.from));
  }
}

function addTable(
  state: EditorState,
  nodeFrom: number,
  nodeTo: number,
  marks: Range<Decoration>[],
  tables: Range<Decoration>[],
): void {
  const from = state.doc.lineAt(nodeFrom).from;
  const to = state.doc.lineAt(nodeTo).to;
  const source = state.doc.sliceString(from, to);

  const model = parseTable(source, from);
  // Parsing failed: better to keep the markup than to lose it.
  if (!model) return;

  const widget = new TableWidget(source, model, tableDom(state, model));
  marks.push(Decoration.replace({ widget, block: true }).range(from, to));
  tables.push(hidden.range(from, to));
}

/**
 * Builds a `<table>` from the model.
 *
 * Cells carry row and column numbers: editing and the toolbar buttons use them
 * to find cells.
 *
 * Row and column handles sit at the edges: a press on a handle selects the
 * whole row or column. This tells the toolbar buttons what to remove, and it
 * also helps reading: in a wide table the eye loses the row.
 */
function tableDom(state: EditorState, model: Table): HTMLElement {
  const table = document.createElement("table");
  const head = document.createElement("thead");
  const body = document.createElement("tbody");

  head.append(handleRow(model));

  for (const [index, cells] of model.rows.entries()) {
    const header = index === 0;
    const tr = document.createElement("tr");
    tr.append(handle("row", index, t("editor.table.selectRow")));

    for (let column = 0; column < model.columns; column += 1) {
      const cell = cells[column];
      const td = document.createElement(header ? "th" : "td");
      td.dataset.row = String(index);
      td.dataset.column = String(column);

      const align = model.aligns[column];
      if (align) td.style.textAlign = align;
      // The cell can be missing: a row shorter than others is valid.
      if (cell) td.append(inlineDom(state, cell.from, cell.to));
      tr.append(td);
    }

    (header ? head : body).append(tr);
  }

  table.append(head);
  if (body.childElementCount > 0) table.append(body);
  return table;
}

/** The handle row above the table: the corner and one per column. */
function handleRow(model: Table): HTMLElement {
  const tr = document.createElement("tr");
  tr.className = "cm-table-handles";
  tr.append(handle("corner", 0, t("editor.table.selectNone")));

  for (let column = 0; column < model.columns; column += 1) {
    tr.append(handle("column", column, t("editor.table.selectColumn")));
  }
  return tr;
}

function handle(kind: "row" | "column" | "corner", index: number, title: string): HTMLElement {
  const cell = document.createElement("th");
  cell.className = kind === "corner" ? "cm-table-corner" : "cm-table-handle";
  cell.dataset.handle = kind;
  cell.dataset.index = String(index);
  cell.title = title;
  cell.ariaLabel = title;
  return cell;
}

/** Inline markup in a cell gets the same class that the decorations use. */
const INLINE_CLASS: Record<string, string> = {
  InlineCode: "cm-inline-code",
  Link: "cm-live-link",
  StrongEmphasis: "cm-strong",
  Emphasis: "cm-emphasis",
};

/**
 * Cell content as ready DOM.
 *
 * The rules match the text decorations, but the table is a widget, and
 * decorations do not reach inside a widget, so the markup is parsed again.
 * Text goes in only as text nodes: it can come from someone else's vault.
 */
function inlineDom(state: EditorState, from: number, to: number): DocumentFragment {
  const tree = syntaxTree(state);
  const hidden: [number, number][] = [];
  const styled: [number, number, string][] = [];

  tree.iterate({
    from,
    to,
    enter: (node) => {
      const style = INLINE_CLASS[node.name];
      if (style) {
        styled.push([node.from, node.to, style]);
      } else if (node.name === "EmphasisMark" || node.name === "CodeMark") {
        hidden.push([node.from, node.to]);
      } else if (node.name === "LinkMark" || node.name === "URL") {
        if (node.node.parent?.name === "Link") hidden.push([node.from, node.to]);
      }
      return true;
    },
  });

  for (const match of state.doc.sliceString(from, to).matchAll(WIKILINK)) {
    const start = from + (match.index ?? 0);
    const end = start + match[0].length;
    if (insideMarkup(tree, start)) continue;
    styled.push([start, end, "cm-live-link"]);
    hidden.push([start, start + 2], [end - 2, end]);
  }

  return assemble(state, from, to, hidden, styled);
}

/** Splits the range into runs with the same style and builds DOM from them. */
function assemble(
  state: EditorState,
  from: number,
  to: number,
  hidden: [number, number][],
  styled: [number, number, string][],
): DocumentFragment {
  const parts = document.createDocumentFragment();
  let text = "";
  let style = "";

  const flush = () => {
    if (text === "") return;
    if (style === "") {
      parts.append(text);
    } else {
      const span = document.createElement("span");
      span.className = style;
      span.textContent = text;
      parts.append(span);
    }
    text = "";
  };

  for (let pos = from; pos < to; pos += 1) {
    if (hidden.some(([start, end]) => pos >= start && pos < end)) continue;
    // The last match is the innermost: the tree walk goes top down.
    const inner = styled.filter(([start, end]) => pos >= start && pos < end).at(-1);
    const next = inner ? inner[2] : "";

    if (next !== style) {
      flush();
      style = next;
    }
    text += state.doc.sliceString(pos, pos + 1);
  }
  flush();

  return parts;
}

function addWikilinks(
  state: EditorState,
  tree: Tree,
  raw: [number, number][],
  marks: Range<Decoration>[],
): void {
  // Scan the whole note text: notes are short, a per-line walk does not pay off.
  for (const match of state.doc.toString().matchAll(WIKILINK)) {
    const from = match.index ?? 0;
    const to = from + match[0].length;
    if (raw.some(([start, end]) => from >= start && from < end)) continue;

    // `![[diagram.png|300]]` is an embedded image: the target is before the
    // first pipe, the caption with size after it, as in `![300](diagram.png)`.
    // The parser sees an `Image` without a URL, so `insideMarkup` would reject
    // it; here only code matters.
    const [target, ...caption] = match[1].split("|");
    if (state.doc.sliceString(from - 1, from) === "!" && isImage(target)) {
      if (!insideCode(tree, from)) {
        placeImage(state, from - 1, to, target.trim(), caption.join("|"), marks);
      }
      continue;
    }
    if (touched(state, from, to) || insideMarkup(tree, from)) continue;

    marks.push(link.range(from, to));
    marks.push(hidden.range(from, from + 2));
    marks.push(hidden.range(to - 2, to));
  }
}

/**
 * Inside code, `[[this]]` is plain text, and inside a link the parser already
 * hides these brackets: a second decoration over the first would be a bug.
 */
function insideMarkup(tree: Tree, pos: number): boolean {
  for (let node: Node | null = tree.resolveInner(pos, 1); node; node = node.parent) {
    if (node.name.includes("Code") || node.name.includes("Link")) return true;
  }
  return false;
}

function insideCode(tree: Tree, pos: number): boolean {
  for (let node: Node | null = tree.resolveInner(pos, 1); node; node = node.parent) {
    if (node.name.includes("Code")) return true;
  }
  return false;
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
  for (const button of chosen.querySelectorAll<HTMLButtonElement>(".cm-table-tools button")) {
    if (button.dataset.command?.endsWith("-")) button.disabled = true;
  }
  chosen = null;
}

/** Esc clears the cell selection, like any other selection. */
function onKey(event: KeyboardEvent): void {
  if (event.key !== "Escape" || !chosen?.isConnected) return;
  if (document.activeElement?.classList.contains("cm-table-input")) return;

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

export function livePreview(): Extension {
  // Copy and Esc listen on the whole document: the event goes to the focused
  // element, which is the editor, not the widget. Adding the same handler again
  // does nothing, so a new state (every note) does not multiply them. Cut uses
  // the same handler: for a widget or cell selection ⌘X works as ⌘C, or the
  // editor would delete the line under the cursor.
  document.addEventListener("copy", onCopy, true);
  document.addEventListener("cut", onCopy, true);
  document.addEventListener("keydown", onKey);
  document.addEventListener("mousedown", onPress, true);

  // Order matters: the diagram generation, the toggle and the table source
  // mode must update before the decorations that read them.
  return [generation, enabled, sourceTable, live, cellsFollowSelection];
}
