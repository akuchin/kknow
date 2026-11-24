import type { EditorState } from "@codemirror/state";
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

const ITEM = /^([ \t]*)([-*+]|(\d{1,9})([.)]))([ \t]+|$)/;

interface Item {
  indent: number;
  content: number;
  number: number | null;
}

function width(space: string): number {
  let columns = 0;
  for (const char of space) columns += char === "\t" ? 4 : 1;
  return columns;
}

function indentOf(line: string): number {
  return width(/^[ \t]*/.exec(line)?.[0] ?? "");
}

function itemOf(line: string): Item | null {
  const found = ITEM.exec(line);
  if (!found) return null;
  const indent = width(found[1]);
  const space = Math.max(1, width(found[5]));
  return {
    indent,
    content: indent + found[2].length + space,
    number: found[3] === undefined ? null : Number(found[3]),
  };
}

const blank = (line: string) => line.trim() === "";

export function blockOf(lines: readonly string[], start: number): [number, number] {
  const indent = indentOf(lines[start]);
  let end = start + 1;
  let at = end;
  while (at < lines.length) {
    if (blank(lines[at])) {
      at += 1;
      continue;
    }
    if (indentOf(lines[at]) <= indent) break;
    at += 1;
    end = at;
  }
  return [start, end];
}

function siblingAbove(lines: readonly string[], start: number): number | null {
  const indent = indentOf(lines[start]);
  for (let at = start - 1; at >= 0; at--) {
    if (blank(lines[at])) continue;
    const level = indentOf(lines[at]);
    if (level > indent) continue;
    return level === indent && itemOf(lines[at]) ? at : null;
  }
  return null;
}

function parentOf(lines: readonly string[], start: number): number | null {
  const indent = indentOf(lines[start]);
  for (let at = start - 1; at >= 0; at--) {
    if (blank(lines[at])) continue;
    const level = indentOf(lines[at]);
    if (level >= indent) continue;
    return itemOf(lines[at]) ? at : null;
  }
  return null;
}

function reindent(line: string, delta: number): string {
  if (blank(line)) return line;
  const space = /^[ \t]*/.exec(line)?.[0] ?? "";
  return " ".repeat(Math.max(0, width(space) + delta)) + line.slice(space.length);
}

function rootsIn(lines: readonly string[], from: number, to: number): number[] {
  const roots: number[] = [];
  let covered = -1;
  for (let at = from; at <= to; at++) {
    if (at < covered || !itemOf(lines[at])) continue;
    roots.push(at);
    covered = blockOf(lines, at)[1];
  }
  return roots;
}

export function shiftItems(
  lines: readonly string[],
  from: number,
  to: number,
  direction: 1 | -1,
): string[] | null {
  const first = itemOf(lines[from]);
  if (!first) return null;
  let delta: number;
  if (direction === 1) {
    const above = siblingAbove(lines, from);
    if (above === null) return null;
    delta = itemOf(lines[above])!.content - first.indent;
  } else {
    if (first.indent === 0) return null;
    const parent = parentOf(lines, from);
    delta = (parent === null ? 0 : indentOf(lines[parent])) - first.indent;
  }
  if (delta === 0) return null;

  const next = [...lines];
  const roots = rootsIn(lines, from, to);
  for (const root of roots) {
    const [start, end] = blockOf(lines, root);
    for (let at = start; at < end; at++) next[at] = reindent(lines[at], delta);
  }
  const [root] = roots;
  if (itemOf(next[root])?.number != null && siblingAbove(next, root) === null) {
    next[root] = next[root].replace(/^([ \t]*)\d+/, "$11");
  }
  return renumber(next, from, to);
}

export function moveItem(
  lines: readonly string[],
  line: number,
  direction: 1 | -1,
): { lines: string[]; line: number } | null {
  if (!itemOf(lines[line])) return null;
  const [start, end] = blockOf(lines, line);
  const block = lines.slice(start, end);
  const next = [...lines];

  if (direction === -1) {
    if (start === 0) return null;
    const above = siblingAbove(lines, start);
    const from = above ?? start - 1;
    next.splice(from, end - from, ...block, ...lines.slice(from, start));
    return { lines: renumber(next, from, end - 1), line: from + (line - start) };
  }

  if (end >= lines.length) return null;
  const below = itemOf(lines[end]) && indentOf(lines[end]) === indentOf(lines[start]);
  const after = below ? blockOf(lines, end)[1] : end + 1;
  const passed = lines.slice(end, after);
  next.splice(start, after - start, ...passed, ...block);
  return {
    lines: renumber(next, start, after - 1),
    line: start + passed.length + (line - start),
  };
}

export function renumber(lines: string[], from: number, to: number): string[] {
  const part = (at: number) => {
    const line = lines[at];
    return blank(line) || itemOf(line) !== null || indentOf(line) > 0;
  };
  let start = Math.min(from, lines.length - 1);
  while (start > 0 && part(start - 1)) start -= 1;
  let end = Math.max(to, 0);
  while (end < lines.length - 1 && part(end + 1)) end += 1;

  const levels: { indent: number; next: number | null }[] = [];
  for (let at = start; at <= end; at++) {
    const line = lines[at];
    if (blank(line)) continue;
    const item = itemOf(line);
    const indent = indentOf(line);
    while (levels.length > 0 && levels[levels.length - 1].indent > indent) levels.pop();
    if (!item) {
      while (levels.length > 0 && levels[levels.length - 1].indent >= indent) levels.pop();
      continue;
    }
    const top = levels[levels.length - 1];
    if (top && top.indent === indent && top.next !== null && item.number !== null) {
      if (item.number !== top.next) {
        lines[at] = line.replace(/^([ \t]*)\d+/, `$1${top.next}`);
      }
      top.next += 1;
      continue;
    }
    if (top && top.indent === indent) levels.pop();
    levels.push({ indent, next: item.number === null ? null : item.number + 1 });
  }
  return lines;
}

function linesOf(state: EditorState): { lines: string[]; from: number; to: number } {
  const range = state.selection.main;
  return {
    lines: state.doc.toString().split("\n"),
    from: state.doc.lineAt(range.from).number - 1,
    to: state.doc.lineAt(range.to).number - 1,
  };
}

function applyLines(view: EditorView, next: readonly string[], userEvent: string): void {
  const { doc } = view.state;
  const changes = [];
  for (let at = 0; at < next.length; at++) {
    const line = doc.line(at + 1);
    const old = line.text;
    const now = next[at];
    if (old === now) continue;
    let head = 0;
    while (head < old.length && head < now.length && old[head] === now[head]) head++;
    let tail = 0;
    while (
      tail < old.length - head &&
      tail < now.length - head &&
      old[old.length - 1 - tail] === now[now.length - 1 - tail]
    ) {
      tail++;
    }
    changes.push({
      from: line.from + head,
      to: line.to - tail,
      insert: now.slice(head, now.length - tail),
    });
  }
  view.dispatch({ changes, scrollIntoView: true, userEvent });
}

export function shiftCommand(direction: 1 | -1) {
  return (view: EditorView): boolean => {
    const { lines, from, to } = linesOf(view.state);
    if (!itemOf(lines[from])) return false;
    const next = shiftItems(lines, from, to, direction);
    if (next) applyLines(view, next, direction === 1 ? "input.indent" : "delete.dedent");
    return true;
  };
}

export function moveCommand(direction: 1 | -1) {
  return (view: EditorView): boolean => {
    const { state } = view;
    const { lines, from } = linesOf(state);
    const moved = moveItem(lines, from, direction);
    if (!moved) return itemOf(lines[from]) !== null;

    let first = 0;
    while (first < lines.length && lines[first] === moved.lines[first]) first++;
    let last = lines.length - 1;
    while (last > first && lines[last] === moved.lines[last]) last--;

    const head = state.selection.main.head;
    const column = head - state.doc.lineAt(head).from;
    const text = moved.lines.slice(first, last + 1).join("\n");
    const start = state.doc.line(first + 1).from;
    const before = moved.lines.slice(first, moved.line).join("\n").length + (moved.line > first ? 1 : 0);
    const cursor = start + before + Math.min(column, moved.lines[moved.line].length);
    view.dispatch({
      changes: { from: start, to: state.doc.line(last + 1).to, insert: text },
      selection: EditorSelection.cursor(cursor),
      scrollIntoView: true,
      userEvent: "move.line",
    });
    return true;
  };
}
