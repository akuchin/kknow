// Tests for link completion (`src/link-complete.ts`) and for the link under
// the cursor (`src/editor-links.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

import { linkAt } from "../src/editor-links.ts";
import { headingsOf } from "../src/outline.ts";
import {
  blocksOf,
  linkCompletion,
  linkQuery,
  linkText,
  type LinkNote,
} from "../src/link-complete.ts";

/** The same parser as in the editor: GFM. */
function stateOf(doc: string): EditorState {
  return EditorState.create({ doc, extensions: markdown({ base: markdownLanguage }) });
}

const notes: LinkNote[] = [
  { id: "Projects/Plan", title: "The plan", folder: "Projects", tags: ["work"] },
  { id: "Inbox", title: "Inbox", folder: null, tags: ["work", "todo/later"] },
  { id: "Archive/Inbox", title: "Old inbox", folder: "Archive", tags: [] },
];

test("[[ asks for a note, # for a heading, #^ for a block", () => {
  assert.deepEqual(linkQuery("see [[Pl", 10), { kind: "note", from: 16, query: "Pl" });
  assert.deepEqual(linkQuery("[[Plan#Ne", 0), {
    kind: "heading",
    from: 7,
    target: "Plan",
    query: "Ne",
  });
  assert.deepEqual(linkQuery("[[Plan#A#B", 0), {
    kind: "heading",
    from: 9,
    target: "Plan",
    query: "B",
  });
  assert.deepEqual(linkQuery("[[#^sh", 0), { kind: "block", from: 4, target: "", query: "sh" });
});

test("a closed link, an alias and plain text ask for nothing", () => {
  assert.equal(linkQuery("[[Plan]] and", 0), null);
  assert.equal(linkQuery("[[Plan|the pl", 0), null);
  assert.equal(linkQuery("just text", 0), null);
});

test("#word asks for a tag; a heading mark and a URL anchor do not", () => {
  assert.deepEqual(linkQuery("note #wo", 0), { kind: "tag", from: 6, query: "wo" });
  assert.deepEqual(linkQuery("#todo/la", 0), { kind: "tag", from: 1, query: "todo/la" });
  assert.equal(linkQuery("# ", 0), null);
  assert.equal(linkQuery("https://x.dev/#top", 0), null);
});

test("a link uses the file name, or the full path when the name repeats", () => {
  assert.equal(linkText("Projects/Plan", notes), "Plan");
  assert.equal(linkText("Inbox", notes), "Inbox");
  assert.equal(linkText("Archive/Inbox", notes), "Archive/Inbox");
});

test("block IDs come from the line ends", () => {
  assert.deepEqual(blocksOf("Text ^one\nx^2\n- item ^two-b\n"), ["one", "two-b"]);
});

async function complete(doc: string, pos = doc.length): Promise<CompletionResult | null> {
  const source = linkCompletion({
    notes: () => notes,
    anchors: async (target) =>
      target === "Plan" ? { headings: [{ level: 2, text: "Next steps" }], blocks: ["ship"] } : null,
    headings: headingsOf,
  });
  return source(new CompletionContext(stateOf(doc), pos, false));
}

test("the completion lists notes after [[", async () => {
  const result = await complete("see [[");
  assert.equal(result?.from, 6);
  assert.deepEqual(
    result?.options.map((option) => option.label),
    ["Plan", "Inbox", "Inbox"],
  );
});

test("the completion lists headings of another note and of this one", async () => {
  const other = await complete("[[Plan#");
  assert.deepEqual(other?.options.map((option) => option.label), ["Next steps"]);
  const own = await complete("# Top\n\n## Second\n\n[[#");
  assert.deepEqual(own?.options.map((option) => option.label), ["Top", "Second"]);
});

test("the completion lists the tags of the vault without repeats", async () => {
  const result = await complete("text #t");
  assert.deepEqual(result?.options.map((option) => option.label), ["todo/later", "work"]);
});

test("the completion stays silent in code", async () => {
  assert.equal(await complete("`[[`", 3), null);
  assert.equal(await complete("```\n[[\n```", 6), null);
});

test("the link under the cursor: wiki, with subpath, without alias", () => {
  const state = stateOf("See [[Plan#Next|the plan]] now");
  assert.equal(linkAt(state, 8), "Plan#Next");
  assert.equal(linkAt(state, 1), null);
});

test("the link under the cursor: a Markdown link to a note", () => {
  const state = stateOf("A [plan](Projects/My%20plan.md#goals) and [web](https://x.dev)");
  assert.equal(linkAt(state, 4), "Projects/My plan.md#goals");
  assert.equal(linkAt(state, 45), null);
});

test("an embedded image and a link in code are no notes", () => {
  assert.equal(linkAt(stateOf("![[shot.png]]"), 4), null);
  assert.equal(linkAt(stateOf("![[Plan]]"), 4), "Plan");
  assert.equal(linkAt(stateOf("`[[Plan]]`"), 4), null);
});
