/**
 * Lists in live preview.
 *
 * - A bullet `-`, `*` or `+` shows as a dot. Deeper levels get a ring and
 *   a square, as in a browser, so the levels differ at a glance.
 * - A task `- [ ]` shows as a box without the bullet. A click on the box
 *   marks the task as done or open: one edit, ⌘Z undoes it.
 * - The number of an ordered item stays: it is text that the user wrote,
 *   and the list keys keep it in order.
 *
 * The markup shows while the cursor is inside it. The cursor at the start of
 * the item text does not show it: there the user types after Enter.
 */

import type { EditorState, Range } from "@codemirror/state";
import { Decoration, WidgetType, type EditorView } from "@codemirror/view";
import type { syntaxTree } from "@codemirror/language";

type Node = ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]>;

const BULLETS = ["•", "◦", "▪"];

class BulletWidget extends WidgetType {
  constructor(readonly depth: number) {
    super();
  }

  eq(other: BulletWidget): boolean {
    return other.depth === this.depth;
  }

  toDOM(): HTMLElement {
    const dot = document.createElement("span");
    dot.className = "cm-list-bullet";
    dot.textContent = BULLETS[this.depth % BULLETS.length];
    return dot;
  }
}

class TaskWidget extends WidgetType {
  /** `offset`: from the widget start to the character inside `[ ]`. */
  constructor(
    readonly done: boolean,
    readonly offset: number,
  ) {
    super();
  }

  eq(other: TaskWidget): boolean {
    return other.done === this.done && other.offset === this.offset;
  }

  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "cm-task-box";
    box.checked = this.done;
    box.addEventListener("mousedown", (event) => {
      // The editor must not move the cursor here: that would show the markup
      // under the hand.
      event.preventDefault();
      const at = view.posAtDOM(box) + this.offset;
      view.dispatch({
        changes: { from: at, to: at + 1, insert: this.done ? " " : "x" },
        userEvent: "input.task",
      });
    });
    return box;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/** Does the cursor or a selection touch `[from, to)`? The end is free. */
function inside(state: EditorState, from: number, to: number): boolean {
  return state.selection.ranges.some((range) => range.from < to && range.to >= from);
}

/** The end of the spaces after `to`. */
function spaceEnd(state: EditorState, to: number): number {
  let end = to;
  while (state.doc.sliceString(end, end + 1) === " ") end += 1;
  return end;
}

/** How many lists hold this item, from 0. */
function depthOf(item: Node): number {
  let depth = -1;
  for (let node: Node | null = item; node; node = node.parent) {
    if (node.name === "BulletList" || node.name === "OrderedList") depth += 1;
  }
  return Math.max(0, depth);
}

/** Draws the `ListMark` of an item; `marks` gets the decorations. */
export function addListMark(state: EditorState, mark: Node, marks: Range<Decoration>[]): void {
  const item = mark.parent;
  if (item?.name !== "ListItem") return;
  const bullet = item.parent?.name === "BulletList";
  const taskMarker = item.getChild("Task")?.getChild("TaskMarker") ?? null;

  if (taskMarker) {
    // `- [ ] ` is one widget; in `1. [ ] ` only the box is one.
    const from = bullet ? mark.from : taskMarker.from;
    const to = spaceEnd(state, taskMarker.to);
    if (inside(state, from, to)) return;
    const done = state.doc.sliceString(taskMarker.from + 1, taskMarker.from + 2) !== " ";
    marks.push(
      Decoration.replace({ widget: new TaskWidget(done, taskMarker.from + 1 - from) }).range(
        from,
        Math.min(to, taskMarker.to + 1),
      ),
    );
    return;
  }

  if (!bullet || inside(state, mark.from, spaceEnd(state, mark.to))) return;
  marks.push(Decoration.replace({ widget: new BulletWidget(depthOf(item)) }).range(mark.from, mark.to));
}
