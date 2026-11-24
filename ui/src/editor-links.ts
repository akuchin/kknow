import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

const WIKILINK = /!?\[\[([^\]\n]+)\]\]/g;

const IMAGE = /\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i;

const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;

export function linkAt(state: EditorState, pos: number): string | null {
  const tree = syntaxTree(state);
  type Node = ReturnType<typeof tree.resolveInner>;
  let link: Node | null = null;
  for (let node: Node | null = tree.resolveInner(pos, 1); node; node = node.parent) {
    if (/Code/.test(node.name)) return null;
    if (node.name === "Link" && !link) link = node;
  }

  const line = state.doc.lineAt(pos);
  for (const match of line.text.matchAll(WIKILINK)) {
    const from = line.from + (match.index ?? 0);
    const to = from + match[0].length;
    if (pos < from || pos > to) continue;
    const target = match[1].split("|")[0].trim();
    if (match[0].startsWith("!") && IMAGE.test(target)) return null;
    return target || null;
  }

  const url = link?.getChild("URL");
  if (!url) return null;
  const target = state.sliceDoc(url.from, url.to).replace(/^<|>$/g, "");
  if (!target || EXTERNAL.test(target)) return null;
  try {
    return decodeURI(target);
  } catch {
    return target;
  }
}

function followKey(event: MouseEvent | KeyboardEvent, mac: boolean): boolean {
  return mac ? event.metaKey : event.ctrlKey;
}

export function followLinks(
  follow: (target: string) => void,
  mac: boolean,
  peek?: (target: string | null, x: number, y: number) => void,
): Extension {
  const mark = (view: EditorView, on: boolean) => view.dom.classList.toggle("cm-follow", on);
  let pointer: { x: number; y: number } | null = null;
  const report = (view: EditorView, on: boolean) => {
    if (!peek) return;
    if (!on || !pointer) return peek(null, 0, 0);
    const pos = view.posAtCoords(pointer);
    peek(pos === null ? null : linkAt(view.state, pos), pointer.x, pointer.y);
  };
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      if (event.button !== 0 || !followKey(event, mac)) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      const target = pos === null ? null : linkAt(view.state, pos);
      if (!target) return false;
      event.preventDefault();
      peek?.(null, 0, 0);
      follow(target);
      return true;
    },
    mousemove(event, view) {
      pointer = { x: event.clientX, y: event.clientY };
      mark(view, followKey(event, mac));
      report(view, followKey(event, mac));
      return false;
    },
    mouseleave(_event, view) {
      pointer = null;
      report(view, false);
      return false;
    },
    keydown(event, view) {
      mark(view, followKey(event, mac));
      if (event.key === "Meta" || event.key === "Control") report(view, followKey(event, mac));
      return false;
    },
    keyup(event, view) {
      mark(view, followKey(event, mac));
      if (event.key === "Meta" || event.key === "Control") report(view, followKey(event, mac));
      return false;
    },
    blur(_event, view) {
      mark(view, false);
      return false;
    },
  });
}
