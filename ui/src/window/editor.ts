// The editor of the main window: one CodeMirror view for the open note.
//
// Almost every zone reads or changes the editor text, so the instance lives in
// a low layer, like the status bar. Its handlers live in higher layers, so
// `main.ts` passes them to `initEditor`. The module does not touch the DOM on
// import.

import { Editor, type EditorOptions } from "../editor";
import { el } from "./dom";

/** The editor of the open note or separate file. */
export let editor: Editor;

export function initEditor(options: EditorOptions): void {
  editor = new Editor(el.editor, options);
}
