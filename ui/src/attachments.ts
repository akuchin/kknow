import { EditorSelection, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { imageLink, pastedImageName } from "./attachment-name";

export type SaveImage = (name: string, bytes: Uint8Array) => Promise<string | null>;

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
