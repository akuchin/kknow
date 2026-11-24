import type {
  BookmarkMark,
  BookmarkView,
  GlobalResult,
  NoteSummary,
  SearchResult,
  TaskView,
} from "../api";
import { setTaskBox, taskBox } from "../tasks";
import { linkMention, linkPreview, linkTarget, unlinkedMentions } from "./links";
import { noteBacklinks } from "./views";
import { MockNote, MockVault, bookmarkLines, box, coded, escapeRegExp, find, lockOf, mockImage, putInTrash, relink, session, vaults } from "./store";
import { view } from "./views";
import type { Handlers } from "./handlers";

export function hits(vault: MockVault, query: string): { note: MockNote; text: string }[] {
  const { field, text } = parseQuery(query);
  if (!text) return [];

  return vault.notes
    .filter((note) => {
      if (field === "title") return note.title.toLowerCase().includes(text);
      if (note.locked) return note.id.toLowerCase().includes(text);
      if (field === "tag") return note.tags.some((tag) => tag.toLowerCase().includes(text));
      return (
        note.content.toLowerCase().includes(text) ||
        note.title.toLowerCase().includes(text) ||
        note.tags.some((tag) => tag.toLowerCase().includes(text))
      );
    })
    .map((note) => ({ note, text }));
}

export function snippetOf(note: MockNote, text: string): string {
  return note.content
    .replace(/[#*`\[\]]/g, "")
    .trim()
    .slice(0, 120)
    .replace(new RegExp(`(${escapeRegExp(text)})`, "gi"), "<b>$1</b>");
}

export function parseQuery(query: string): { field: string | null; text: string } {
  const match = /^(title|tag|body|path):\((.*)\)$|^(title|tag|body|path):(\S+)$/.exec(query.trim());
  if (!match) {
    if (/^\w+:\(/.test(query.trim())) throw coded("searchQuery", "could not parse the query", { detail: query });
    return { field: null, text: query.trim().toLowerCase() };
  }

  return {
    field: match[1] ?? match[3],
    text: (match[2] ?? match[4]).toLowerCase(),
  };
}

export const attachments = new Map<string, ArrayBuffer | null>();

export function mockAttachment(name: string, bytes: ArrayBuffer | null): string {
  if (!isImageName(name)) throw coded("invalidImageName", `invalid image name: ${name}`, { value: name });
  const dot = name.lastIndexOf(".");
  let free = name;
  for (let n = 2; attachments.has(`attachments/${free}`); n++) {
    free = `${name.slice(0, dot)} ${n}${name.slice(dot)}`;
  }
  attachments.set(`attachments/${free}`, bytes);
  return `attachments/${free}`;
}

export function isImageName(name: string): boolean {
  return /^[^./\\][^/\\]*\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i.test(name);
}

export const notesHandlers = {
  list_notes: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    return asked.notes.map<NoteSummary>((note) => ({
      id: note.id,
      title: note.title,
      folder: note.folder,
      tags: note.locked ? [] : note.tags,
      locked: note.locked === true,
    }));
  },

  load_note: (args) => {
    const id = args.id;
    return view(find(id));
  },

  save_note: (args) => {
    const id = args.id;
    const note = find(id);
    if (note.locked && !lockOf().unlocked) {
      throw coded("notesClosed", "locked notes are closed: enter the vault password");
    }
    note.content = args.content;
    const lines = args.bookmarks as BookmarkMark[] | null;
    if (lines) note.bookmarks = bookmarkLines(lines, note.content);
    return { status: "saved", note: view(note) };
  },

  set_bookmarks: (args) => {
    const id = args.id;
    const note = find(id);
    note.bookmarks = bookmarkLines(args.lines as BookmarkMark[], args.content);
    return note.bookmarks;
  },

  list_bookmarks: () => {
    return session.activeVault.notes.flatMap((note) =>
      (note.bookmarks ?? []).map<BookmarkView>(({ line, name }) => ({
        note: note.id,
        title: note.title,
        folder: note.folder,
        line,
        text: note.content.split("\n")[line - 1] ?? "",
        ...(name ? { name } : {}),
      })),
    );
  },

  list_tasks: () => {
    return session.activeVault.notes.flatMap((note) =>
      note.content.split("\n").flatMap<TaskView>((text, index) => {
        const found = taskBox(text);
        if (!found) return [];
        return [
          {
            note: note.id,
            title: note.title,
            folder: note.folder,
            line: index + 1,
            text: text.slice(found.at + 2).trim(),
            done: found.done,
          },
        ];
      }),
    );
  },

  set_task: (args) => {
    const id = args.id;
    const note = find(id);
    const number = args.line;
    const done = args.done;
    const lines = note.content.split("\n");
    const changed = setTaskBox(lines[number - 1] ?? "", done);
    if (changed === null) throw coded("notATask", `${note.id} has no task on line ${number}: the note has changed`, { value: note.id, line: String(number) });
    lines[number - 1] = changed;
    note.content = lines.join("\n");
    note.html = note.html.replace(
      new RegExp(`(data-line="${number}"></span>)<input[^>]*/>\\n`),
      `$1${box(done)}`,
    );
    return view(note);
  },

  preview: (args) => {
    const id = args.id;
    return find(id).html;
  },

  note_image: (args) => {
    const target = args.target;
    return (attachments.get(target) ?? mockImage(target));
  },

  save_attachment: (body, options) => {
    const name = decodeURIComponent(options?.headers?.["kknow-name"] ?? "");
    const bytes = body.slice().buffer;
    return mockAttachment(name, bytes);
  },

  attach_file: (args) => {
    const source = args.source;
    return mockAttachment(source.slice(source.lastIndexOf("/") + 1), null);
  },

  link_target: (args) => {
    return linkTarget(args.from, args.target);
  },

  link_preview: (args) => {
    return linkPreview(args.from, args.target);
  },

  note_backlinks: (args) => {
    return noteBacklinks(args.id);
  },

  unlinked_mentions: (args) => {
    return unlinkedMentions(args.id);
  },

  link_mention: (args) => {
    return linkMention(
      args.from,
      args.start,
      args.end,
      args.text,
      args.target,
    );
  },

  create_note: (args) => {
    const name = args.name;
    const note: MockNote = {
      id: name,
      title: name.split("/").pop() ?? name,
      folder: name.includes("/") ? name.slice(0, name.lastIndexOf("/")) : null,
      tags: [],
      content: `# ${name}\n\n`,
      html: `<h1>${name}</h1>`,
    };
    session.activeVault.notes.push(note);
    return view(note);
  },

  search: (args) => {
    return hits(session.activeVault, (args.query) ?? "").map<SearchResult>(
      ({ note, text }) => ({
        id: note.id,
        title: note.title,
        folder: note.folder,
        snippet: snippetOf(note, text),
      }),
    );
  },

  search_all: (args) => {
    const order = [session.activeVault, ...vaults.filter((vault) => vault !== session.activeVault)];
    const only = args.vaults;
    return order
      .filter((vault) => vault.available && (!only || only.includes(vault.id)))
      .flatMap((vault) =>
        hits(vault, args.query).map<GlobalResult>(({ note, text }) => ({
          vault: vault.id,
          vaultName: vault.name,
          color: vault.color,
          id: note.id,
          title: note.title,
          folder: note.folder,
          snippet: snippetOf(note, text),
        })),
      );
  },

  reveal_note: (args) => {
    const id = args.id;
    console.info("reveal_note", id ?? "(vault)");
    return;
  },

  delete_note: (args) => {
    const id = args.id;
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const index = asked.notes.findIndex((note) => note.id === id);
    if (index < 0) throw coded("notFound", `note not found: ${id}`, { value: id });
    putInTrash(asked, `${id}.md`, asked.notes.splice(index, 1), []);
    return;
  },

  open_note: (args) => {
    console.info("open_note", args.id);
    return;
  },

  reveal_folder: (args) => {
    console.info("reveal_folder", args.name);
    return;
  },

  rename_note: (args) => {
    const id = args.id;
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const to = (args.to).replace(/^\/+|\/+$/g, "");
    const note = asked.notes.find((note) => note.id === id);
    if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
    if (asked.notes.some((other) => other !== note && other.id === to)) {
      throw coded("noteExists", `note already exists: ${to}`, { value: to });
    }
    note.id = to;
    note.folder = to.includes("/") ? to.slice(0, to.lastIndexOf("/")) : null;
    return { id: to, relinked: relink(asked, [[id as string, to]]) };
  },

  duplicate_note: (args) => {
    const id = args.id;
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const note = asked.notes.find((note) => note.id === id);
    if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
    let copy = `${note.id} copy`;
    for (let n = 2; asked.notes.some((other) => other.id === copy); n += 1) {
      copy = `${note.id} copy ${n}`;
    }
    asked.notes.push({ ...note, id: copy, tags: [...note.tags] });
    return copy;
  },

  rename_folder: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const name = args.name;
    const to = (args.to).replace(/^\/+|\/+$/g, "");
    const prefix = `${name}/`;
    const moved = (path: string) => `${to}/${path.slice(prefix.length)}`;
    const moves: [string, string][] = [];
    for (const note of asked.notes) {
      if (!note.id.startsWith(prefix)) continue;
      moves.push([note.id, moved(note.id)]);
      note.id = moved(note.id);
      note.folder = note.id.slice(0, note.id.lastIndexOf("/"));
    }
    asked.folders = asked.folders.map((folder) =>
      folder === name ? to : folder.startsWith(prefix) ? moved(folder) : folder,
    );
    return relink(asked, moves);
  },

  list_folders: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const all = new Set<string>(asked.folders);
    for (const note of asked.notes) {
      let path = "";
      for (const part of note.folder?.split("/") ?? []) {
        path = path ? `${path}/${part}` : part;
        all.add(path);
      }
    }
    return [...all];
  },

  create_folder: (args) => {
    const name = (args.name).replace(/^\/+|\/+$/g, "");
    if (!session.activeVault.folders.includes(name)) session.activeVault.folders.push(name);
    return;
  },

  delete_folder: (args) => {
    const asked = vaults.find((entry) => entry.id === args.vault) ?? session.activeVault;
    const name = args.name;
    const prefix = `${name}/`;
    const inside = (path: string) => path === name || path.startsWith(prefix);
    putInTrash(
      asked,
      name,
      asked.notes.filter((note) => inside(note.id)),
      asked.folders.filter(inside),
    );
    asked.notes = asked.notes.filter((note) => !inside(note.id));
    asked.folders = asked.folders.filter((folder) => !inside(folder));
    return;
  },
} satisfies Handlers;
