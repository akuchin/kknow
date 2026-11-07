/**
 * Outline of the open note: its headings, for the "Go to heading" palette.
 *
 * The headings come from the editor's Markdown syntax tree, not from the core.
 * The editor already has the parse, and the outline must follow unsaved edits.
 * The tree also knows code blocks, so a `#` line inside a block is no heading.
 *
 * The module imports only packages: the Node tests load it as is.
 */

import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

/** A heading of the note. */
export interface Heading {
  /** 1…6; a setext heading is 1 (`===`) or 2 (`---`). */
  level: number;
  /** The text without the hashes, the underline and the inline marks. */
  text: string;
  /** 1-based line of the heading start. */
  line: number;
}

/** `ATXHeading2`, `SetextHeading1` → the level. */
const HEADING = /^(?:ATX|Setext)Heading([1-6])$/;

/** Marks that are not part of the heading text. */
const MARKS = new Set(["HeaderMark", "EmphasisMark", "CodeMark", "StrikethroughMark"]);

/**
 * Frontmatter at the file start, as the core reads it. The Markdown parser
 * does not know frontmatter: its closing `---` would make a setext heading.
 */
const FRONTMATTER = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/;

/** How long to parse a long note before the outline uses a partial tree, in ms. */
const PARSE_BUDGET = 200;

/** The headings of the document in their order. */
export function headingsOf(state: EditorState): Heading[] {
  const tree = ensureSyntaxTree(state, state.doc.length, PARSE_BUDGET) ?? syntaxTree(state);
  const skip = FRONTMATTER.exec(state.doc.toString())?.[0].length ?? 0;
  const headings: Heading[] = [];
  tree.iterate({
    from: skip,
    enter: (node) => {
      const level = HEADING.exec(node.name)?.[1];
      if (!level) return;
      if (node.from >= skip) {
        let text = "";
        let at = node.from;
        // The text is the heading range without the mark ranges.
        node.node.cursor().iterate((inner) => {
          if (!MARKS.has(inner.name)) return;
          text += state.doc.sliceString(at, inner.from);
          at = Math.max(at, inner.to);
        });
        text += state.doc.sliceString(at, node.to);
        headings.push({
          level: Number(level),
          text: text.replace(/\s+/g, " ").trim(),
          line: state.doc.lineAt(node.from).number,
        });
      }
      // A heading has no headings inside it.
      return false;
    },
  });
  return headings;
}

/**
 * The index of the heading whose section is at the top of the view: the last
 * heading at or above line `top`. `-1` means the view is above the first
 * heading.
 *
 * At the end of the note the last sections cannot scroll to the top. Then
 * `bottom` is the last visible line, and the last heading on the screen wins.
 */
export function currentHeading(
  headings: readonly Heading[],
  top: number,
  bottom: number | null = null,
): number {
  const limit = bottom ?? top;
  let found = -1;
  for (const [index, heading] of headings.entries()) {
    if (heading.line > limit) break;
    found = index;
  }
  return found;
}
