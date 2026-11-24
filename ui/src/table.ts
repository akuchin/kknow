export type Align = "left" | "center" | "right" | null;

export interface Cell {
  text: string;
  from: number;
  to: number;
}

export interface Table {
  from: number;
  to: number;
  rows: Cell[][];
  aligns: Align[];
  columns: number;
  delimiter: boolean;
  lines: number[];
  indent: string;
}

const DELIMITER = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

export const ROW = /^ {0,3}\|/;

export function isDelimiter(line: string): boolean {
  return DELIMITER.test(line) && line.includes("-");
}

export function parseTable(source: string, offset: number): Table | null {
  const lines = source.split("\n");
  const rows: Cell[][] = [];
  const at: number[] = [];
  let aligns: Align[] = [];
  let delimiter = false;
  let start = offset;

  for (const [index, line] of lines.entries()) {
    if (index === 1 && isDelimiter(line)) {
      delimiter = true;
      aligns = splitRow(line, start).map((cell) => alignOf(cell.text));
    } else if (line.trim() !== "") {
      rows.push(splitRow(line, start));
      at.push(index);
    }
    start += line.length + 1;
  }

  if (rows.length === 0) return null;
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  return {
    from: offset,
    to: offset + source.length,
    rows,
    aligns,
    columns,
    delimiter,
    lines: at,
    indent: /^\s*/.exec(lines[0])?.[0] ?? "",
  };
}

function splitRow(line: string, offset: number): Cell[] {
  const spans = cellSpans(line);

  return spans.map(([begin, end]) => {
    const text = line.slice(begin, end);
    const trimmed = text.trim();
    const lead = trimmed === "" ? Math.min(1, text.length) : text.length - text.trimStart().length;

    return { text: trimmed, from: offset + begin + lead, to: offset + begin + lead + trimmed.length };
  });
}

function cellSpans(line: string): [number, number][] {
  const spans: [number, number][] = [];
  let start = 0;

  for (let at = 0; at < line.length; at += 1) {
    if (line[at] === "\\") {
      at += 1;
    } else if (line[at] === "|") {
      spans.push([start, at]);
      start = at + 1;
    }
  }
  spans.push([start, line.length]);

  if (spans.length > 1 && blank(line, spans[0])) spans.shift();
  if (spans.length > 1 && blank(line, spans[spans.length - 1])) spans.pop();
  return spans;
}

export function columnAt(line: string, column: number): number {
  let pipes = 0;
  for (let at = 0; at < Math.min(column, line.length); at += 1) {
    if (line[at] === "\\") at += 1;
    else if (line[at] === "|") pipes += 1;
  }
  const framed = line.trimStart().startsWith("|");
  return Math.max(0, framed ? pipes - 1 : pipes);
}

function blank(line: string, [begin, end]: [number, number]): boolean {
  return line.slice(begin, end).trim() === "";
}

function alignOf(text: string): Align {
  const left = text.startsWith(":");
  const right = text.endsWith(":");
  if (left && right) return "center";
  if (right) return "right";
  if (left) return "left";
  return null;
}

export function textWidth(text: string): number {
  let width = 0;
  for (const char of text) {
    if (/\p{M}/u.test(char)) continue;
    width += WIDE.test(char) ? 2 : 1;
  }
  return width;
}

const WIDE =
  /[ᄀ-ᅟ⺀-〾ぁ-㏿㐀-䶿一-鿿ꀀ-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]|\p{Extended_Pictographic}/u;

export function serializeTable(rows: string[][], aligns: Align[], indent = ""): string {
  const count = Math.max(1, aligns.length, ...rows.map((row) => row.length));
  const grid = rows.map((row) => Array.from({ length: count }, (_, at) => row[at] ?? ""));
  const widths = Array.from({ length: count }, (_, at) =>
    Math.max(3, ...grid.map((row) => textWidth(row[at]))),
  );

  const line = (cells: string[]) =>
    `${indent}| ${cells.map((cell, at) => pad(cell, widths[at], aligns[at] ?? null)).join(" | ")} |`;

  const marks = widths.map((width, at) => delimiter(aligns[at] ?? null, width));
  const [header, ...body] = grid.map(line);
  return [header, `${indent}| ${marks.join(" | ")} |`, ...body].join("\n");
}

function pad(text: string, width: number, align: Align): string {
  const space = Math.max(0, width - textWidth(text));
  if (align === "right") return " ".repeat(space) + text;
  if (align === "center") {
    const left = Math.floor(space / 2);
    return " ".repeat(left) + text + " ".repeat(space - left);
  }
  return text + " ".repeat(space);
}

function delimiter(align: Align, width: number): string {
  if (align === "center") return `:${"-".repeat(width - 2)}:`;
  if (align === "right") return `${"-".repeat(width - 1)}:`;
  if (align === "left") return `:${"-".repeat(width - 1)}`;
  return "-".repeat(width);
}

export function cellsOf(table: Table): string[][] {
  return table.rows.map((row) =>
    Array.from({ length: table.columns }, (_, at) => row[at]?.text ?? ""),
  );
}

export function insertRow(rows: string[][], after: number): string[][] {
  const empty = Array.from({ length: rows[0]?.length ?? 1 }, () => "");
  const next = rows.map((row) => [...row]);
  next.splice(Math.max(1, after + 1), 0, empty);
  return next;
}

export function removeRows(rows: string[][], from: number, to: number): string[][] {
  const first = Math.max(1, from);
  const last = Math.min(to, rows.length - 1);
  if (first > last) return rows;
  return rows.filter((_, index) => index < first || index > last);
}

export function insertColumn(
  rows: string[][],
  aligns: Align[],
  after: number,
): { rows: string[][]; aligns: Align[] } {
  const at = after + 1;
  return {
    rows: rows.map((row) => [...row.slice(0, at), "", ...row.slice(at)]),
    aligns: [...aligns.slice(0, at), null, ...aligns.slice(at)],
  };
}

export function removeColumns(
  rows: string[][],
  aligns: Align[],
  from: number,
  to: number,
): { rows: string[][]; aligns: Align[] } {
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  const first = Math.max(0, from);
  const last = Math.min(to, columns - 1);
  if (first > last || last - first + 1 >= columns) return { rows, aligns };

  const keep = <T>(values: T[]) => values.filter((_, index) => index < first || index > last);
  return { rows: rows.map(keep), aligns: keep(aligns) };
}

export function moveRows(
  rows: string[][],
  from: number,
  to: number,
  step: -1 | 1,
): string[][] | null {
  if (from < 1 || from + step < 1 || to + step > rows.length - 1) return null;
  const next = rows.map((row) => [...row]);
  const block = next.splice(from, to - from + 1);
  next.splice(from + step, 0, ...block);
  return next;
}

export function moveColumns(
  rows: string[][],
  aligns: Align[],
  from: number,
  to: number,
  step: -1 | 1,
): { rows: string[][]; aligns: Align[] } | null {
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  if (from + step < 0 || to + step > columns - 1) return null;

  const move = <T>(values: T[]): T[] => {
    const next = [...values];
    const block = next.splice(from, to - from + 1);
    next.splice(from + step, 0, ...block);
    return next;
  };
  const full = Array.from({ length: columns }, (_, at) => aligns[at] ?? null);
  return { rows: rows.map((row) => move(row)), aligns: move(full) };
}

export function setAlign(aligns: Align[], columns: number, from: number, to: number, align: Align): Align[] {
  const next = Array.from({ length: columns }, (_, at) => aligns[at] ?? null);
  const same = next.slice(from, to + 1).every((value) => value === align);
  for (let at = from; at <= to; at += 1) next[at] = same ? null : align;
  return next;
}

export function sortRows(rows: string[][], column: number, descending: boolean): string[][] {
  const [header, ...body] = rows;
  if (!header) return rows;
  const order = new Intl.Collator("ru", { numeric: true, sensitivity: "base" });

  const sorted = [...body].sort((a, b) => {
    const left = (a[column] ?? "").trim();
    const right = (b[column] ?? "").trim();
    if (left === "" || right === "") return Number(left === "") - Number(right === "");

    const x = Number(left.replace(",", "."));
    const y = Number(right.replace(",", "."));
    const result = Number.isNaN(x) || Number.isNaN(y) ? order.compare(left, right) : x - y;
    return descending ? -result : result;
  });
  return [header, ...sorted];
}

export function transpose(rows: string[][]): string[][] {
  const columns = Math.max(...rows.map((row) => row.length));
  return Array.from({ length: columns }, (_, column) => rows.map((row) => row[column] ?? ""));
}

export interface Rect {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

export type Command =
  | "rowAdd"
  | "rowRemove"
  | "columnAdd"
  | "columnRemove"
  | "alignLeft"
  | "alignCenter"
  | "alignRight"
  | "rowUp"
  | "rowDown"
  | "columnLeft"
  | "columnRight"
  | "sortAsc"
  | "sortDesc"
  | "transpose";

export interface Edited {
  rows: string[][];
  aligns: Align[];
  rect: Rect;
}

export function runCommand(
  command: Command,
  rows: string[][],
  aligns: Align[],
  rect: Rect,
): Edited | null {
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  const single = (row: number, column: number): Rect => ({
    top: row,
    left: column,
    bottom: row,
    right: column,
  });

  switch (command) {
    case "rowAdd": {
      const row = Math.max(1, rect.bottom + 1);
      return { rows: insertRow(rows, rect.bottom), aligns, rect: single(row, rect.left) };
    }
    case "rowRemove": {
      if (rect.bottom < 1) return null;
      const next = removeRows(rows, rect.top, rect.bottom);
      const row = Math.min(Math.max(1, rect.top), next.length - 1);
      return { rows: next, aligns, rect: single(row, rect.left) };
    }
    case "columnAdd": {
      const next = insertColumn(rows, aligns, rect.right);
      return { ...next, rect: single(rect.top, rect.right + 1) };
    }
    case "columnRemove": {
      if (rect.right - rect.left + 1 >= columns) return null;
      const next = removeColumns(rows, aligns, rect.left, rect.right);
      return { ...next, rect: single(rect.top, Math.min(rect.left, columns - 2 - (rect.right - rect.left))) };
    }
    case "alignLeft":
    case "alignCenter":
    case "alignRight": {
      const align = command === "alignLeft" ? "left" : command === "alignCenter" ? "center" : "right";
      return { rows, aligns: setAlign(aligns, columns, rect.left, rect.right, align), rect };
    }
    case "rowUp":
    case "rowDown": {
      const step = command === "rowUp" ? -1 : 1;
      const next = moveRows(rows, rect.top, rect.bottom, step);
      if (!next) return null;
      return { rows: next, aligns, rect: { ...rect, top: rect.top + step, bottom: rect.bottom + step } };
    }
    case "columnLeft":
    case "columnRight": {
      const step = command === "columnLeft" ? -1 : 1;
      const next = moveColumns(rows, aligns, rect.left, rect.right, step);
      if (!next) return null;
      return { ...next, rect: { ...rect, left: rect.left + step, right: rect.right + step } };
    }
    case "sortAsc":
    case "sortDesc":
      if (rows.length < 3) return null;
      return { rows: sortRows(rows, rect.left, command === "sortDesc"), aligns, rect };
    case "transpose":
      return {
        rows: transpose(rows),
        aligns: [],
        rect: { top: rect.left, left: rect.top, bottom: rect.right, right: rect.bottom },
      };
  }
}
