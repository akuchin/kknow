import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

export interface Heading {
  level: number;
  text: string;
  line: number;
}

const HEADING = /^(?:ATX|Setext)Heading([1-6])$/;

const MARKS = new Set(["HeaderMark", "EmphasisMark", "CodeMark", "StrikethroughMark"]);

const FRONTMATTER = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/;

const PARSE_BUDGET = 200;

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
      return false;
    },
  });
  return headings;
}

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
