import type { BacklinkLineView } from "../api";
import type { NoteView } from "../bindings";
import { MockNote, lockOf, proposals, recount, session, threads } from "./store";
export function view(note: MockNote): NoteView {
  if (note.locked && !lockOf().unlocked) {
    return { ...openView(note), content: "", html: "", closed: true, bookmarks: [] };
  }
  return openView(note);
}

export function openView(note: MockNote): NoteView {
  const locked = note.locked === true;
  return {
    locked,
    closed: false,
    id: note.id,
    title: note.title,
    content: note.content,
    html: note.html,
    backlinks: [...new Map(noteBacklinks(note.id).map((row) => [row.id, row])).values()].map(
      (row) => ({ id: row.id, title: row.title, label: null }),
    ),
    outgoing: [],
    tags: note.tags,
    bookmarks: locked ? [] : (note.bookmarks ?? []),
    threads: locked ? [] : threads.filter((thread) => thread.note === note.id),
    proposals: locked
      ? []
      : proposals
          .filter((proposal) => proposal.note === note.id)
          .map((proposal) => recount(proposal, note.content))
          .filter((proposal) => proposal.status === "pending"),
  };
}

export function noteBacklinks(id: string): BacklinkLineView[] {
  const names = [id, id.split("/").pop() ?? id].map((each) => each.toLowerCase());
  const lines: BacklinkLineView[] = [];
  for (const note of [...session.activeVault.notes].sort((a, b) => a.id.localeCompare(b.id))) {
    if (note.id === id || note.locked) continue;
    note.content.split("\n").forEach((text, index) => {
      for (const found of text.matchAll(/!?\[\[([^\]|#]+)[^\]]*\]\]/g)) {
        if (!names.includes(found[1].trim().toLowerCase())) continue;
        const at = found.index ?? 0;
        lines.push({
          id: note.id,
          title: note.title,
          line: index + 1,
          before: text.slice(0, at).trimStart(),
          text: found[0],
          after: text.slice(at + found[0].length).trimEnd(),
        });
      }
    });
  }
  return lines;
}

export function linkSpans(text: string): [number, number][] {
  return [...text.matchAll(/!?\[\[[^\]]*\]\]/g)].map((found) => [
    found.index ?? 0,
    (found.index ?? 0) + found[0].length,
  ]);
}

