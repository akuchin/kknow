/**
 * A markdown table as data: parsing, serializing and structure edits.
 *
 * This is separate from live preview because it is pure text work: no
 * CodeMirror and no DOM here. The core is not involved either: the edits change
 * the editor buffer, not the vault.
 */

/** Column alignment as written in the separator line. */
export type Align = "left" | "center" | "right" | null;

export interface Cell {
  /** Raw content with markup, without surrounding spaces. */
  text: string;
  /** Document range: it lets an edit change one cell, not the whole table. */
  from: number;
  to: number;
}

export interface Table {
  from: number;
  to: number;
  /** The first row is the header; the separator line is not included. */
  rows: Cell[][];
  aligns: Align[];
  /** Column count of the widest row. */
  columns: number;
}

/**
 * Parses a table from its markup. `offset` is where the markup starts in the
 * document, so that cells get real positions.
 *
 * Parsing uses the text, not the CodeMirror tree: cell ranges must be exact,
 * and the separator line would need a character scan anyway.
 */
export function parseTable(source: string, offset: number): Table | null {
  const lines = source.split("\n");
  if (lines.length < 2) return null;

  const rows: Cell[][] = [];
  let aligns: Align[] = [];
  let at = offset;

  for (const [index, line] of lines.entries()) {
    if (index === 1) {
      aligns = splitRow(line, at).map((cell) => alignOf(cell.text));
    } else if (line.trim() !== "") {
      rows.push(splitRow(line, at));
    }
    // The line break that split removed.
    at += line.length + 1;
  }

  if (rows.length === 0) return null;
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  return { from: offset, to: offset + source.length, rows, aligns, columns };
}

/**
 * Splits a row at unescaped pipes.
 *
 * The outer pipes are the row frame, not empty edge cells: `| a | b |` is two
 * cells, not four.
 */
function splitRow(line: string, offset: number): Cell[] {
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

  return spans.map(([begin, end]) => {
    const text = line.slice(begin, end);
    const trimmed = text.trim();
    // For an empty cell, put the range inside the spaces: an edit turns `|  |`
    // into `| a |`, not `|  a|`.
    const lead = trimmed === "" ? Math.min(1, text.length) : text.length - text.trimStart().length;

    return { text: trimmed, from: offset + begin + lead, to: offset + begin + lead + trimmed.length };
  });
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

/**
 * Builds the markup back and pads the columns with spaces.
 *
 * The padding is for a person who opens the `.md` in another editor. Because
 * of it, any structure edit rewrites the whole table.
 */
export function serializeTable(rows: string[][], aligns: Align[]): string {
  const count = Math.max(aligns.length, ...rows.map((row) => row.length));
  const grid = rows.map((row) => Array.from({ length: count }, (_, at) => row[at] ?? ""));
  // At least three dashes: `|-|` is not a separator.
  const widths = Array.from({ length: count }, (_, at) =>
    Math.max(3, ...grid.map((row) => row[at].length)),
  );

  const line = (cells: string[]) =>
    `| ${cells.map((cell, at) => cell.padEnd(widths[at])).join(" | ")} |`;

  const marks = widths.map((width, at) => delimiter(aligns[at] ?? null, width));
  const [header, ...body] = grid.map(line);
  return [header, line(marks), ...body].join("\n");
}

function delimiter(align: Align, width: number): string {
  if (align === "center") return `:${"-".repeat(width - 2)}:`;
  if (align === "right") return `${"-".repeat(width - 1)}:`;
  if (align === "left") return `:${"-".repeat(width - 1)}`;
  return "-".repeat(width);
}

/**
 * Makes text safe to put into a cell.
 *
 * A pipe would split the cell, a line break would split the table. A pipe that
 * is already escaped is not escaped again: cells hold raw markup.
 */
export function toCellSource(text: string): string {
  return text.replace(/\s*\n\s*/g, " ").replace(/\\?\|/g, "\\|").trim();
}

/** Table content as a plain string grid; structure edits work on it. */
export function cellsOf(table: Table): string[][] {
  return table.rows.map((row) =>
    Array.from({ length: table.columns }, (_, at) => row[at]?.text ?? ""),
  );
}

export function insertRow(rows: string[][], after: number): string[][] {
  const empty = Array.from({ length: rows[0]?.length ?? 1 }, () => "");
  const next = rows.map((row) => [...row]);
  // The header stays first: the earliest insert position is right below it.
  next.splice(Math.max(1, after + 1), 0, empty);
  return next;
}

/**
 * Removes rows `from` to `to` inclusive: users select rows as a group, and
 * they must go as a group.
 *
 * The header is never removed: without it the table is not a table. So a
 * selection that covers it is trimmed, and the edit still happens.
 */
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

/** `from` and `to` are inclusive. */
export function removeColumns(
  rows: string[][],
  aligns: Align[],
  from: number,
  to: number,
): { rows: string[][]; aligns: Align[] } {
  const columns = Math.max(aligns.length, ...rows.map((row) => row.length));
  const first = Math.max(0, from);
  const last = Math.min(to, columns - 1);
  // A table cannot have zero columns, so removing all of them is refused.
  if (first > last || last - first + 1 >= columns) return { rows, aligns };

  const keep = <T>(values: T[]) => values.filter((_, index) => index < first || index > last);
  return { rows: rows.map(keep), aligns: keep(aligns) };
}
