/**
 * Completion of links and tags while you type.
 *
 * - `[[` lists the notes of the vault. The pick inserts the shortest text
 *   that finds the note: the file name if it is unique, else the full path.
 * - `[[note#` lists the headings of that note, `[[note#^` its block IDs.
 *   `[[#` and `[[#^` are about the open note: the editor has its text, also
 *   with unsaved edits.
 * - `#ta` lists the tags of the vault.
 *
 * Only the vault knows the notes, so the window gives the data
 * (`LinkSource`). A standalone file gets an empty list.
 *
 * The module imports only packages: the Node tests load it as is.
 */

import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

/** A note as the completion needs it. */
export interface LinkNote {
  id: string;
  title: string;
  folder: string | null;
  tags: readonly string[];
}

/** The anchors of a note that a link can point to. */
export interface LinkAnchors {
  headings: { level: number; text: string }[];
  blocks: string[];
}

export interface LinkSource {
  /** The notes of the vault; empty for a standalone file. */
  notes(): readonly LinkNote[];
  /** Headings and blocks of the note that `target` finds; `null` if none. */
  anchors(target: string): Promise<LinkAnchors | null>;
  /** Headings of the open text, for `[[#`: the editor outline gives them. */
  headings(state: EditorState): LinkAnchors["headings"];
}

/** What the text before the cursor asks for. `from` is where the query starts. */
export type LinkQuery =
  | { kind: "note"; from: number; query: string }
  | { kind: "heading"; from: number; target: string; query: string }
  | { kind: "block"; from: number; target: string; query: string }
  | { kind: "tag"; from: number; query: string };

/** A tag: letters, digits, `_`, `-` and `/` for nested tags, as the core reads it. */
const TAG = /(?:^|[\s(])#([\p{L}\p{N}_/-]+)$/u;

/**
 * Reads the query from the line text before the cursor. `start` is the
 * document position of the line start.
 */
export function linkQuery(before: string, start: number): LinkQuery | null {
  const open = before.lastIndexOf("[[");
  if (open >= 0 && !before.includes("]]", open)) {
    const inner = before.slice(open + 2);
    // After the pipe comes the alias: free text, nothing to complete.
    if (inner.includes("|") || inner.includes("[")) return null;
    const at = start + open + 2;
    const hash = inner.indexOf("#");
    if (hash < 0) return { kind: "note", from: at, query: inner };
    const target = inner.slice(0, hash);
    const rest = inner.slice(hash + 1);
    if (rest.startsWith("^")) {
      return { kind: "block", from: at + hash + 2, target, query: rest.slice(1) };
    }
    // `[[note#Part#Section]]`: complete the last part.
    const last = inner.lastIndexOf("#");
    return { kind: "heading", from: at + last + 1, target, query: inner.slice(last + 1) };
  }
  const tag = TAG.exec(before);
  if (tag) return { kind: "tag", from: start + before.length - tag[1].length, query: tag[1] };
  return null;
}

/** The file name of a note ID. */
function nameOf(id: string): string {
  return id.slice(id.lastIndexOf("/") + 1);
}

/**
 * The text of a link to `id`: the file name if no other note has it, else
 * the full path. The core resolves both the same way.
 */
export function linkText(id: string, notes: readonly LinkNote[]): string {
  const name = nameOf(id).toLowerCase();
  const same = notes.filter((note) => nameOf(note.id).toLowerCase() === name).length;
  return same > 1 ? id : nameOf(id);
}

/** Block IDs in the text, as the core finds them: `^id` at a line end. */
export function blocksOf(text: string): string[] {
  return [...text.matchAll(/(?:^|[ \t])\^([A-Za-z0-9-]+)[ \t]*$/gm)].map((found) => found[1]);
}

/** Is the position in code? There `[[` and `#` are plain text. */
function inCode(state: EditorState, pos: number): boolean {
  type Node = ReturnType<typeof syntaxTree>["topNode"];
  for (let node: Node | null = syntaxTree(state).resolveInner(pos, -1); node; node = node.parent) {
    if (/Code|Comment|HTML/.test(node.name)) return true;
  }
  return false;
}

/**
 * An option that closes the link: `]]` goes after the text unless it is
 * already there. The cursor stays after the brackets.
 */
function closing(label: string, insert: string, extra: Partial<Completion> = {}): Completion {
  return {
    label,
    ...extra,
    apply: (view, _completion, from, to) => {
      const after = view.state.sliceDoc(to, to + 2);
      const text = after === "]]" ? insert : `${insert}]]`;
      view.dispatch({
        changes: { from, to, insert: text },
        selection: { anchor: from + insert.length + 2 },
        userEvent: "input.complete",
      });
    },
  };
}

/** The completion source of the editor. */
export function linkCompletion(source: LinkSource): CompletionSource {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const { state, pos } = context;
    const line = state.doc.lineAt(pos);
    const query = linkQuery(line.text.slice(0, pos - line.from), line.from);
    if (!query || inCode(state, pos)) return null;

    switch (query.kind) {
      case "note": {
        const notes = source.notes();
        if (notes.length === 0) return null;
        return {
          from: query.from,
          options: notes.map((note) =>
            closing(nameOf(note.id), linkText(note.id, notes), {
              detail: note.folder ?? undefined,
              // The title helps when the file name says little.
              info: note.title !== nameOf(note.id) ? note.title : undefined,
              type: "note",
            }),
          ),
          validFor: /^[^[\]|#^\n]*$/,
        };
      }
      case "heading": {
        const anchors = query.target.trim()
          ? await source.anchors(query.target)
          : { headings: source.headings(state), blocks: [] };
        if (!anchors || anchors.headings.length === 0 || context.aborted) return null;
        return {
          from: query.from,
          options: anchors.headings.map((heading) =>
            closing(heading.text, heading.text, { detail: "#".repeat(heading.level), type: "heading" }),
          ),
          validFor: /^[^[\]|#\n]*$/,
        };
      }
      case "block": {
        const blocks = query.target.trim()
          ? (await source.anchors(query.target))?.blocks
          : blocksOf(state.doc.toString());
        if (!blocks || blocks.length === 0 || context.aborted) return null;
        return {
          from: query.from,
          options: blocks.map((id) => closing(id, id, { type: "block" })),
          validFor: /^[A-Za-z0-9-]*$/,
        };
      }
      case "tag": {
        const tags = new Set(source.notes().flatMap((note) => note.tags));
        // The tag being typed is in the list when the note is saved: drop it.
        tags.delete(query.query);
        if (tags.size === 0) return null;
        return {
          from: query.from,
          options: [...tags].sort().map((tag) => ({ label: tag, type: "tag" })),
          validFor: /^[\p{L}\p{N}_/-]*$/u,
        };
      }
    }
  };
}
