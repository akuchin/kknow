import { icon } from "./icons";
import { formKeys, matches, withShortcut } from "./actions";
import { t } from "./i18n";
import { lineAt } from "./preview-lines";

export interface PreviewNotesOptions {
  enabled: () => boolean;
  scope?: (node: Node) => Element | null;
  add: (words: string, index: number, total: number, text: string, within: Element) => boolean;
  fail: (message: string) => void;
  bookmarkable: () => boolean;
  bookmark: (line: number) => void;
}

interface PreviewWords {
  words: string;
  index: number;
  total: number;
}

interface Picked extends PreviewWords {
  range: Range;
  within: Element;
}


const SKIPPED = "pre, code, svg, .embed";

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
    this.dom.addEventListener("mousedown", (event) => {
      if (!(event.target instanceof HTMLTextAreaElement)) event.preventDefault();
    });
    document.body.append(this.dom);

    document.addEventListener("selectionchange", () => {
      if (!this.form) this.update();
    });
    root.addEventListener("scroll", () => this.place());
    window.addEventListener("resize", () => this.place());
  }

  openForm(): boolean {
    if (!this.picked || this.form) return false;
    this.showForm();
    return true;
  }

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
      button(t("editor.note.add"), withShortcut(t("editor.note.add.title"), "note.comment"), () => this.showForm(), "comment"),
    );
    if (this.options.bookmarkable()) {
      this.dom.append(
        button(
          t("bookmark.toggle"),
          withShortcut(t("bookmark.toggle.title"), "bookmark.toggle"), () => this.bookmark(), "bookmark"),
      );
    }
    this.dom.hidden = false;
    this.place();
  }

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

  selectedLine(): number | null {
    const selection = document.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
    const range = selection.getRangeAt(0);
    if (!this.root.contains(range.startContainer)) return null;
    return lineAt(this.root, range.startContainer, range.startOffset);
  }

  private pick(): Picked | null {
    const selection = document.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
    const range = selection.getRangeAt(0);
    if (!this.root.contains(range.commonAncestorContainer)) return null;
    const { scope } = this.options;
    const within = scope ? scope(range.commonAncestorContainer) : this.root;
    if (!within) return null;

    const words = range.toString();
    if (words.trim() === "" || /[\n»]/.test(words)) return null;

    const found = this.count(within, range.startContainer, range.startOffset, words);
    return found && { ...found, range: range.cloneRange(), within };
  }

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
    field.placeholder = t("editor.note.placeholder", { words: picked.words, ...formKeys() });

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
        this.options.fail(t("preview.note.ambiguous", { words: picked.words }));
        return;
      }
      this.hide();
      document.getSelection()?.removeAllRanges();
    };

    field.addEventListener("keydown", (event) => {
      if (matches(event, "submit") && !event.isComposing) {
        event.preventDefault();
        save();
      } else if (matches(event, "cancel")) {
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

  private place(): void {
    if (this.dom.hidden || !this.picked) return;
    const rects = this.picked.range.getClientRects();
    const rect = rects[0] ?? this.picked.range.getBoundingClientRect();
    const pane = this.root.getBoundingClientRect();
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
