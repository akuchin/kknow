export const CONFLUENCE_KEY = "confluence";

const COMMENT = /^<!--\s*confluence\s*:\s*(.*?)\s*-->$/;

export function parsePageComment(comment: string): string | null {
  const match = COMMENT.exec(comment.trim());
  if (!match || match[1].includes("\n")) return null;
  return match[1].replace(/^(["'])(.*)\1$/, "$2");
}

export function cleanPageId(input: string): string {
  const text = input.trim();
  const link = /\/pages\/(\d+)/.exec(text) ?? /[?&]pageId=(\d+)/.exec(text);
  if (link) return link[1];
  return text.replace(/\s+/g, " ").replaceAll("-->", "").trim();
}

export function formatPageComment(pageId: string): string {
  return `<!-- ${CONFLUENCE_KEY}: ${pageId} -->`;
}

export function pageCommentPlace(text: string): number {
  const frontmatter = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/.exec(text);
  return frontmatter ? frontmatter[0].length : 0;
}
