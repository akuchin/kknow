import type {
  LinkTargetView,
  MentionView,
  LinkPreview,
  LinkRef,
} from "../api";
import { coded, find, session } from "./store";
import { linkSpans, view } from "./views";
export function linkTarget(from: string, link: string): LinkTargetView | null {
  const [target, subpath] = link.split("|")[0].split(/#(.*)/s);
  const wanted = target.trim().replace(/\.md$/i, "").toLowerCase();
  const note = wanted
    ? session.activeVault.notes.find(
        (each) =>
          each.id.toLowerCase() === wanted ||
          (each.id.split("/").pop() ?? "").toLowerCase() === wanted,
      )
    : session.activeVault.notes.find((each) => each.id === from);
  if (!note) return null;
  const lines = note.content.split("\n");
  const headings = lines.flatMap((text, index) => {
    const found = /^(#{1,6})\s+(.*?)\s*$/.exec(text);
    return found ? [{ level: found[1].length, text: found[2], line: index + 1 }] : [];
  });
  const blocks = lines.flatMap((text, index) => {
    const found = /(?:^|\s)\^([A-Za-z0-9-]+)\s*$/.exec(text);
    return found ? [{ id: found[1], line: index + 1 }] : [];
  });
  const sub = subpath?.trim() ?? "";
  let line: number | null = null;
  if (sub.startsWith("^")) {
    line = blocks.find((each) => each.id.toLowerCase() === sub.slice(1).toLowerCase())?.line ?? null;
  } else if (sub) {
    const last = (sub.split("#").pop() ?? sub).toLowerCase();
    line = headings.find((each) => each.text.toLowerCase() === last)?.line ?? null;
  }
  return { id: note.id, line, headings, blocks: blocks.map((each) => each.id) };
}

export function unlinkedMentions(id: string): MentionView[] {
  const name = id.split("/").pop() ?? id;
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\p{L}\\p{N}_])`, "giu");
  const mentions: MentionView[] = [];
  for (const note of [...session.activeVault.notes].sort((a, b) => a.id.localeCompare(b.id))) {
    if (note.id === id || note.locked) continue;
    const links = linkSpans(note.content);
    let lineStart = 0;
    let code = false;
    note.content.split("\n").forEach((text, index) => {
      if (text.startsWith("```")) code = !code;
      for (const found of code ? [] : text.matchAll(pattern)) {
        const start = lineStart + (found.index ?? 0);
        const end = start + found[0].length;
        if (links.some(([from, to]) => start < to && from < end)) continue;
        mentions.push({
          id: note.id,
          title: note.title,
          line: index + 1,
          before: text.slice(0, found.index).trimStart(),
          text: found[0],
          after: text.slice((found.index ?? 0) + found[0].length).trimEnd(),
          start,
          end,
        });
      }
      lineStart += text.length + 1;
    });
  }
  return mentions;
}

export function linkMention(from: string, start: number, end: number, text: string, target: string): LinkRef[] {
  const note = find(from);
  if (note.content.slice(start, end) !== text) {
    const line = note.content.slice(0, start).split("\n").length;
    throw coded("notAMention", `${from} has no mention on line ${line}`, { value: from, line: String(line) });
  }
  const name = target.split("/").pop() ?? target;
  const link = name === text ? `[[${name}]]` : `[[${name}|${text}]]`;
  note.content = note.content.slice(0, start) + link + note.content.slice(end);
  return view(find(target)).backlinks;
}

export function linkPreview(from: string, link: string): LinkPreview | null {
  const target = linkTarget(from, link);
  if (!target) return null;
  const note = find(target.id);
  if (note.locked) {
    return { id: note.id, title: note.title, html: "", line: null, partMissing: false, locked: true };
  }
  const lines = note.content.split("\n");
  const sub = link.split("|")[0].split(/#(.*)/s)[1]?.trim() ?? "";
  const start = target.line;
  let part = lines;
  if (start !== null) {
    const level = /^(#+)\s/.exec(lines[start - 1])?.[1].length ?? 7;
    const end = lines.findIndex(
      (text, index) => index >= start && (/^(#+)\s/.exec(text)?.[1].length ?? 7) <= level,
    );
    part = lines.slice(start - 1, end < 0 ? lines.length : end);
  }
  const whole = note.html.replace(/<span class="line-mark[^"]*" data-line="\d+"><\/span>/g, "");
  const html = start === null ? whole : part
    .filter((text) => text.trim() && !text.startsWith("<!--"))
    .map((text) => {
      const heading = /^(#+)\s+(.*)$/.exec(text);
      const escaped = (heading?.[2] ?? text).replace(/&/g, "&amp;").replace(/</g, "&lt;");
      return heading ? `<h${heading[1].length}>${escaped}</h${heading[1].length}>` : `<p>${escaped}</p>`;
    })
    .join("");
  return {
    id: note.id,
    title: note.title,
    html,
    line: start,
    partMissing: Boolean(sub) && start === null,
    locked: false,
  };
}


