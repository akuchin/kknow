// The sidebar tree as data (`src/tree-model.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import type { NoteSummary } from "../src/api.ts";
import {
  buildTree,
  descend,
  fileName,
  folderOf,
  join,
  leafOf,
  sortFolder,
  type Folder,
} from "../src/tree-model.ts";

const collator = new Intl.Collator("ru");

/** A note at the id; the folder is the part before the last slash. */
function note(id: string, title = id): NoteSummary {
  const slash = id.lastIndexOf("/");
  return { id, title, folder: slash < 0 ? null : id.slice(0, slash), tags: [] };
}

/** The tree as nested names: folders end with `/`. */
function shape(folder: Folder): unknown[] {
  return [
    ...folder.folders.map((child) => ({ [`${child.name}/`]: shape(child) })),
    ...folder.notes.map(fileName),
  ];
}

test("a note path becomes folders and a leaf", () => {
  const root = buildTree([note("Projects/Archive/Ideas")], [], collator);
  assert.deepEqual(shape(root), [{ "Projects/": [{ "Archive/": ["Ideas"] }] }]);
  assert.equal(root.folders[0].folders[0].path, "Projects/Archive");
});

test("notes without a folder stay at the root", () => {
  const root = buildTree([note("Inbox")], [], collator);
  assert.equal(root.path, "");
  assert.deepEqual(shape(root), ["Inbox"]);
});

test("an empty folder from the folder list is in the tree", () => {
  const root = buildTree([note("Note")], ["Empty", "Work/New"], collator);
  assert.deepEqual(shape(root), [
    { "Empty/": [] },
    { "Work/": [{ "New/": [] }] },
    "Note",
  ]);
});

test("a folder from both lists appears once", () => {
  const root = buildTree([note("Work/Plan")], ["Work"], collator);
  assert.deepEqual(shape(root), [{ "Work/": ["Plan"] }]);
});

test("folders come first, then notes, each in collator order", () => {
  const root = buildTree(
    [note("ёлка"), note("Яблоко"), note("арбуз"), note("Банан/б"), note("Банан/а")],
    ["Ель", "Абрикос"],
    collator,
  );
  assert.deepEqual(shape(root), [
    { "Абрикос/": [] },
    { "Банан/": ["а", "б"] },
    { "Ель/": [] },
    "арбуз",
    "ёлка",
    "Яблоко",
  ]);
});

test("notes sort by file name, not by title", () => {
  const root = buildTree([note("b", "Alpha"), note("a", "Omega")], [], collator);
  assert.deepEqual(shape(root), ["a", "b"]);
});

test("fileName is the last segment of the id", () => {
  assert.equal(fileName(note("a/b/c")), "c");
  assert.equal(fileName(note("c")), "c");
});

test("descend reuses existing folders and creates missing ones", () => {
  const root: Folder = { path: "", name: "", folders: [], notes: [] };
  const deep = descend(root, "a/b");
  assert.equal(descend(root, "a/b"), deep);
  assert.equal(descend(root, ""), root);
  assert.deepEqual(shape(root), [{ "a/": [{ "b/": [] }] }]);
});

test("sortFolder sorts nested folders too", () => {
  const root: Folder = { path: "", name: "", folders: [], notes: [] };
  descend(root, "c/z");
  descend(root, "c/a");
  descend(root, "b");
  sortFolder(root, collator);
  assert.deepEqual(shape(root), [{ "b/": [] }, { "c/": [{ "a/": [] }, { "z/": [] }] }]);
});

test("path helpers split and join folder paths", () => {
  assert.equal(folderOf("a/b/c"), "a/b");
  assert.equal(folderOf("c"), "");
  assert.equal(leafOf("a/b/c"), "c");
  assert.equal(leafOf("c"), "c");
  assert.equal(join("a/b", "c"), "a/b/c");
  assert.equal(join("", "c"), "c");
});
