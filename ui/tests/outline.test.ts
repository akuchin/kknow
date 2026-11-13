// Tests for the note outline (`src/outline.ts`). Node runs them as is: it
// strips the types itself, and the module imports only packages. Run:
// `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

import { currentHeading, headingsOf } from "../src/outline.ts";

/** The same parser as in the editor: GFM. */
function outline(text: string) {
  return headingsOf(EditorState.create({ doc: text, extensions: markdown({ base: markdownLanguage }) }));
}

test("hash and underline headings, with levels and lines", () => {
  const text = [
    "# Project",
    "",
    "Text.",
    "",
    "## Goals ##",
    "",
    "Tasks",
    "======",
    "",
    "Deadlines",
    "-----",
    "",
    "###### Detail",
  ].join("\n");
  assert.deepEqual(outline(text), [
    { level: 1, text: "Project", line: 1 },
    { level: 2, text: "Goals", line: 5 },
    { level: 1, text: "Tasks", line: 7 },
    { level: 2, text: "Deadlines", line: 10 },
    { level: 6, text: "Detail", line: 13 },
  ]);
});

test("a hash in a code block is not a heading", () => {
  const text = ["# Plan", "", "```sh", "# comment", "```", "", "    # indent", "", "## Summary"].join("\n");
  assert.deepEqual(
    outline(text).map((heading) => heading.text),
    ["Plan", "Summary"],
  );
});

test("markup inside a heading does not get into the text", () => {
  assert.equal(outline("## **Important**: `code` and ~~old~~")[0].text, "Important: code and old");
});

test("the closing frontmatter line is not a heading", () => {
  const text = ["---", "title: Note", "---", "", "# Start"].join("\n");
  assert.deepEqual(outline(text), [{ level: 1, text: "Start", line: 5 }]);
});

test("a note without headings", () => {
  assert.deepEqual(outline("Just text.\n\n- item"), []);
  assert.deepEqual(outline(""), []);
});

test("the current heading is the last one at or above the top line", () => {
  const headings = [
    { level: 1, text: "Plan", line: 3 },
    { level: 2, text: "Goals", line: 10 },
    { level: 2, text: "Risks", line: 20 },
  ];
  assert.equal(currentHeading(headings, 1), -1);
  assert.equal(currentHeading(headings, 3), 0);
  assert.equal(currentHeading(headings, 15), 1);
  assert.equal(currentHeading(headings, 99), 2);
  assert.equal(currentHeading([], 5), -1);
  // At the end of the note the last heading on the screen wins.
  assert.equal(currentHeading(headings, 15, 25), 2);
});
