import { Editor, type EditorOptions } from "../editor";
import { el } from "./dom";

export let editor: Editor;

export function initEditor(options: EditorOptions): void {
  editor = new Editor(el.editor, options);
}
