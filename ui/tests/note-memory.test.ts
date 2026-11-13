// The per-file mode and font size (`src/note-mode.ts`, `src/note-scale.ts`).
// Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { modeOf, moveFolderMode, rememberMode } from "../src/note-mode.ts";
import { moveFolderScale, scaleOf, stepScale } from "../src/note-scale.ts";

test("a folder rename moves the modes of the notes inside", () => {
  rememberMode("v/Projects/plan.md", "read");
  rememberMode("v/Projects/archive/old.md", "split");
  rememberMode("v/Projects-2/other.md", "read");

  moveFolderMode("v/Projects", "v/Work");

  assert.equal(modeOf("v/Work/plan.md", "edit"), "read");
  assert.equal(modeOf("v/Work/archive/old.md", "edit"), "split");
  assert.equal(modeOf("v/Projects/plan.md", "edit"), "edit");
  // A folder with the same start is a different folder.
  assert.equal(modeOf("v/Projects-2/other.md", "edit"), "read");
});

test("a folder rename moves the font sizes of the notes inside", () => {
  stepScale("v/Notes/long.md", 3);
  stepScale("v/Notes2/other.md", 3);

  moveFolderScale("v/Notes", "v/Archive/Notes");

  assert.equal(scaleOf("v/Archive/Notes/long.md"), 1.3);
  assert.equal(scaleOf("v/Notes/long.md"), 1);
  assert.equal(scaleOf("v/Notes2/other.md"), 1.3);
});
