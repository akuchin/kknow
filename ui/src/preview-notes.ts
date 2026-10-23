/**
 * Agent notes from the preview, in "Read" and "Side by side" modes.
 *
 * The preview is HTML from the core and is not editable, so this is only a
 * popup: select words, get a "For agent" button, and it opens a task form. The
 * note goes into the source through the editor (`addNoteFromPreview`), then to
 * the file and back to the preview, like any edit.
 *
 * Next to "For agent" is "Bookmark": its source line comes from the line marks
 * that the core puts into the HTML (`preview-lines.ts`).
 *
 * The preview does not know where the words are in the source. It passes the
 * words and their index among equal matches in the visible preview text.
 * Matches in code count neither here nor in the source: a note cannot go into code.
 */

import { icon } from "./icons";
import { t } from "./i18n";
import { lineAt } from "./preview-lines";

export interface PreviewNotesOptions {
  /** Notes are allowed only while the preview is visible. */
  enabled: () => boolean;
  /**
   * The element whose text the match index counts, for a selection in `node`;
   * `null` means no popup there. Without it, the whole root counts.
   */
  scope?: (node: Node) => Element | null;
  /** Returns `false` if no single matching place exists in the source. `within` is the scope. */
  add: (words: string, index: number, total: number, text: string, within: Element) => boolean;
  /** Reports that the note was not added. */
  fail: (message: string) => void;
  /** Separate files cannot have bookmarks. */
  bookmarkable: () => boolean;
  /** Toggles a bookmark on a source line. */
  bookmark: (line: number) => void;
}

/** Preview words and their index among equal matches: the source lookup uses both. */
interface PreviewWords {
  words: string;
  index: number;
  total: number;
}

/** Selected words with their range: the popup sticks to it. */
interface Picked extends PreviewWords {
  range: Range;
  within: Element;
}


/** Text where a note is useless or impossible: code and diagrams. */
const SKIPPED = "pre, code, svg";

export class PreviewNotes {
  private readonly dom = document.createElement("div");
  private picked: Picked | null = null;
  private form = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly options: PreviewNotesOptions,
  ) {
    this.dom.className = "cm-agent-popup preview-note-popup";
    this.dom.hidden = true;
    // A click inside the popup must not clear the preview selection.
    this.dom.addEventListener("mousedown", (event) => {
      if (!(event.target instanceof HTMLTextAreaElement)) event.preventDefault();
    });
    document.body.append(this.dom);

    document.addEventListener("selectionchange", () => {
      // An open form is left alone: focusing it clears the selection anyway.
      if (!this.form) this.update();
    });
    root.addEventListener("scroll", () => this.place());
    window.addEventListener("resize", () => this.place());
  }

  /** ⌘⇧M: opens the form if the button is shown over the selection. */
  openForm(): boolean {
    if (!this.picked || this.form) return false;
    this.showForm();
    return true;
  }

  /** Called when the note or the mode changes. */
  hide(): void {
    this.picked = null;
    this.form = false;
    this.dom.hidden = true;
    this.dom.replaceChildren();
  }

  private update(): void {
    const picked = this.options.enabled() ? this.pick() : null;
    if (!picked) {
      this.hide();
      return;
    }
    this.picked = picked;
    this.dom.classList.remove("is-form");
    this.dom.replaceChildren(
      button(t("editor.note.add"), t("editor.note.add.title"), () => this.showForm(), "comment"),
    );
    if (this.options.bookmarkable()) {
      this.dom.append(
        button(t("bookmark.toggle"), t("bookmark.toggle.title"), () => this.bookmark(), "bookmark"),
      );
    }
    this.dom.hidden = false;
    this.place();
  }

  /** The "Bookmark" popup button: uses the line where the selection starts. */
  private bookmark(): void {
    const picked = this.picked;
    if (!picked) return;
    const line = lineAt(this.root, picked.range.startContainer, picked.range.startOffset);
    if (line === null) {
      this.options.fail(t("status.bookmarkNowhere"));
      return;
    }
    this.options.bookmark(line);
    this.hide();
    document.getSelection()?.removeAllRanges();
  }

  /**
   * The selection line in the preview, for ⌘⇧B in "Read" mode. Any selection
   * works here, not only one with the popup: even one across several paragraphs.
   */
  selectedLine(): number | null {
    const selection = document.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
    const range = selection.getRangeAt(0);
    if (!this.root.contains(range.startContainer)) return null;
    return lineAt(this.root, range.startContainer, range.startOffset);
  }

  /**
   * The preview selection: the words and their index among equal matches.
   *
   * The index uses the joined visible text: a word highlighted by a note is
   * split into several text nodes, but in the source it is continuous.
   */
  private pick(): Picked | null {
    const selection = document.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
    const range = selection.getRangeAt(0);
    if (!this.root.contains(range.commonAncestorContainer)) return null;
    const { scope } = this.options;
    const within = scope ? scope(range.commonAncestorContainer) : this.root;
    if (!within) return null;

    const words = range.toString();
    // The anchor is one line without `»`, as in the editor.
    if (words.trim() === "" || /[\n»]/.test(words)) return null;

    const found = this.count(within, range.startContainer, range.startOffset, words);
    return found && { ...found, range: range.cloneRange(), within };
  }

  /**
   * The index of `words` that start in `node` at `offset`, among equal matches
   * in the visible text of `within`. `null` if the start or the words touch
   * code: notes and bookmarks do not belong there.
   */
  private count(within: Element, node: Node, offset: number, words: string): PreviewWords | null {
    let text = "";
    let start = -1;
    const walker = document.createTreeWalker(within, NodeFilter.SHOW_TEXT);
    for (let each = walker.nextNode(); each; each = walker.nextNode()) {
      if (each.parentElement?.closest(SKIPPED)) continue;
      if (each === node) start = text.length + offset;
      text += each.textContent ?? "";
    }
    if (start === -1 || !text.startsWith(words, start)) return null;

    let index = 0;
    let total = 0;
    for (let at = text.indexOf(words); at !== -1; at = text.indexOf(words, at + 1)) {
      if (at < start) index += 1;
      total += 1;
    }
    return { words, index, total };
  }

  private showForm(): void {
    const picked = this.picked;
    if (!picked) return;
    this.form = true;
    this.dom.classList.add("is-form");

    const field = document.createElement("textarea");
    field.className = "cm-agent-popup-field";
    field.rows = 2;
    field.placeholder = t("editor.note.placeholder", { words: picked.words });

    const save = (): void => {
      if (field.value.trim() === "") {
        this.hide();
        return;
      }
      const added = this.options.add(
        picked.words,
        picked.index,
        picked.total,
        field.value,
        picked.within,
      );
      if (!added) {
        // Keep the form open so the typed task is not lost.
        this.options.fail(t("preview.note.ambiguous", { words: picked.words }));
        return;
      }
      this.hide();
      document.getSelection()?.removeAllRanges();
    };

    field.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        save();
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        this.hide();
      }
    });

    const actions = document.createElement("div");
    actions.className = "cm-agent-popup-actions";
    actions.append(
      button(t("editor.note.cancel"), t("editor.note.cancel"), () => this.hide()),
      button(t("editor.note.save"), t("editor.note.save"), save),
    );
    actions.lastElementChild?.classList.add("is-primary");

    this.dom.replaceChildren(field, actions);
    this.place();
    field.focus();
  }

  /** Above the selection start, as in the editor, and inside the window. */
  private place(): void {
    if (this.dom.hidden || !this.picked) return;
    const rects = this.picked.range.getClientRects();
    const rect = rects[0] ?? this.picked.range.getBoundingClientRect();
    const pane = this.root.getBoundingClientRect();
    // The words scrolled out of view: the popup has nothing to stick to.
    if (!this.form && (rect.bottom < pane.top || rect.top > pane.bottom)) {
      this.dom.style.visibility = "hidden";
      return;
    }
    this.dom.style.visibility = "";
    const { width, height } = this.dom.getBoundingClientRect();
    const left = Math.min(Math.max(8, rect.left), window.innerWidth - width - 8);
    const above = rect.top - height - 6;
    const top = above >= 8 ? above : rect.bottom + 6;
    this.dom.style.left = `${left}px`;
    this.dom.style.top = `${top}px`;
  }
}

function button(
  label: string,
  title: string,
  run: () => void,
  iconName?: "comment" | "bookmark",
): HTMLButtonElement {
  const dom = document.createElement("button");
  dom.type = "button";
  dom.title = title;
  if (iconName) dom.append(icon(iconName));
  if (label) dom.append(label);
  dom.addEventListener("click", run);
  return dom;
}
