import { EMPTY_QUERY, findRanges, type FindQuery } from "./find-ranges";
import { t } from "./i18n";

const SKIPPED = "button, input, svg, script, style, [aria-hidden='true']";
const BLOCKS = "p, li, td, th, h1, h2, h3, h4, h5, h6, pre, blockquote, div, dt, dd, figcaption, summary";
const ALL = "preview-find";
const CURRENT = "preview-find-current";

type Piece = { node: Text; from: number; to: number };
type Match = Piece[];

function highlightsAvailable(): boolean {
  return typeof CSS !== "undefined" && "highlights" in CSS && typeof Highlight !== "undefined";
}

function blockOf(node: Node, body: HTMLElement): Element {
  const block = node.parentElement?.closest(BLOCKS);
  return block && body.contains(block) ? block : body;
}

function textBlocks(body: HTMLElement): Text[][] {
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest(SKIPPED) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  });
  const groups: { block: Element; nodes: Text[] }[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const block = blockOf(node, body);
    const last = groups[groups.length - 1];
    if (last && last.block === block) last.nodes.push(node as Text);
    else groups.push({ block, nodes: [node as Text] });
  }
  return groups.map((group) => group.nodes);
}

function piecesOf(nodes: Text[], from: number, to: number): Match {
  const pieces: Match = [];
  let offset = 0;
  for (const node of nodes) {
    const length = node.data.length;
    const start = Math.max(from, offset);
    const end = Math.min(to, offset + length);
    if (start < end) pieces.push({ node, from: start - offset, to: end - offset });
    offset += length;
  }
  return pieces;
}

function rangeOf(match: Match): Range {
  const range = document.createRange();
  range.setStart(match[0].node, match[0].from);
  range.setEnd(match[match.length - 1].node, match[match.length - 1].to);
  return range;
}

function clearHighlights(): void {
  if (!highlightsAvailable()) return;
  CSS.highlights.delete(ALL);
  CSS.highlights.delete(CURRENT);
}

function toggle(label: string, checked: boolean, change: (value: boolean) => void): HTMLLabelElement {
  const box = document.createElement("label");
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => change(input.checked));
  box.append(input, ` ${label}`);
  return box;
}

function button(label: string, text: string, click: () => void): HTMLButtonElement {
  const item = document.createElement("button");
  item.type = "button";
  item.className = "find-button";
  item.title = label;
  item.setAttribute("aria-label", label);
  item.textContent = text;
  item.addEventListener("click", click);
  return item;
}

export class PreviewFind {
  private readonly bar = document.createElement("div");
  private readonly input = document.createElement("input");
  private readonly counter = document.createElement("span");
  private readonly observer = new MutationObserver(() => this.schedule());
  private query: FindQuery = { ...EMPTY_QUERY };
  private ordinal = 0;
  private matches: Match[] = [];
  private marks: HTMLElement[][] = [];
  private frame = 0;
  private opened = false;

  constructor(private readonly bodyOf: () => HTMLElement) {
    this.build();
  }

  private get body(): HTMLElement {
    return this.bodyOf();
  }

  isOpen(): boolean {
    return this.opened;
  }

  state(): { query: FindQuery; ordinal: number } {
    return { query: { ...this.query }, ordinal: this.ordinal };
  }

  open(query?: FindQuery, ordinal = 0): void {
    const selected = query ? "" : this.selectedText();
    if (query) {
      this.query = { ...query };
      this.ordinal = ordinal;
    } else if (selected !== "") {
      this.query = { ...this.query, text: selected };
      this.ordinal = 0;
    }
    if (!this.opened) {
      this.body.before(this.bar);
      this.opened = true;
      this.observe();
    }
    this.syncControls();
    this.search();
    this.input.focus();
    this.input.select();
  }

  close(): void {
    if (!this.opened) return;
    window.cancelAnimationFrame(this.frame);
    this.observer.disconnect();
    this.unwrap();
    clearHighlights();
    this.matches = [];
    this.bar.remove();
    this.opened = false;
  }

  next(): void {
    this.step(1);
  }

  previous(): void {
    this.step(-1);
  }

  private build(): void {
    this.bar.className = "find-bar";
    this.bar.setAttribute("role", "search");
    this.bar.setAttribute("aria-label", t("find.bar"));
    this.input.type = "text";
    this.input.className = "find-input";
    this.input.placeholder = t("editor.search.find");
    this.input.setAttribute("aria-label", t("editor.search.find"));
    this.input.addEventListener("input", () => {
      this.query = { ...this.query, text: this.input.value };
      this.ordinal = 0;
      this.search();
    });
    this.counter.className = "find-count";
    this.counter.setAttribute("aria-live", "polite");
    this.bar.addEventListener("keydown", (event) => this.onKey(event));
    document.addEventListener("keydown", (event) => this.onShortcut(event));
  }

  private syncControls(): void {
    this.input.value = this.query.text;
    const toggles = [
      toggle(t("editor.search.matchCase"), this.query.caseSensitive, (value) => this.option({ caseSensitive: value })),
      toggle(t("editor.search.byWord"), this.query.wholeWord, (value) => this.option({ wholeWord: value })),
      toggle(t("editor.search.regexp"), this.query.regexp, (value) => this.option({ regexp: value })),
    ];
    const close = button(t("editor.search.close"), "×", () => this.close());
    close.classList.add("find-close");
    this.bar.replaceChildren(
      this.input,
      button(t("editor.search.previous"), "↑", () => this.previous()),
      button(t("editor.search.next"), "↓", () => this.next()),
      ...toggles,
      this.counter,
      close,
    );
  }

  private option(change: Partial<FindQuery>): void {
    this.query = { ...this.query, ...change };
    this.ordinal = 0;
    this.search();
  }

  private onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      this.close();
    } else if (event.key === "Enter" && event.target === this.input) {
      event.preventDefault();
      this.step(event.shiftKey ? -1 : 1);
    }
  }

  private onShortcut(event: KeyboardEvent): void {
    if (!this.opened || event.defaultPrevented || event.altKey) return;
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "g") return;
    event.preventDefault();
    this.step(event.shiftKey ? -1 : 1);
  }

  private selectedText(): string {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || selection.rangeCount === 0) return "";
    const range = selection.getRangeAt(0);
    return this.body.contains(range.commonAncestorContainer) ? selection.toString().trim() : "";
  }

  private observe(): void {
    this.observer.observe(this.body, { childList: true, subtree: true, characterData: true });
  }

  private schedule(): void {
    if (!this.opened) return;
    window.cancelAnimationFrame(this.frame);
    this.frame = window.requestAnimationFrame(() => this.search());
  }

  private search(): void {
    if (!this.opened) return;
    this.observer.disconnect();
    this.unwrap();
    const groups = textBlocks(this.body);
    const texts = groups.map((nodes) => nodes.map((node) => node.data).join(""));
    const ranges = findRanges(texts, this.query);
    if (ranges === null) this.input.setAttribute("aria-invalid", "true");
    else this.input.removeAttribute("aria-invalid");
    this.matches = (ranges ?? [])
      .map((range) => piecesOf(groups[range.block], range.from, range.to))
      .filter((match) => match.length > 0);
    this.paint();
    this.observe();
  }

  private step(direction: 1 | -1): void {
    const total = this.matches.length;
    if (total === 0) return;
    this.ordinal = (this.current() + direction + total) % total;
    this.search();
  }

  private current(): number {
    return Math.min(this.ordinal, Math.max(this.matches.length - 1, 0));
  }

  private paint(): void {
    const total = this.matches.length;
    const current = this.current();
    this.counter.textContent =
      total === 0 ? t("find.none") : t("find.count", { current: current + 1, total });
    if (total === 0) {
      clearHighlights();
      return;
    }
    if (highlightsAvailable()) this.paintHighlights(current);
    else this.paintMarks(current);
  }

  private paintHighlights(current: number): void {
    const ranges = this.matches.map(rangeOf);
    CSS.highlights.set(ALL, new Highlight(...ranges));
    CSS.highlights.set(CURRENT, new Highlight(ranges[current]));
    this.reveal(ranges[current].getBoundingClientRect(), ranges[current].startContainer.parentElement);
  }

  private paintMarks(current: number): void {
    const pieces = this.matches.flatMap((match, index) => match.map((piece) => ({ piece, index })));
    this.marks = this.matches.map(() => []);
    for (const { piece, index } of pieces.reverse()) {
      const target = piece.node.splitText(piece.from);
      target.splitText(piece.to - piece.from);
      const mark = document.createElement("mark");
      mark.className = index === current ? "find-match find-current" : "find-match";
      target.replaceWith(mark);
      mark.append(target);
      this.marks[index].unshift(mark);
    }
    const first = this.marks[current][0];
    this.reveal(first.getBoundingClientRect(), first);
  }

  private reveal(rect: DOMRect, element: Element | null): void {
    const view = this.body.getBoundingClientRect();
    if (rect.top >= view.top && rect.bottom <= view.bottom) return;
    element?.scrollIntoView({ block: "center" });
  }

  private unwrap(): void {
    for (const mark of Array.from(this.body.querySelectorAll("mark.find-match"))) {
      const parent = mark.parentNode;
      mark.replaceWith(...Array.from(mark.childNodes));
      parent?.normalize();
    }
    this.marks = [];
  }
}
