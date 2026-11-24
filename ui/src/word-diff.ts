export function splitChange(old: string, next: string): [string, string, string, string] {
  let start = 0;
  while (start < old.length && start < next.length && old[start] === next[start]) start++;
  let end = 0;
  while (
    end < old.length - start &&
    end < next.length - start &&
    old[old.length - 1 - end] === next[next.length - 1 - end]
  ) {
    end++;
  }
  const letter = (ch: string | undefined) => ch !== undefined && /[\p{L}\p{N}_]/u.test(ch);
  while (start > 0 && letter(old[start - 1]) && (letter(old[start]) || letter(next[start]))) {
    start--;
  }
  while (
    end > 0 &&
    letter(old[old.length - end]) &&
    (letter(old[old.length - end - 1]) || letter(next[next.length - end - 1]))
  ) {
    end--;
  }
  return [
    old.slice(0, start),
    old.slice(start, old.length - end),
    next.slice(start, next.length - end),
    old.slice(old.length - end),
  ];
}
