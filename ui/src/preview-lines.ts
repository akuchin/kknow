/**
 * Source lines in the preview: they let "Read" mode show and place bookmarks.
 *
 * The core marks the start of a paragraph, heading and list item with an empty
 * `<span class="line-mark" data-line="N">`. Before a code block, table and
 * rule it puts the same mark with `is-block`. Everything else is computed here:
 * a line inside a paragraph comes from the line breaks in its text (a markdown
 * soft break stays a line break in the HTML).
 */

const MARK = ".line-mark";

/** The block of a mark: for `is-block`, the next element. */
function blockOf(mark: HTMLElement): HTMLElement | null {
  return mark.classList.contains("is-block")
    ? (mark.nextElementSibling as HTMLElement | null)
    : mark.parentElement;
}

/** The last mark at or before `node` in document order. */
function markBefore(root: HTMLElement, node: Node): HTMLElement | null {
  let found: HTMLElement | null = null;
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    const before =
      mark === node || (mark.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    if (!before) break;
    found = mark;
  }
  return found;
}

/** Number of line breaks in `block` text before (`node`, `offset`). */
function breaksBefore(block: HTMLElement, node: Node, offset: number): number {
  let count = 0;
  const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT);
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    const value = text.textContent ?? "";
    if (text === node) return count + (value.slice(0, offset).match(/\n/g)?.length ?? 0);
    count += value.match(/\n/g)?.length ?? 0;
  }
  return count;
}

/**
 * The source line of a position in the preview; `null` if there are no marks
 * (this happens for a separate file).
 *
 * Inside a paragraph, heading, list item and code block, line breaks refine
 * the line. A table does not: its HTML has its own line breaks, and the
 * `|---|` separator line is not visible in the preview at all, so the bookmark
 * goes to the start of the table.
 */
export function lineAt(root: HTMLElement, node: Node, offset: number): number | null {
  const mark = markBefore(root, node);
  if (!mark) return null;
  const line = Number(mark.dataset.line);
  const block = blockOf(mark);
  if (!block || !block.contains(node)) return line;

  if (block.tagName === "PRE") {
    // The window already replaced the diagram with a picture; it has no text.
    if (block.classList.contains("mermaid")) return line;
    // The mark is on the ``` line; the code starts on the next line.
    return line + 1 + breaksBefore(block, node, offset);
  }
  if (/^(P|LI|H[1-6])$/.test(block.tagName)) return line + breaksBefore(block, node, offset);
  return line;
}

/**
 * The line of the mark at or before `node`, without a refinement. A task box
 * stands right after the mark of its list item, so this is the task line.
 */
export function markLine(root: HTMLElement, node: Node): number | null {
  const mark = markBefore(root, node);
  return mark ? Number(mark.dataset.line) : null;
}

/** The line of the first block still visible at the top of the preview. */
export function topLine(root: HTMLElement): number | null {
  const pane = root.getBoundingClientRect();
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    const block = blockOf(mark) ?? mark;
    if (block.getBoundingClientRect().bottom > pane.top + 4) return Number(mark.dataset.line);
  }
  return null;
}

/**
 * Scrolls the block of `line` to the top of the preview. Without a mark for
 * the line, it takes the nearest mark above. Without marks, it does nothing.
 */
export function scrollToLine(root: HTMLElement, line: number): void {
  let best: HTMLElement | null = null;
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    if (Number(mark.dataset.line) > line) break;
    best = mark;
  }
  if (best) (blockOf(best) ?? best).scrollIntoView({ block: "start" });
}

/**
 * Bookmark icons in the preview go on the mark of the bookmarked line. If there
 * is no such mark (the bookmark is inside a paragraph or code block), they go
 * on the nearest mark above.
 */
export function paintBookmarks(root: HTMLElement, lines: number[]): void {
  const marks = [...root.querySelectorAll<HTMLElement>(MARK)];
  const marked = new Set<HTMLElement>();
  for (const line of lines) {
    let best: HTMLElement | null = null;
    for (const mark of marks) {
      if (Number(mark.dataset.line) > line) break;
      best = mark;
    }
    if (best) marked.add(best);
  }
  for (const mark of marks) mark.classList.toggle("is-bookmarked", marked.has(mark));
}
