/**
 * Images pasted or dropped into the editor.
 *
 * The editor does not write files. It gives the bytes to the window, and the
 * core saves them into `attachments/`. Then the editor inserts
 * `![[attachments/name.png]]`. The renderer and the image loader resolve this
 * form, also with spaces in the name.
 *
 * A dropped file does not come here: the shell catches the drop, not the
 * page (`onDragDrop` in `api.ts`). The window inserts its link through
 * `insertImageLinks`.
 */

import { EditorSelection, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { imageLink, pastedImageName } from "./attachment-name";

/** Saves the image and returns its path, or `null`; the window shows why. */
export type SaveImage = (name: string, bytes: Uint8Array) => Promise<string | null>;

/**
 * Catches a paste with images. A paste that also has plain text stays text:
 * a spreadsheet puts both a picture and the cells into the clipboard.
 *
 * A file drop that reaches the page is not for CodeMirror either: it would
 * insert the file bytes as text. The window handles dropped files.
 */
export function editorImages(save: SaveImage): Extension {
  return EditorView.domEventHandlers({
    drop(event) {
      return (event.dataTransfer?.files.length ?? 0) > 0;
    },
    paste(event, view) {
      const data = event.clipboardData;
      if (!data || data.getData("text/plain")) return false;
      const images = [...data.files].filter((file) => pastedImageName(file.type, new Date()));
      if (images.length === 0) return false;
      event.preventDefault();
      void (async () => {
        const links: string[] = [];
        for (const image of images) {
          const name = pastedImageName(image.type, new Date()) ?? "";
          const path = await save(name, new Uint8Array(await image.arrayBuffer()));
          if (path) links.push(imageLink(path));
        }
        insertImageLinks(view, links);
      })();
      return true;
    },
  });
}

/**
 * Inserts the links at `at`, or in place of the selection. Several images go
 * one per line. The cursor goes after the last link.
 */
export function insertImageLinks(view: EditorView, links: string[], at?: number): void {
  if (links.length === 0) return;
  const text = links.join("\n");
  const { from, to } =
    at === undefined
      ? view.state.selection.main
      : EditorSelection.cursor(Math.min(at, view.state.doc.length));
  view.dispatch({
    changes: { from, to, insert: text },
    selection: EditorSelection.cursor(from + text.length),
    scrollIntoView: true,
    userEvent: at === undefined ? "input.paste" : "input.drop",
  });
  view.focus();
}
