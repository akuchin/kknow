export type FindQuery = {
  text: string;
  caseSensitive: boolean;
  wholeWord: boolean;
  regexp: boolean;
};

export type FindRange = { block: number; from: number; to: number };

export const EMPTY_QUERY: FindQuery = { text: "", caseSensitive: false, wholeWord: false, regexp: false };

const WORD = /[\p{L}\p{N}_]/u;

function pattern(query: FindQuery): RegExp | null {
  const source = query.regexp ? query.text : query.text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return new RegExp(source, query.caseSensitive ? "gu" : "giu");
  } catch {
    return null;
  }
}

function isWordAt(text: string, index: number): boolean {
  const point = text.codePointAt(index);
  return point !== undefined && WORD.test(String.fromCodePoint(point));
}

function wordBefore(text: string, index: number): boolean {
  if (index <= 0) return false;
  const low = text.charCodeAt(index - 1);
  const start = low >= 0xdc00 && low <= 0xdfff && index >= 2 ? index - 2 : index - 1;
  return isWordAt(text, start);
}

function separated(text: string, from: number, to: number): boolean {
  return !wordBefore(text, from) && !isWordAt(text, to);
}

export function findRanges(blocks: string[], query: FindQuery): FindRange[] | null {
  if (query.text === "") return [];
  const regexp = pattern(query);
  if (regexp === null) return null;
  const found: FindRange[] = [];
  blocks.forEach((text, block) => {
    regexp.lastIndex = 0;
    for (let match = regexp.exec(text); match !== null; match = regexp.exec(text)) {
      const from = match.index;
      const to = from + match[0].length;
      if (to === from) {
        regexp.lastIndex += 1;
        continue;
      }
      if (query.wholeWord && !separated(text, from, to)) {
        regexp.lastIndex = from + 1;
        continue;
      }
      found.push({ block, from, to });
    }
  });
  return found;
}
