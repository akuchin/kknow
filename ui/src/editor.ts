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
import {
  bookmarkLines,
  bookmarkToggled,
  bookmarks,
  goToLine,
  setBookmarkLines,
  toggleBookmark,
  toggleBookmarkAt,
} from "./bookmarks";
import { codeLanguages, highlightStyle } from "./highlight";
import { t } from "./i18n";
import { liveEnabled, livePreview, themeChanged } from "./live-preview";
import { tableEditing } from "./table-editing";

const theme = EditorView.theme({
  "&": {
    height: "100%",
    // The window sets the factor, per file (`note-scale.ts`).
    fontSize: "calc(15px * var(--note-scale, 1))",
    backgroundColor: "transparent",
    color: "var(--text)",
  },
  ".cm-content": {
    fontFamily: "var(--font-text)",
    lineHeight: "1.65",
    padding: "24px 28px 40vh",
    caretColor: "var(--accent)",
    // Without this, a wide code block would stretch the editor content, and the
    // whole text would scroll sideways. This way only the block scrolls.
    maxWidth: "100%",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { overflow: "auto" },
  ".cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection) !important",
  },
  // The selection is one rectangle per line: full width for widgets (table,
  // code block, diagram), line length for text. With rounded corners the
  // difference looks like a highlighter stroke, not like broken rectangles.
  ".cm-selectionBackground": {
    borderRadius: "4px",
  },

  // Note search panel. CodeMirror has its own colors for it and picks light or
  // dark by its own setting, not by our variables, so the panel and its fields
  // are restyled completely.
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

  // Match color derives from the theme accent, so it stays visible on both
  // light and dark backgrounds.
  ".cm-searchMatch": {
    backgroundColor: "color-mix(in oklab, var(--accent) 35%, transparent)",
    borderRadius: "3px",
  },
  ".cm-searchMatch-selected": {
    backgroundColor: "color-mix(in oklab, var(--accent) 70%, transparent)",
  },
});

/**
 * Marks a selection that spans several lines.
 *
 * Widgets need it. A table, code block and diagram are plain DOM, and the mouse
 * selects text in them the browser way. Inside one line this is right: it lets
 * users read and copy a table. But when the selection spans the document, a
 * second per-cell highlight lies over the CodeMirror band, and together they
 * look like boxes with gaps instead of one band. So during such a selection
 * the widgets' own selection is off (`styles.css`, `[data-selection="block"]`).
 */
const blockSelection = EditorView.contentAttributes.compute(["selection"], (state) => {
  const across = state.selection.ranges.some(
    (range) =>
      !range.empty &&
      state.doc.lineAt(range.from).from !== state.doc.lineAt(range.to).from,
  );
  // An empty object means "nothing to add"; the explicit type lets both
  // branches share one type.
  const attrs: Record<string, string> = across ? { "data-selection": "block" } : {};
  return attrs;
});

/**
 * Search panel labels: CodeMirror reads them from this map, keyed by the
 * English phrase in its source. The values come from the UI dictionary, so the
 * panel language follows the window language.
 */
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

/**
 * Wraps or unwraps the selection with a paired marker: `**`, `*`, `~~`.
 *
 * A second press removes the markers, both when they surround the selection
 * and when they are selected with the text. With no selection it inserts an
 * empty pair and puts the cursor between the markers, as other editors do.
 */
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
        // Both insert positions use old document coordinates: CodeMirror
        // handles the shift from the first insert.
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

/** Bold, italic, strikethrough. */
const formatKeymap = [
  { key: "Mod-b", run: toggleInline("**") },
  { key: "Mod-i", run: toggleInline("*") },
  { key: "Mod-Shift-x", run: toggleInline("~~") },
];

/**
 * Where the user was reading a note: cursor and scroll.
 *
 * The scroll is a CodeMirror snapshot, not a pixel count: it is tied to a line,
 * so after heights change (widgets, line wrapping) it shows the same text.
 */
export interface EditorPosition {
  anchor: number;
  head: number;
  scroll: StateEffect<unknown>;
}

export interface EditorOptions {
  onChange: (text: string) => void;
  /** Agent notes in the open text, for the list in the window header. */
  onNotes?: (notes: AgentNote[]) => void;
  /** A bookmark was toggled; `lines` holds all bookmark lines after it. */
  onBookmark?: (lines: number[], set: boolean) => void;
}

/** A CodeMirror wrapper that exposes only the text. */
export class Editor {
  private view: EditorView;
  private options: EditorOptions;
  /**
   * Whether to draw the markup right in the text.
   *
   * It is off while the preview is open: two renderings side by side are
   * useless, and with the result on the right, plain markdown fits the left.
   */
  private live = true;

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
        // Panel on top: at the bottom it covered the text being searched.
        search({ top: true }),
        // Tab, Enter and the arrows in a table; outside a table they do nothing.
        tableEditing(),
        EditorState.phrases.of(PHRASES),
        // Search goes first: ⌘F and ⌘G must reach it, not the editor.
        keymap.of([...searchKeymap, ...formatKeymap, ...defaultKeymap, ...historyKeymap]),
        // GFM base, not CommonMark: the core renders tables, and the editor
        // parser must see them the same way.
        markdown({ base: markdownLanguage, codeLanguages }),
        livePreview(),
        agentNotes(),
        bookmarks(),
        blockSelection,
        syntaxHighlighting(highlightStyle),
        EditorView.lineWrapping,
        theme,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            options.onChange(update.state.doc.toString());
          }
          const toggled = bookmarkToggled(update);
          if (toggled) options.onBookmark?.(bookmarkLines(update.state), toggled.set);
          // Notes come from the syntax tree, which a long note builds in the
          // background after opening, so tree updates also trigger a report.
          if (update.docChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
            this.reportNotes();
          }
        }),
      ],
    });
  }

  /** Replaces the whole document; the undo history starts over. */
  setText(text: string): void {
    this.view.setState(this.createState(text));
    // The new state does not know live preview is off: that is a window
    // setting, not a document one.
    this.applyLive();
    // A state replacement does not call update listeners, so report notes by hand.
    this.reportNotes();
  }

  private reportNotes(): void {
    this.options.onNotes?.(notesOf(this.view.state));
  }

  revealNote(note: AgentNote): void {
    revealNote(this.view, note);
  }

  /**
   * Adds a note to words selected in the preview: match `index` of `total`.
   * Returns `false` if no single matching place exists in the source.
   */
  addNoteFromPreview(words: string, index: number, total: number, text: string): boolean {
    const to = previewAnchor(this.view.state, words, index, total);
    if (to === null) return false;
    insertNote(this.view, to, words, text);
    return true;
  }

  /** Lets the window return here when the note opens again. */
  getPosition(): EditorPosition {
    const { anchor, head } = this.view.state.selection.main;
    return { anchor, head, scroll: this.view.scrollSnapshot() };
  }

  /**
   * Restores the cursor and scroll.
   *
   * The file can change outside and get shorter, so positions are clamped:
   * CodeMirror clamps the scroll itself, the cursor is clamped here. Without a
   * position (the note was not opened before) the start stays, set by `setText`.
   */
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

  /** Bookmark lines in the current text, 1-based. */
  getBookmarks(): number[] {
    return bookmarkLines(this.view.state);
  }

  /**
   * Call after `setText`, which clears bookmarks.
   * `enabled` is false for separate files: they cannot have bookmarks.
   */
  setBookmarks(lines: number[], enabled: boolean): void {
    setBookmarkLines(this.view, lines, enabled);
  }

  toggleBookmark(): void {
    toggleBookmark(this.view);
  }

  /** `line` is 1-based. */
  toggleBookmarkLine(line: number): void {
    toggleBookmarkAt(this.view, line);
  }

  /** Puts the cursor at the line start and the line in the middle of the view. */
  goToLine(line: number): void {
    goToLine(this.view, line);
    this.view.focus();
  }

  getText(): string {
    return this.view.state.doc.toString();
  }

  focus(): void {
    this.view.focus();
  }

  /**
   * Turns live preview on or off.
   *
   * It uses an effect, not a new state: ⌘Z keeps working as before, and the
   * cursor stays where it was.
   */
  setLive(on: boolean): void {
    if (this.live === on) return;
    this.live = on;
    this.applyLive();
  }

  private applyLive(): void {
    this.view.dispatch({ effects: liveEnabled.of(this.live) });
  }

  /**
   * Opens note search and focuses its field.
   *
   * The window calls it too, not only ⌘F in the editor: searching the note
   * makes sense even when focus is, for example, in the note list.
   */
  openSearch(): void {
    openSearchPanel(this.view);
  }

  /**
   * Font size as a factor of the normal size.
   *
   * It uses a CSS variable, not a new theme: the theme is part of the state, and
   * replacing it would reset the undo history. Line heights need a new measure:
   * CodeMirror does not notice a font change itself.
   */
  setScale(scale: number): void {
    const dom = this.view.dom;
    if (scale === 1) dom.style.removeProperty("--note-scale");
    else dom.style.setProperty("--note-scale", String(scale));
    this.view.requestMeasure();
  }

  /** Redraws diagrams: mermaid colors come from its config, not from CSS. */
  redrawDiagrams(): void {
    this.view.dispatch({ effects: themeChanged.of(undefined) });
  }
}
