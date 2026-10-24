/**
 * Task boxes in the note text: `- [ ] text`, `- [x] text`.
 *
 * The core finds tasks with the markdown parser and writes the file. The
 * window only changes the same character in the editor after the core wrote
 * it, so the editor text stays equal to the disk.
 *
 * The module has no DOM imports: the Node tests load it as is.
 */

/** A list marker (also nested, numbered and in a quote), then the box. */
const BOX = /^(\s*(?:>\s*)*(?:[-*+]|\d{1,9}[.)])\s+)\[([ xX])\]/;

/** The box of a line: the offset of the character in the brackets and its state. */
export function taskBox(line: string): { at: number; done: boolean } | null {
  const match = BOX.exec(line);
  if (!match) return null;
  return { at: match[1].length + 1, done: match[2] !== " " };
}

/** The line with the box set to `done`; `null` if the line has no box. */
export function setTaskBox(line: string, done: boolean): string | null {
  const box = taskBox(line);
  if (!box) return null;
  return line.slice(0, box.at) + (done ? "x" : " ") + line.slice(box.at + 1);
}
