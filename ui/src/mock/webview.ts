type DragDropEvent =
  | { type: "enter" | "over"; paths: string[] }
  | { type: "drop"; paths: string[]; position: { x: number; y: number } }
  | { type: "leave" };

type Handler = (event: { payload: DragDropEvent }) => void;

const handlers = new Set<Handler>();

function emit(payload: DragDropEvent): void {
  for (const handler of handlers) handler({ payload });
}

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
