/**
 * The Confluence page of a note: the comment `<!-- confluence: 123456789 -->`.
 *
 * The core reads it as the field `confluence` (`kknow-index/src/parse.rs`),
 * and a command with `requires: ["confluence"]` gets the page as
 * `$KKNOW_FM_CONFLUENCE`. This file has only the text rules: the parser, the
 * comment format and where a new comment goes. The module has no imports on
 * purpose: the Node tests load it as is. The editor part is `confluence.ts`.
 */

export const CONFLUENCE_KEY = "confluence";

const COMMENT = /^<!--\s*confluence\s*:\s*(.*?)\s*-->$/;

/** The page from a one-line comment; `null` for other comments. */
export function parsePageComment(comment: string): string | null {
  const match = COMMENT.exec(comment.trim());
  if (!match || match[1].includes("\n")) return null;
  return match[1].replace(/^(["'])(.*)\1$/, "$2");
}

/**
 * The page from what the user typed: a number, or a link to the page. A link
 * has the number in `/pages/123/` or in `pageId=123`. Line breaks and `-->`
 * would break the comment, so they go.
 */
export function cleanPageId(input: string): string {
  const text = input.trim();
  const link = /\/pages\/(\d+)/.exec(text) ?? /[?&]pageId=(\d+)/.exec(text);
  if (link) return link[1];
  return text.replace(/\s+/g, " ").replaceAll("-->", "").trim();
}

export function formatPageComment(pageId: string): string {
  return `<!-- ${CONFLUENCE_KEY}: ${pageId} -->`;
}

/**
 * Where a new comment goes: the first line of the note, or the line after
 * the frontmatter. The page is about the whole file, so its place is at the
 * top, where the user sees it first.
 */
export function pageCommentPlace(text: string): number {
  const frontmatter = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/.exec(text);
  return frontmatter ? frontmatter[0].length : 0;
}
