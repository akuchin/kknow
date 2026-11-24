import type {
  FileEntry,
} from "../api";
import type { FileView } from "../bindings";
import { attachments } from "./notes";
import { coded, mockImage, session } from "./store";
import type { Handlers } from "./handlers";

export const looseFiles = new Map<string, string>([
  [
    "/Users/me/src/kknow/README.md",
    "# kknow\n\nA notes app in Rust. See [[Rust]]: in a standalone file the link is broken.\n",
  ],
]);


export function fileEntry(path: string): FileEntry {
  const slash = path.lastIndexOf("/");
  return { path, name: path.slice(slash + 1), folder: path.slice(0, Math.max(slash, 0)) };
}

export function fileView(path: string): FileView {
  const content = looseFiles.get(path);
  if (content === undefined) throw coded("fileGone", `the file is gone and was removed from the list: ${path}`, { value: path });
  const heading = /^#\s+(.+)$/m.exec(content)?.[1];
  return {
    id: path,
    title: heading ?? fileEntry(path).name.replace(/\.[^.]+$/, ""),
    content,
    html: `<pre>${content.replace(/&/g, "&amp;").replace(/</g, "&lt;")}</pre>`,
    backlinks: [],
    outgoing: [],
    tags: [],
    file: true,
  };
}


export const filesHandlers = {
  file_image: (args) => {
    const target = args.target;
    return (attachments.get(target) ?? mockImage(target));
  },

  list_files: () => {
    return [...looseFiles.keys()].map(fileEntry);
  },

  open_file: (args) => {
    const path = args.path;
    if (!/\.(md|markdown|mdown)$/i.test(path)) throw coded("notMarkdown", `not a markdown file: ${path}`, { value: path });
    if (!looseFiles.has(path)) looseFiles.set(path, `# ${fileEntry(path).name}\n\nOpened in the mock.\n`);
    return { kind: "file", path };
  },

  take_opened_file: () => {
    return null;
  },

  load_file: (args) => {
    return fileView(args.path);
  },

  save_file: (args) => {
    const path = args.path;
    looseFiles.set(path, args.content);
    return { status: "saved", note: fileView(path) };
  },

  preview_file: (args) => {
    const path = args.path;
    return fileView(path).html;
  },

  forget_file: (args) => {
    looseFiles.delete(args.path);
    return [...looseFiles.keys()].map(fileEntry);
  },

  reveal_file: () => {
    return;
  },

  markdown_association: () => {
    return session.association;
  },

  claim_markdown: () => {
    session.association = { supported: true, current: "dev.kknow.desktop", ours: true };
    return session.association;
  },
} satisfies Handlers;
