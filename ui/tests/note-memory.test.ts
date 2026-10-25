// The per-file mode and font size (`src/note-mode.ts`, `src/note-scale.ts`).
// Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { modeOf, moveFolderMode, rememberMode } from "../src/note-mode.ts";
import { moveFolderScale, scaleOf, stepScale } from "../src/note-scale.ts";

test("a folder rename moves the modes of the notes inside", () => {
  rememberMode("v/Проекты/план.md", "read");
  rememberMode("v/Проекты/архив/старое.md", "split");
  rememberMode("v/Проекты-2/другое.md", "read");

  moveFolderMode("v/Проекты", "v/Работа");

  assert.equal(modeOf("v/Работа/план.md", "edit"), "read");
  assert.equal(modeOf("v/Работа/архив/старое.md", "edit"), "split");
  assert.equal(modeOf("v/Проекты/план.md", "edit"), "edit");
  // A folder with the same start is a different folder.
  assert.equal(modeOf("v/Проекты-2/другое.md", "edit"), "read");
});

test("a folder rename moves the font sizes of the notes inside", () => {
  stepScale("v/Заметки/длинная.md", 3);
  stepScale("v/Заметки2/чужая.md", 3);

  moveFolderScale("v/Заметки", "v/Архив/Заметки");

  assert.equal(scaleOf("v/Архив/Заметки/длинная.md"), 1.3);
  assert.equal(scaleOf("v/Заметки/длинная.md"), 1);
  assert.equal(scaleOf("v/Заметки2/чужая.md"), 1.3);
});
