/**
 * Links in the editor text: ⌘-click (Ctrl-click) or the `link.follow` key
 * opens the note that a `[[link]]` or a `[text](note.md)` points to.
 *
 * A plain click stays an edit: it puts the cursor into the link, and the
 * markup shows. In Obsidian a plain click follows the link, and then you need
 * the arrow keys to change the link text. kknow keeps the click for text, as
 * VS Code does.
 *
 * The editor only finds the link under the cursor. The window resolves it in
 * the vault, because only the core knows the notes.
 *
 * The module imports only packages: the Node tests load it as is.
 */

import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

/** `[[target#sub|alias]]`; the grammar has no wikilinks, so a regex finds them. */
const WIKILINK = /!?\[\[([^\]\n]+)\]\]/g;

/** An image file: `![[shot.png]]` is a picture, not a note. */
const IMAGE = /\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i;

/** A URL with a scheme (`https:`, `mailto:`): not a note. */
const EXTERNAL = /^[a-z][a-z0-9+.-]*:/i;

/**
 * The link at `pos`: the target with its subpath, without the alias, as the
 * core reads it. `null` outside a link, in code, and for an external URL.
 */
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
    // An embedded image opens on a plain click; it is no note.
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

/** Is the "follow" modifier down: ⌘ on macOS, Ctrl elsewhere? */
function followKey(event: MouseEvent | KeyboardEvent, mac: boolean): boolean {
  return mac ? event.metaKey : event.ctrlKey;
}

/**
 * ⌘-click on a link calls `follow`. While ⌘ is down, the editor has the
 * `cm-follow` class: the links get a pointer, so the user sees what a click
 * does.
 */
export function followLinks(follow: (target: string) => void, mac: boolean): Extension {
  const mark = (view: EditorView, on: boolean) => view.dom.classList.toggle("cm-follow", on);
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      if (event.button !== 0 || !followKey(event, mac)) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      const target = pos === null ? null : linkAt(view.state, pos);
      if (!target) return false;
      event.preventDefault();
      follow(target);
      return true;
    },
    mousemove(event, view) {
      mark(view, followKey(event, mac));
      return false;
    },
    keydown(event, view) {
      mark(view, followKey(event, mac));
      return false;
    },
    keyup(event, view) {
      mark(view, followKey(event, mac));
      return false;
    },
    blur(_event, view) {
      mark(view, false);
      return false;
    },
  });
}
