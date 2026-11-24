import type { NoteSummary } from "./api";

export interface Folder {
  path: string;
  name: string;
  folders: Folder[];
  notes: NoteSummary[];
}

export function descend(root: Folder, folderPath: string): Folder {
  let node = root;
  for (const part of folderPath ? folderPath.split("/") : []) {
    let next = node.folders.find((folder) => folder.name === part);
    if (!next) {
      next = {
        path: node.path ? `${node.path}/${part}` : part,
        name: part,
        folders: [],
        notes: [],
      };
      node.folders.push(next);
    }
    node = next;
  }
  return node;
}

export function buildTree(
  items: NoteSummary[],
  folders: string[],
  collator: Intl.Collator,
): Folder {
  const root: Folder = { path: "", name: "", folders: [], notes: [] };

  for (const folder of folders) descend(root, folder);
  for (const note of items) descend(root, note.folder ?? "").notes.push(note);

  sortFolder(root, collator);
  return root;
}

export function fileName(note: NoteSummary): string {
  return note.id.slice(note.id.lastIndexOf("/") + 1);
}

export function sortFolder(folder: Folder, collator: Intl.Collator): void {
  folder.folders.sort((a, b) => collator.compare(a.name, b.name));
  folder.notes.sort((a, b) => collator.compare(fileName(a), fileName(b)));
  for (const child of folder.folders) sortFolder(child, collator);
}

export function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at > 0 ? path.slice(0, at) : "";
}

export function leafOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function join(folder: string, name: string): string {
  return folder ? `${folder}/${name}` : name;
}
