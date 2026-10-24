/**
 * The Confluence page of a note in the editor.
 *
 * The comment `<!-- confluence: 123456789 -->` (`confluence-field.ts`) shows
 * as a chip with an icon and the page number. A click on the chip opens a
 * form, where the user changes the number or removes the comment. The editor
 * context menu (`editor-menu.ts`) opens the same form, also for a note
 * without a page: then the form adds the comment at the top of the note.
 *
 * The live preview `build` calls `addPageComment` on each comment node, as it
 * does for agent notes. The chip is not drawn while the selection is inside
 * the comment: then the user edits its text by hand.
 *
 * Only the main editor has this extension. A table cell is not a file, so the
 * cell editors have no page.
 */

import { syntaxTree } from "@codemirror/language";
import { StateEffect, StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, showTooltip, type Tooltip } from "@codemirror/view";

import { formKeys, matches } from "./actions";
import {
  cleanPageId,
  formatPageComment,
  pageCommentPlace,
  parsePageComment,
} from "./confluence-field";
import { t } from "./i18n";
import { icon } from "./icons";

/** A page comment in the text. */
export interface PageComment {
  from: number;
  to: number;
  pageId: string;
}

/** Comment nodes: inline (`Comment`) and on a line of their own (`CommentBlock`). */
function isComment(name: string): boolean {
  return name === "Comment" || name === "CommentBlock";
}

function pageAt(state: EditorState, from: number, to: number): PageComment | null {
  const pageId = parsePageComment(state.doc.sliceString(from, to));
  return pageId === null ? null : { from, to, pageId };
}

/**
 * The page comment of the note: the first one, as in the core, where the
 * upper comment wins. The syntax tree skips `<!-- … -->` in code blocks.
 */
export function pageCommentOf(state: EditorState): PageComment | null {
  let found: PageComment | null = null;
  syntaxTree(state).iterate({
    enter: (node) => {
      if (found) return false;
      if (isComment(node.name)) found = pageAt(state, node.from, node.to);
    },
  });
  return found;
}

// ——— Chip ———

class PageWidget extends WidgetType {
  constructor(readonly page: PageComment) {
    super();
  }

  eq(other: PageWidget): boolean {
    return other.page.pageId === this.page.pageId && other.page.from === this.page.from;
  }

  toDOM(view: EditorView): HTMLElement {
    const dom = document.createElement("span");
    dom.className = "cm-confluence";
    dom.title = t("editor.confluence.title");
    const name = document.createElement("span");
    name.className = "cm-confluence-name";
    name.textContent = "Confluence";
    const id = document.createElement("span");
    id.className = "cm-confluence-id";
    id.textContent = this.page.pageId === "" ? t("editor.confluence.empty") : this.page.pageId;
    dom.append(icon("confluence"), name, id);

    // A press must not move the cursor: a cursor move closes the form.
    dom.addEventListener("mousedown", (event) => {
      event.preventDefault();
      openPageForm(view);
    });
    return dom;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

/** Decorations for a comment node; nothing if it is not a page comment. */
export function addPageComment(
  state: EditorState,
  from: number,
  to: number,
  marks: Range<Decoration>[],
): void {
  if (!pagesEnabled(state)) return;
  const page = pageAt(state, from, to);
  if (!page) return;
  const inside = state.selection.ranges.some((range) => range.to > from && range.from < to);
  if (inside) return;
  marks.push(Decoration.replace({ widget: new PageWidget(page) }).range(from, to));
}

// ——— Form ———

/** Opens the form (`true`) or closes it (`false`). */
const setForm = StateEffect.define<boolean>();

const formField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setForm)) return effect.value ? tooltipFor(tr.state) : null;
    }
    // As with the note form: an edit or a cursor move means the user went on
    // with something else.
    if (value && (tr.docChanged || tr.selection)) return null;
    return value;
  },
  provide: (field) => showTooltip.from(field),
});

/** Whether this editor can have a page: the main one, not a table cell. */
export function pagesEnabled(state: EditorState): boolean {
  return state.field(formField, false) !== undefined;
}

function tooltipFor(state: EditorState): Tooltip {
  const page = pageCommentOf(state);
  return {
    pos: page?.from ?? pageCommentPlace(state.doc.toString()),
    create: (view) => {
      const dom = document.createElement("div");
      dom.className = "cm-agent-popup is-form cm-confluence-form";
      return { dom, mount: fillForm(view, dom, page) };
    },
  };
}

function button(label: string, run: () => void): HTMLButtonElement {
  const dom = document.createElement("button");
  dom.type = "button";
  dom.textContent = label;
  dom.addEventListener("mousedown", (event) => event.preventDefault());
  dom.addEventListener("click", run);
  return dom;
}

function fillForm(view: EditorView, dom: HTMLElement, page: PageComment | null): () => void {
  const label = document.createElement("label");
  label.className = "cm-confluence-label";
  label.append(icon("confluence"), t("editor.confluence.label"));

  const field = document.createElement("input");
  field.type = "text";
  field.className = "cm-agent-popup-field";
  field.placeholder = t("editor.confluence.placeholder", formKeys());
  field.spellcheck = false;
  if (page) field.value = page.pageId;
  label.htmlFor = field.id = "cm-confluence-field";

  const cancel = (): void => {
    view.dispatch({ effects: setForm.of(false) });
    view.focus();
  };
  // An emptied field means "remove the page", as in the note form.
  const save = (): void => {
    const pageId = cleanPageId(field.value);
    if (pageId === "") {
      if (page) removePage(view, page);
      else cancel();
      return;
    }
    setPage(view, page, pageId);
  };

  field.addEventListener("keydown", (event) => {
    if (matches(event, "submit") && !event.isComposing) {
      event.preventDefault();
      save();
    } else if (matches(event, "cancel")) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  });

  const actions = document.createElement("div");
  actions.className = "cm-agent-popup-actions";
  if (page) {
    const remove = button(t("editor.confluence.remove"), () => removePage(view, page));
    remove.classList.add("is-remove");
    remove.prepend(icon("trash"));
    actions.append(remove);
  }
  actions.append(button(t("editor.note.cancel"), cancel), button(t("editor.note.save"), save));
  actions.lastElementChild?.classList.add("is-primary");

  dom.append(label, field, actions);
  return () => {
    field.focus();
    field.select();
  };
}

/** Opens the form: the context menu and a click on the chip use it. */
export function openPageForm(view: EditorView): void {
  view.dispatch({ effects: setForm.of(true) });
}

/** Changes the page, or adds the comment at the top of the note. */
function setPage(view: EditorView, page: PageComment | null, pageId: string): void {
  const comment = formatPageComment(pageId);
  if (page) {
    view.dispatch({
      changes: { from: page.from, to: page.to, insert: comment },
      effects: setForm.of(false),
      userEvent: "input",
    });
  } else {
    const text = view.state.doc.toString();
    const at = pageCommentPlace(text);
    // Frontmatter at the very end of the file has no line break after it.
    const before = at > 0 && text[at - 1] !== "\n" ? "\n" : "";
    view.dispatch({
      changes: { from: at, insert: `${before}${comment}\n` },
      effects: setForm.of(false),
      userEvent: "input",
    });
  }
  view.focus();
}

/** Removes the comment together with its line, if the line has nothing else. */
export function removePage(view: EditorView, page: PageComment): void {
  const { doc } = view.state;
  const line = doc.lineAt(page.from);
  const alone = line.text.trim() === doc.sliceString(page.from, page.to).trim();
  const range =
    alone && line.to < doc.length ? { from: line.from, to: line.to + 1 } : { from: page.from, to: page.to };
  view.dispatch({ changes: range, effects: setForm.of(false), userEvent: "delete" });
  view.focus();
}

export function confluence(): Extension {
  return formField;
}
