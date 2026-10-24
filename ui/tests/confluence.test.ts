// Tests for the Confluence page comment (`src/confluence-field.ts`). Run:
// `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  cleanPageId,
  formatPageComment,
  pageCommentPlace,
  parsePageComment,
} from "../src/confluence-field.ts";

test("комментарий со страницей разбирается", () => {
  assert.equal(parsePageComment("<!-- confluence: 987654321  -->"), "987654321");
  assert.equal(parsePageComment("<!--confluence:42-->"), "42");
  assert.equal(parsePageComment('<!-- confluence: "42" -->'), "42");
  assert.equal(parsePageComment("<!-- confluence: -->"), "");
});

test("другие комментарии — не страница", () => {
  assert.equal(parsePageComment("<!-- @agent: сократить -->"), null);
  assert.equal(parsePageComment("<!-- page: 42 -->"), null);
  assert.equal(parsePageComment("<!--\n  confluence: 42\n  space: DOCS\n-->"), null);
});

test("из ссылки берётся номер страницы", () => {
  assert.equal(cleanPageId("  123 "), "123");
  assert.equal(cleanPageId("https://wiki.example/spaces/DOCS/pages/4455/Заметка"), "4455");
  assert.equal(cleanPageId("https://wiki.example/pages/viewpage.action?pageId=777"), "777");
  assert.equal(cleanPageId("12-->34"), "1234");
});

test("комментарий собирается обратно", () => {
  const comment = formatPageComment("42");
  assert.equal(comment, "<!-- confluence: 42 -->");
  assert.equal(parsePageComment(comment), "42");
});

test("новый комментарий идёт после frontmatter", () => {
  assert.equal(pageCommentPlace("# Заголовок\n"), 0);
  const front = "---\ntags: [a]\n---\n";
  assert.equal(pageCommentPlace(`${front}# Заголовок\n`), front.length);
  assert.equal(pageCommentPlace("---\ntags: [a]\n---"), 17);
});
