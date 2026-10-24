/**
 * The bookmark marker as a control: a hover shows the bookmark name, a right
 * click opens a small menu to name, rename or remove the bookmark.
 *
 * The handlers sit on the editor root and look at the event target
 * (delegation): the marker is a gutter element (`.cm-bookmark-gutter`), and
 * the gutter builds its elements again on updates. The line comes from the
 * pointer height, which is the same for the text and for a gutter.
 *
 * The right click listener captures the event on the root. So it runs before
 * other context menu handlers in the editor, and a click on a marker does not
 * also open them.
 */

import type { Extension } from "@codemirror/state";
import { EditorView, ViewPlugin } from "@codemirror/view";

import { bookmarkName, hasBookmark, nameBookmarkAt, toggleBookmarkAt } from "./bookmarks";
import { matches } from "./actions";
import { t } from "./i18n";
import { openMenu } from "./menu";

const GUTTER_MARKER = ".cm-bookmark-gutter";

/** The bookmarked line under the pointer, if the pointer is on its marker. */
function markerLine(view: EditorView, event: MouseEvent): number | null {
  const target = event.target instanceof Element ? event.target : null;
  if (!target?.closest(GUTTER_MARKER)) return null;
  const block = view.lineBlockAtHeight(event.clientY - view.documentTop);
  const line = view.state.doc.lineAt(block.from).number;
  return hasBookmark(view.state, line) ? line : null;
}

/**
 * The menu module places a menu at a button. A right click has no button, so
 * an empty element at the pointer stands in for it. When the menu closes, it
 * gives focus back to its anchor; the anchor passes it on to the editor.
 */
let anchor: HTMLElement | null = null;
let anchorView: EditorView | null = null;

function pointAnchor(view: EditorView, x: number, y: number): HTMLElement {
  if (!anchor) {
    anchor = document.createElement("div");
    anchor.className = "point-anchor";
    anchor.tabIndex = -1;
    anchor.addEventListener("focus", () => anchorView?.focus());
    document.body.append(anchor);
  }
  anchorView = view;
  anchor.style.left = `${x}px`;
  anchor.style.top = `${y}px`;
  return anchor;
}

function openBookmarkMenu(view: EditorView, line: number, x: number, y: number): void {
  const name = bookmarkName(view.state, line);
  openMenu(pointAnchor(view, x, y), [
    {
      title: name ?? t("bookmark.menu.title", { line }),
      items: [
        {
          label: t(name ? "bookmark.menu.rename" : "bookmark.menu.name"),
          glyph: "pencil",
          key: "R",
          run: () => editName(view, line, x, y),
        },
        ...(name
          ? [
              {
                label: t("bookmark.menu.unname"),
                key: "U",
                run: () => {
                  nameBookmarkAt(view, line, "");
                  view.focus();
                },
              },
            ]
          : []),
      ],
    },
    {
      items: [
        {
          label: t("bookmark.remove"),
          glyph: "trash",
          tone: "danger",
          key: "⌫",
          run: () => {
            toggleBookmarkAt(view, line);
            view.focus();
          },
        },
      ],
    },
  ]);
}

/**
 * A field at the pointer for the bookmark name. Enter and a blur save, Esc
 * cancels, as the rename field in the sidebar does.
 */
function editName(view: EditorView, line: number, x: number, y: number): void {
  const box = document.createElement("div");
  box.className = "menu bookmark-name";
  const field = document.createElement("input");
  field.className = "bookmark-name-field";
  field.value = bookmarkName(view.state, line) ?? "";
  field.placeholder = t("bookmark.name.placeholder");
  field.setAttribute("aria-label", t("bookmark.name.placeholder"));
  box.append(field);
  document.body.append(box);

  const margin = 6;
  const width = box.getBoundingClientRect().width;
  box.style.left = `${Math.max(margin, Math.min(x, window.innerWidth - width - margin))}px`;
  box.style.top = `${y + 4}px`;
  field.focus();
  field.select();

  let done = false;
  const finish = (saving: boolean): void => {
    if (done) return;
    done = true;
    box.remove();
    if (saving) nameBookmarkAt(view, line, field.value);
    view.focus();
  };
  field.onkeydown = (event) => {
    if (matches(event, "submit")) {
      event.preventDefault();
      finish(true);
    } else if (matches(event, "cancel")) {
      event.preventDefault();
      finish(false);
    }
  };
  field.onblur = () => finish(true);
}

const markerControls = ViewPlugin.define((view) => {
  /**
   * The title is on the editor root. The gutter builds its markers again on
   * updates, so a title on the marker element would not keep the name.
   */
  let title: string | null = null;
  const showTitle = (next: string | null): void => {
    if (next === title) return;
    title = next;
    if (next === null) view.dom.removeAttribute("title");
    else view.dom.title = next;
  };

  const onMove = (event: MouseEvent): void => {
    const line = markerLine(view, event);
    showTitle(
      line === null ? null : (bookmarkName(view.state, line) ?? t("bookmark.marker.title")),
    );
  };
  const onLeave = (): void => showTitle(null);
  const onContextMenu = (event: MouseEvent): void => {
    const line = markerLine(view, event);
    if (line === null) return;
    event.preventDefault();
    event.stopPropagation();
    showTitle(null);
    openBookmarkMenu(view, line, event.clientX, event.clientY);
  };

  view.dom.addEventListener("mousemove", onMove);
  view.dom.addEventListener("mouseleave", onLeave);
  view.dom.addEventListener("contextmenu", onContextMenu, true);
  return {
    destroy() {
      view.dom.removeEventListener("mousemove", onMove);
      view.dom.removeEventListener("mouseleave", onLeave);
      view.dom.removeEventListener("contextmenu", onContextMenu, true);
    },
  };
});

export function bookmarkMenu(): Extension {
  return markerControls;
}
