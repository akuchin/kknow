const MARK = ".line-mark";

function blockOf(mark: HTMLElement): HTMLElement | null {
  return mark.classList.contains("is-block")
    ? (mark.nextElementSibling as HTMLElement | null)
    : mark.parentElement;
}

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

export function lineAt(root: HTMLElement, node: Node, offset: number): number | null {
  const mark = markBefore(root, node);
  if (!mark) return null;
  const line = Number(mark.dataset.line);
  const block = blockOf(mark);
  if (!block || !block.contains(node)) return line;

  if (block.tagName === "PRE") {
    if (block.classList.contains("mermaid")) return line;
    return line + 1 + breaksBefore(block, node, offset);
  }
  if (/^(P|LI|H[1-6])$/.test(block.tagName)) return line + breaksBefore(block, node, offset);
  return line;
}

export function markLine(root: HTMLElement, node: Node): number | null {
  const mark = markBefore(root, node);
  return mark ? Number(mark.dataset.line) : null;
}

export function topLine(root: HTMLElement): number | null {
  const pane = root.getBoundingClientRect();
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    const block = blockOf(mark) ?? mark;
    if (block.getBoundingClientRect().bottom > pane.top + 4) return Number(mark.dataset.line);
  }
  return null;
}

export function bottomLine(root: HTMLElement): number | null {
  const pane = root.getBoundingClientRect();
  let found: number | null = null;
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    if (mark.getBoundingClientRect().top >= pane.bottom) break;
    found = Number(mark.dataset.line);
  }
  return found;
}

export function scrollToLine(root: HTMLElement, line: number): void {
  let best: HTMLElement | null = null;
  for (const mark of root.querySelectorAll<HTMLElement>(MARK)) {
    if (Number(mark.dataset.line) > line) break;
    best = mark;
  }
  if (best) (blockOf(best) ?? best).scrollIntoView({ block: "start" });
}

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
