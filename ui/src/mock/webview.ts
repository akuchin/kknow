/**
 * Webview mock: the window needs only file drag and drop from it.
 *
 * In the app the shell sends the events, and they contain the path of the
 * dropped folder. In the browser no API gives the path, so the folder name
 * stands in for it. The `add_vault` mock accepts that, and you can still see
 * the drop hint and the new entry in the tree.
 */

/** The part of the real `onDragDropEvent` payload that the window uses. */
type DragDropEvent =
  | { type: "enter" | "over"; paths: string[] }
  | { type: "drop"; paths: string[]; position: { x: number; y: number } }
  | { type: "leave" };

type Handler = (event: { payload: DragDropEvent }) => void;

const handlers = new Set<Handler>();

function emit(payload: DragDropEvent): void {
  for (const handler of handlers) handler({ payload });
}

// `dragover` fires repeatedly while the cursor is over the window. For the
// subscriber it is always the same state: "something is dragged over".
window.addEventListener("dragover", (event) => {
  event.preventDefault();
  emit({ type: "over", paths: [] });
});

window.addEventListener("dragleave", () => emit({ type: "leave" }));

window.addEventListener("drop", (event) => {
  event.preventDefault();
  const names = [...(event.dataTransfer?.items ?? [])]
    .map((item) => item.webkitGetAsEntry()?.name ?? item.getAsFile()?.name)
    .filter((name): name is string => Boolean(name));

  // The shell gives the point in physical pixels, so the mock does too.
  const scale = window.devicePixelRatio || 1;
  const position = { x: event.clientX * scale, y: event.clientY * scale };
  emit({ type: "drop", paths: names.map((name) => `~/${name}`), position });
});

export function getCurrentWebview() {
  return {
    onDragDropEvent(handler: Handler): Promise<() => void> {
      handlers.add(handler);
      return Promise.resolve(() => handlers.delete(handler));
    },
  };
}
