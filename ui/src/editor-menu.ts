/**
 * The editor context menu: a right click in the text.
 *
 * It is the same menu as on the sidebar rows (`menu.ts`), with the shortcut
 * next to each item, so the user learns the keys from it. It has a comment
 * (an agent note), a bookmark and the clipboard. The note and the bookmark
 * work without a selection: the note then goes to the whole paragraph, the
 * bookmark to the line.
 *
 * Note editors of table cells have it too. There bookmarks are off: a cell
 * is not a line of the note.
 */

import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { noteTarget, openNoteForm, removeNote } from "./agent-notes";
import { bookmarksEnabled, hasBookmark, toggleBookmarkAt } from "./bookmarks";
import { t } from "./i18n";
import { openMenuAt, type MenuItem, type MenuSection } from "./menu";

/** Longer text in an item detail is cut: the menu stays narrow. */
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
        hint: "⌘⇧M",
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
      hint: "⌘⇧M",
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
      hint: "⌘⇧B",
      run: () => toggleBookmarkAt(view, line),
    },
  ];
}

/**
 * Cut, copy and paste through the clipboard API: the menu takes the focus
 * from the text, and the `execCommand` way needs it there.
 */
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
      hint: "⌘X",
      disabled: selected === "",
      run: () =>
        void copy()
          .then(() => view.dispatch(view.state.replaceSelection(""), { userEvent: "delete.cut" }))
          .catch(console.error),
    },
    {
      label: t("editor.menu.copy"),
      glyph: "copy",
      hint: "⌘C",
      disabled: selected === "",
      run: () => void copy().catch(console.error),
    },
    {
      label: t("editor.menu.paste"),
      hint: "⌘V",
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
  // A cell editor inside this one has its own menu, and a closed table cell
  // is a widget: its text selection belongs to the browser, not to us.
  const target = event.target as HTMLElement;
  if (event.defaultPrevented || target.closest(".cm-editor") !== view.dom) return false;
  const table = target.closest(".cm-table");
  if (table && view.dom.contains(table)) return false;

  // A right click outside the selection moves the cursor there first, as in
  // other editors: the menu is about the place under the pointer.
  const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
  const inside = view.state.selection.ranges.some(
    (range) => pos !== null && pos >= range.from && pos <= range.to && !range.empty,
  );
  if (pos !== null && !inside) view.dispatch({ selection: { anchor: pos } });

  event.preventDefault();
  const sections: MenuSection[] = [
    { items: [...noteItems(view), ...bookmarkItems(view)] },
    { items: clipboardItems(view) },
  ];
  openMenuAt(event.clientX, event.clientY, view.contentDOM, sections);
  return true;
}

export function editorMenu(): Extension {
  return EditorView.domEventHandlers({ contextmenu: open });
}
