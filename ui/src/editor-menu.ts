import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { noteTarget, openNoteForm, removeNote } from "./agent-notes";
import { openPageForm, pageCommentOf, pagesEnabled, removePage } from "./confluence";
import { bookmarksEnabled, hasBookmark, toggleBookmarkAt } from "./bookmarks";
import { t } from "./i18n";
import { openMenuAt, type MenuItem, type MenuSection } from "./menu";

const DETAIL_LENGTH = 40;

function shorten(text: string): string {
  return text.length > DETAIL_LENGTH ? `${text.slice(0, DETAIL_LENGTH)}…` : text;
}

function noteItems(view: EditorView): MenuItem[] {
  const target = noteTarget(view.state);
  if (target?.kind === "edit") {
    return [
      {
        label: t("editor.menu.note.edit"),
        detail: shorten(target.note.text),
        glyph: "comment",
        action: "note.comment",
        run: () => openNoteForm(view, target),
      },
      {
        label: t("editor.menu.note.remove"),
        glyph: "trash",
        run: () => removeNote(view, target.note),
      },
    ];
  }

  let detail = t("editor.menu.note.none");
  if (target) {
    detail =
      target.words === ""
        ? t("editor.menu.note.block")
        : t("editor.menu.note.words", { words: shorten(target.words) });
  }
  return [
    {
      label: t("editor.menu.note.add"),
      detail,
      glyph: "comment",
      action: "note.comment",
      disabled: !target,
      run: () => target && openNoteForm(view, target),
    },
  ];
}

function bookmarkItems(view: EditorView): MenuItem[] {
  const { state } = view;
  if (!bookmarksEnabled(state)) return [];
  const line = state.doc.lineAt(state.selection.main.head).number;
  return [
    {
      label: t(hasBookmark(state, line) ? "bookmark.remove" : "bookmark.add"),
      glyph: "bookmark",
      action: "bookmark.toggle",
      run: () => toggleBookmarkAt(view, line),
    },
  ];
}

function pageItems(view: EditorView): MenuItem[] {
  if (!pagesEnabled(view.state)) return [];
  const page = pageCommentOf(view.state);
  if (!page) {
    return [
      {
        label: t("editor.menu.confluence.add"),
        detail: t("editor.menu.confluence.add.detail"),
        glyph: "confluence",
        run: () => openPageForm(view),
      },
    ];
  }
  return [
    {
      label: t("editor.menu.confluence.edit"),
      detail: page.pageId,
      glyph: "confluence",
      run: () => openPageForm(view),
    },
    {
      label: t("editor.menu.confluence.remove"),
      glyph: "trash",
      run: () => removePage(view, page),
    },
  ];
}

function clipboardItems(view: EditorView): MenuItem[] {
  const { state } = view;
  const selected = state.selection.ranges
    .filter((range) => !range.empty)
    .map((range) => state.sliceDoc(range.from, range.to))
    .join(state.lineBreak);
  const copy = (): Promise<void> => navigator.clipboard.writeText(selected);

  return [
    {
      label: t("editor.menu.cut"),
      action: "edit.cut",
      disabled: selected === "",
      run: () =>
        void copy()
          .then(() => view.dispatch(view.state.replaceSelection(""), { userEvent: "delete.cut" }))
          .catch(console.error),
    },
    {
      label: t("editor.menu.copy"),
      glyph: "copy",
      action: "edit.copy",
      disabled: selected === "",
      run: () => void copy().catch(console.error),
    },
    {
      label: t("editor.menu.paste"),
      action: "edit.paste",
      run: () =>
        void navigator.clipboard
          .readText()
          .then((text) =>
            view.dispatch(view.state.replaceSelection(text), { userEvent: "input.paste" }),
          )
          .catch(console.error),
    },
  ];
}

function open(event: MouseEvent, view: EditorView): boolean {
  const target = event.target as HTMLElement;
  if (event.defaultPrevented || target.closest(".cm-editor") !== view.dom) return false;
  const table = target.closest(".cm-table");
  if (table && view.dom.contains(table)) return false;

  const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
  const inside = view.state.selection.ranges.some(
    (range) => pos !== null && pos >= range.from && pos <= range.to && !range.empty,
  );
  if (pos !== null && !inside) view.dispatch({ selection: { anchor: pos } });

  event.preventDefault();
  const sections: MenuSection[] = [
    { items: [...noteItems(view), ...bookmarkItems(view)] },
    { items: pageItems(view) },
    { items: clipboardItems(view) },
  ];
  openMenuAt(event.clientX, event.clientY, view.contentDOM, sections);
  return true;
}

export function editorMenu(): Extension {
  return EditorView.domEventHandlers({ contextmenu: open });
}
