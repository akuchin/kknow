import { autocompletion } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { EditorSelection, EditorState, type StateEffect } from "@codemirror/state";
import { EditorView, drawSelection, keymap } from "@codemirror/view";

import {
  agentNotes,
  insertNote,
  notesOf,
  previewAnchor,
  revealNote,
  type AgentNote,
} from "./agent-notes";
import type { BookmarkMark } from "./api";
import { editorImages, insertImageLinks, type SaveImage } from "./attachments";
import { bookmarkMenu } from "./bookmark-menu";
import {
  bookmarkChanged,
  bookmarkLines,
  bookmarkMarks,
  bookmarks,
  goToBookmark,
  goToLine,
  setBookmarkMarks,
  toggleBookmark,
  toggleBookmarkAt,
  type BookmarkChange,
} from "./bookmarks";
import { editorGutter, setLineNumbers } from "./gutter";
import { confluence } from "./confluence";
import { editorMenu } from "./editor-menu";
import { codeLanguages, highlightStyle } from "./highlight";
import { MAC, bindings } from "./actions";
import { followLinks, linkAt } from "./editor-links";
import { linkCompletion, type LinkAnchors, type LinkNote } from "./link-complete";
import { moveCommand, shiftCommand } from "./list-editing";
import { t } from "./i18n";
import { clearEmbeds, embedsChanged } from "./live-embeds";
import { liveEnabled, livePreview, themeChanged } from "./live-preview";
import { headingsOf, type Heading } from "./outline";
import { tableEditing } from "./table-editing";
import { taskBox } from "./tasks";
import { addNoteInCell } from "./table-widget";

const theme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "calc(15px * var(--note-scale, 1))",
    backgroundColor: "transparent",
    color: "var(--text)",
  },
  ".cm-content": {
    fontFamily: "var(--font-text)",
    lineHeight: "1.65",
    padding: "24px 28px 40vh 6px",
    caretColor: "var(--accent)",
    minWidth: "0",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { overflow: "auto" },
  ".cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection) !important",
  },
  ".cm-selectionBackground": {
    borderRadius: "4px",
  },

  ".cm-panels": {
    backgroundColor: "var(--bg-soft)",
    color: "var(--text)",
    fontFamily: "var(--font-text)",
  },
  ".cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-search": { padding: "6px 30px 6px 10px" },
  ".cm-search .cm-textfield": {
    fontSize: "13px",
    color: "var(--text)",
    backgroundColor: "var(--bg)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "3px 8px",
  },
  ".cm-search .cm-textfield:focus-visible": {
    outline: "1px solid var(--accent)",
  },
  ".cm-search .cm-button": {
    fontSize: "13px",
    color: "var(--text)",
    background: "var(--bg)",
    backgroundImage: "none",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    padding: "3px 10px",
    cursor: "pointer",
  },
  ".cm-search .cm-button:hover": { background: "var(--bg-raised)" },
  ".cm-search label": { fontSize: "12px", color: "var(--text-muted)" },
  ".cm-search [name=close]": {
    color: "var(--text-muted)",
    fontSize: "18px",
    cursor: "pointer",
  },

  ".cm-searchMatch": {
    backgroundColor: "color-mix(in oklab, var(--accent) 35%, transparent)",
    borderRadius: "3px",
  },
  ".cm-searchMatch-selected": {
    backgroundColor: "color-mix(in oklab, var(--accent) 70%, transparent)",
  },

  ".cm-tooltip.cm-tooltip-autocomplete": {
    backgroundColor: "var(--bg-raised)",
    color: "var(--text)",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    padding: "4px",
    boxShadow: "0 6px 24px color-mix(in oklab, black 18%, transparent)",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul": {
    fontFamily: "var(--font-text)",
    fontSize: "14px",
    maxHeight: "16em",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
    padding: "3px 8px",
    borderRadius: "5px",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "color-mix(in oklab, var(--accent) 22%, transparent)",
    color: "var(--text)",
  },
  ".cm-completionMatchedText": { textDecoration: "none", fontWeight: "600" },
  ".cm-completionDetail": {
    marginLeft: "0.8em",
    fontStyle: "normal",
    color: "var(--text-muted)",
  },
  ".cm-tooltip.cm-completionInfo": {
    backgroundColor: "var(--bg-raised)",
    color: "var(--text-muted)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    fontFamily: "var(--font-text)",
    fontSize: "13px",
  },
});

const blockSelection = EditorView.contentAttributes.compute(["selection"], (state) => {
  const across = state.selection.ranges.some(
    (range) =>
      !range.empty &&
      state.doc.lineAt(range.from).from !== state.doc.lineAt(range.to).from,
  );
  const attrs: Record<string, string> = across ? { "data-selection": "block" } : {};
  return attrs;
});

const PHRASES = {
  Find: t("editor.search.find"),
  Replace: t("editor.search.replace"),
  next: t("editor.search.next"),
  previous: t("editor.search.previous"),
  all: t("editor.search.all"),
  "match case": t("editor.search.matchCase"),
  regexp: t("editor.search.regexp"),
  "by word": t("editor.search.byWord"),
  replace: t("editor.search.replaceOne"),
  "replace all": t("editor.search.replaceAll"),
  close: t("editor.search.close"),
  "current match": t("editor.search.current"),
  "Go to line": t("editor.search.gotoLine"),
  go: t("editor.search.go"),
  "on line": t("editor.search.onLine"),
};

function toggleInline(marker: string) {
  return (view: EditorView): boolean => {
    const { state } = view;
    const changes = state.changeByRange((range) => {
      const { from, to } = range;
      const before = state.sliceDoc(Math.max(0, from - marker.length), from);
      const after = state.sliceDoc(to, Math.min(state.doc.length, to + marker.length));
      if (before === marker && after === marker) {
        return {
          changes: [
            { from: from - marker.length, to: from },
            { from: to, to: to + marker.length },
          ],
          range: EditorSelection.range(from - marker.length, to - marker.length),
        };
      }

      const inner = state.sliceDoc(from, to);
      if (inner.length >= marker.length * 2 && inner.startsWith(marker) && inner.endsWith(marker)) {
        return {
          changes: { from, to, insert: inner.slice(marker.length, inner.length - marker.length) },
          range: EditorSelection.range(from, to - marker.length * 2),
        };
      }

      return {
        changes: [
          { from, insert: marker },
          { from: to, insert: marker },
        ],
        range: EditorSelection.range(from + marker.length, to + marker.length),
      };
    });
    view.dispatch(changes, { scrollIntoView: true, userEvent: "input" });
    return true;
  };
}

const formatKeymap = [
  ...bindings("format.bold", toggleInline("**")),
  ...bindings("format.italic", toggleInline("*")),
  ...bindings("format.strike", toggleInline("~~")),
];

const listKeymap = [
  { key: "Tab", run: shiftCommand(1) },
  { key: "Shift-Tab", run: shiftCommand(-1) },
  ...bindings("item.up", moveCommand(-1)),
  ...bindings("item.down", moveCommand(1)),
];

export interface EditorLinks {
  notes(): readonly LinkNote[];
  anchors(target: string): Promise<LinkAnchors | null>;
  follow(target: string): void;
  peek?(target: string | null, x: number, y: number): void;
}

function links(options: EditorLinks) {
  return [
    autocompletion({
      override: [linkCompletion({ ...options, headings: headingsOf })],
      icons: false,
    }),
    followLinks(options.follow, MAC, options.peek),
    keymap.of(
      bindings("link.follow", (view) => {
        const target = linkAt(view.state, view.state.selection.main.head);
        if (!target) return false;
        options.follow(target);
        return true;
      }),
    ),
  ];
}

export interface EditorPosition {
  anchor: number;
  head: number;
  scroll: StateEffect<unknown>;
}

export interface EditorOptions {
  onChange: (text: string) => void;
  onNotes?: (notes: AgentNote[]) => void;
  onBookmark?: (marks: BookmarkMark[], change: BookmarkChange) => void;
  onBookmarkStep?: (direction: 1 | -1) => void;
  saveImage?: SaveImage;
  links?: EditorLinks;
}

export class Editor {
  private view: EditorView;
  private options: EditorOptions;
  private live = true;
  private lineNumbers = false;
  private textListeners: (() => void)[] = [];

  constructor(parent: HTMLElement, options: EditorOptions) {
    this.options = options;
    this.view = new EditorView({ parent, state: this.createState("") });
  }

  private createState(text: string): EditorState {
    const { options } = this;
    return EditorState.create({
      doc: text,
      extensions: [
        history(),
        drawSelection(),
        search({ top: true }),
        tableEditing(),
        EditorState.phrases.of(PHRASES),
        keymap.of([
          ...searchKeymap,
          ...formatKeymap,
          ...listKeymap,
          ...bindings("bookmark.next", () => (options.onBookmarkStep?.(1), true)),
          ...bindings("bookmark.previous", () => (options.onBookmarkStep?.(-1), true)),
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        markdown({ base: markdownLanguage, codeLanguages, completeHTMLTags: false }),
        options.links ? links(options.links) : [],
        livePreview(),
        agentNotes(),
        confluence(),
        bookmarks(),
        editorGutter(this.lineNumbers),
        editorMenu(),
        bookmarkMenu(),
        blockSelection,
        options.saveImage ? editorImages(options.saveImage) : [],
        syntaxHighlighting(highlightStyle),
        EditorView.lineWrapping,
        theme,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            options.onChange(update.state.doc.toString());
            for (const listener of this.textListeners) listener();
          }
          const changed = bookmarkChanged(update);
          if (changed) options.onBookmark?.(bookmarkMarks(update.state), changed.change);
          if (update.docChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
            this.reportNotes();
          }
        }),
      ],
    });
  }

  setLinks(links: EditorLinks): void {
    this.options = { ...this.options, links };
  }

  setText(text: string): void {
    this.view.setState(this.createState(text));
    this.applyLive();
    this.reportNotes();
  }

  private reportNotes(): void {
    this.options.onNotes?.(notesOf(this.view.state));
  }

  revealNote(note: AgentNote): void {
    revealNote(this.view, note);
  }

  addNoteFromPreview(words: string, index: number, total: number, text: string): boolean {
    const to = previewAnchor(this.view.state, words, index, total);
    if (to === null) return false;
    insertNote(this.view, to, words, text);
    return true;
  }

  addNoteInCell(cell: Element, words: string, index: number, total: number, text: string): boolean {
    return addNoteInCell(this.view, cell, words, index, total, text);
  }

  get scroller(): HTMLElement {
    return this.view.scrollDOM;
  }

  getPosition(): EditorPosition {
    const { anchor, head } = this.view.state.selection.main;
    return { anchor, head, scroll: this.view.scrollSnapshot() };
  }

  setPosition(position: EditorPosition | undefined): void {
    if (!position) return;
    const length = this.view.state.doc.length;
    this.view.dispatch({
      selection: EditorSelection.single(
        Math.min(position.anchor, length),
        Math.min(position.head, length),
      ),
      effects: position.scroll,
    });
  }

  getBookmarks(): number[] {
    return bookmarkLines(this.view.state);
  }

  getBookmarkMarks(): BookmarkMark[] {
    return bookmarkMarks(this.view.state);
  }

  setBookmarks(marks: BookmarkMark[], enabled: boolean): void {
    setBookmarkMarks(this.view, marks, enabled);
  }

  toggleBookmark(): void {
    toggleBookmark(this.view);
  }

  toggleBookmarkLine(line: number): void {
    toggleBookmarkAt(this.view, line);
  }

  goToLine(line: number, at: "center" | "start" = "center"): void {
    goToLine(this.view, line, at);
    this.view.focus();
  }

  goToBookmark(direction: 1 | -1): number | null {
    const line = goToBookmark(this.view, direction);
    if (line !== null) this.view.focus();
    return line;
  }

  onText(listener: () => void): void {
    this.textListeners.push(listener);
  }

  topLine(): number {
    return this.lineAtHeight(this.view.scrollDOM.scrollTop);
  }

  bottomLine(): number {
    const scroller = this.view.scrollDOM;
    return this.lineAtHeight(scroller.scrollTop + scroller.clientHeight - 1);
  }

  private lineAtHeight(scrolled: number): number {
    const view = this.view;
    const height = Math.max(0, scrolled - view.documentPadding.top);
    return view.state.doc.lineAt(view.lineBlockAtHeight(height).from).number;
  }

  getHeadings(): Heading[] {
    return headingsOf(this.view.state);
  }

  setTask(line: number, done: boolean): boolean {
    const doc = this.view.state.doc;
    if (line < 1 || line > doc.lines) return false;
    const { from, text } = doc.line(line);
    const box = taskBox(text);
    if (!box) return false;
    if (box.done !== done) {
      this.view.dispatch({
        changes: { from: from + box.at, to: from + box.at + 1, insert: done ? "x" : " " },
      });
    }
    return true;
  }

  dropPosition(x: number, y: number): number | null {
    const target = document.elementFromPoint(x, y);
    if (!target || !this.view.dom.contains(target)) return null;
    return this.view.posAtCoords({ x, y }, false);
  }

  insertImages(links: string[], at?: number): void {
    insertImageLinks(this.view, links, at);
  }

  getText(): string {
    return this.view.state.doc.toString();
  }

  focus(): void {
    this.view.focus();
  }

  setLive(on: boolean): void {
    if (this.live === on) return;
    this.live = on;
    this.applyLive();
  }

  setLineNumbers(on: boolean): void {
    if (this.lineNumbers === on) return;
    this.lineNumbers = on;
    setLineNumbers(this.view, on);
  }

  private applyLive(): void {
    this.view.dispatch({ effects: liveEnabled.of(this.live) });
  }

  openSearch(): void {
    openSearchPanel(this.view);
  }

  setScale(scale: number): void {
    const dom = this.view.dom;
    if (scale === 1) dom.style.removeProperty("--note-scale");
    else dom.style.setProperty("--note-scale", String(scale));
    this.view.requestMeasure();
  }

  redrawEmbeds(): void {
    clearEmbeds();
    if (this.view.state.doc.toString().includes("![[")) {
      this.view.dispatch({ effects: embedsChanged.of(undefined) });
    }
  }

  redrawDiagrams(): void {
    this.view.dispatch({ effects: themeChanged.of(undefined) });
  }
}
