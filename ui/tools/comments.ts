import { readdirSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript";

export type Range = { start: number; end: number };

const DIRECTIVE = /^(\/\/\s*@ts-(expect-error|ignore|nocheck)|\/\/\/\s*<reference\s|\/\*\s*@vite-ignore\s*\*\/)/;

const SKIPPED_DIRS = new Set(["node_modules", "dist", "test-results", "playwright-report", "cache", ".temp"]);

export function findComments(text: string, fileName = "file.ts"): Range[] {
  const source = ts.createSourceFile(fileName, text, ts.ScriptTarget.Latest, true);
  const found = new Map<number, Range>();
  const collect = (ranges: readonly ts.CommentRange[] | undefined): void => {
    for (const { pos, end } of ranges ?? []) {
      if (!DIRECTIVE.test(text.slice(pos, end))) found.set(pos, { start: pos, end });
    }
  };
  const visit = (node: ts.Node): void => {
    if (node.kind >= ts.SyntaxKind.FirstJSDocNode && node.kind <= ts.SyntaxKind.LastJSDocNode) return;
    collect(ts.getLeadingCommentRanges(text, node.pos));
    collect(ts.getTrailingCommentRanges(text, node.end));
    for (const child of node.getChildren(source)) visit(child);
  };
  visit(source);
  return [...found.values()].sort((a, b) => a.start - b.start);
}

export function scriptFiles(root: string, dirs: string[]): string[] {
  const files: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (!SKIPPED_DIRS.has(entry.name)) walk(path);
      } else if (/\.(ts|mts|js|mjs)$/.test(entry.name) && !entry.name.endsWith(".d.ts")) {
        files.push(path);
      }
    }
  };
  for (const dir of dirs) walk(join(root, dir));
  return files.map((path) => relative(root, path).replaceAll("\\", "/")).sort();
}

const REMOVED_LINE = "\u0000";

export function stripComments(text: string, comments: Range[]): string {
  return dropLinesLeftByComments(cutComments(text, mergeSameLine(text, comments)));
}

function mergeSameLine(text: string, comments: Range[]): Range[] {
  const merged: Range[] = [];
  for (const comment of comments) {
    const last = merged.at(-1);
    if (last && /^[ \t]*$/.test(text.slice(last.end, comment.start))) {
      last.end = comment.end;
    } else {
      merged.push({ ...comment });
    }
  }
  return merged;
}

function cutComments(text: string, comments: Range[]): string {
  let out = "";
  let cursor = 0;
  for (const { start, end } of comments) {
    const lineStart = text.lastIndexOf("\n", start - 1) + 1;
    const newline = text.indexOf("\n", end);
    const lineEnd = newline === -1 ? text.length : newline;
    const blankBefore = text.slice(lineStart, start).trim() === "";
    const blankAfter = text.slice(end, lineEnd).trim() === "";
    const spacesBefore = start - text.slice(0, start).replace(/[ \t]+$/, "").length;
    const spacesAfter = text.slice(end).length - text.slice(end).replace(/^[ \t]+/, "").length;

    if (blankBefore && blankAfter) {
      out += text.slice(cursor, lineStart) + REMOVED_LINE + (lineEnd < text.length ? "\n" : "");
      cursor = Math.min(lineEnd + 1, text.length);
    } else if (blankAfter) {
      out += text.slice(cursor, start - spacesBefore);
      cursor = end;
    } else if (blankBefore) {
      out += text.slice(cursor, start);
      cursor = end + spacesAfter;
    } else {
      const left = start - spacesBefore;
      const right = end + spacesAfter;
      out += text.slice(cursor, left);
      if (needsSpace(text, left, right, spacesBefore + spacesAfter > 0)) out += " ";
      cursor = right;
    }
  }
  return out + text.slice(cursor);
}

function needsSpace(text: string, left: number, right: number, hadSpace: boolean): boolean {
  const before = text[left - 1] ?? "";
  const after = text[right] ?? "";
  return hadSpace && !"([{".includes(before || "(") && !")]},;.".includes(after || ")");
}

function dropLinesLeftByComments(text: string): string {
  const lines = text.split("\n");
  const kept: string[] = [];
  let at = 0;
  while (at < lines.length) {
    if (lines[at] !== REMOVED_LINE) {
      kept.push(lines[at++]);
      continue;
    }
    while (at < lines.length && lines[at] === REMOVED_LINE) at++;
    const previous = kept.at(-1);
    const next = lines[at];
    const nextIsLastEmpty = at + 1 === lines.length && next === "";
    if (next !== undefined && isBlank(next) && !nextIsLastEmpty && (previous === undefined || isBlank(previous) || opensBlock(previous))) {
      at++;
    } else if (previous !== undefined && isBlank(previous) && (next === undefined || closesBlock(next) || nextIsLastEmpty)) {
      kept.pop();
    }
  }
  return kept.join("\n");
}

function isBlank(line: string): boolean {
  return line.trim() === "";
}

function opensBlock(line: string): boolean {
  return /[{([]\s*$/.test(line);
}

function closesBlock(line: string): boolean {
  return /^\s*[})\]]/.test(line);
}

export function lineOf(text: string, offset: number): number {
  return text.slice(0, offset).split("\n").length;
}
