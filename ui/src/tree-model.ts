/**
 * The sidebar tree as data: folders and notes from the flat note list.
 *
 * This module has no imports except types, so the tests run it in Node
 * without Vite. The caller passes the collator, because the window language
 * comes from `i18n.ts`, which needs Vite.
 */

import type { NoteSummary } from "./api";

/** Tree node: a folder with its subfolders and notes. */
export interface Folder {
  /** Path from the root. It is also the key for the collapsed state. */
  path: string;
  name: string;
  folders: Folder[];
  notes: NoteSummary[];
}

/** Walks down to the folder at the path and creates missing nodes. */
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

/**
 * Builds the tree from note paths: `Projects/Archive/Ideas` is two folders and
 * a leaf.
 *
 * Folders also come as a separate list: notes cannot produce an empty folder,
 * but it must be visible because the user just created it to fill it.
 * The collator gives the order of names: the window passes one for its
 * language.
 */
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

/** Note file name: the last segment of the id, without folders. */
export function fileName(note: NoteSummary): string {
  return note.id.slice(note.id.lastIndexOf("/") + 1);
}

/**
 * Folders come before notes: first the structure, then the content.
 * Notes sort by file name, not title, so the order matches the folder on disk.
 */
export function sortFolder(folder: Folder, collator: Intl.Collator): void {
  folder.folders.sort((a, b) => collator.compare(a.name, b.name));
  folder.notes.sort((a, b) => collator.compare(fileName(a), fileName(b)));
  for (const child of folder.folders) sortFolder(child, collator);
}
