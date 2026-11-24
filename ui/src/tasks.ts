const BOX = /^(\s*(?:>\s*)*(?:[-*+]|\d{1,9}[.)])\s+)\[([ xX])\]/;

export function taskBox(line: string): { at: number; done: boolean } | null {
  const match = BOX.exec(line);
  if (!match) return null;
  return { at: match[1].length + 1, done: match[2] !== " " };
}

export function setTaskBox(line: string, done: boolean): string | null {
  const box = taskBox(line);
  if (!box) return null;
  return line.slice(0, box.at) + (done ? "x" : " ") + line.slice(box.at + 1);
}
