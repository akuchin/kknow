// Tests for the note outline (`src/outline.ts`). Node runs them as is: it
// strips the types itself, and the module imports only packages. Run:
// `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";

import { headingsOf } from "../src/outline.ts";

/** The same parser as in the editor: GFM. */
function outline(text: string) {
  return headingsOf(EditorState.create({ doc: text, extensions: markdown({ base: markdownLanguage }) }));
}

test("заголовки с решётками и подчёркиванием, с уровнями и строками", () => {
  const text = [
    "# Проект",
    "",
    "Текст.",
    "",
    "## Цели ##",
    "",
    "Задачи",
    "======",
    "",
    "Сроки",
    "-----",
    "",
    "###### Мелочь",
  ].join("\n");
  assert.deepEqual(outline(text), [
    { level: 1, text: "Проект", line: 1 },
    { level: 2, text: "Цели", line: 5 },
    { level: 1, text: "Задачи", line: 7 },
    { level: 2, text: "Сроки", line: 10 },
    { level: 6, text: "Мелочь", line: 13 },
  ]);
});

test("решётка в блоке кода — не заголовок", () => {
  const text = ["# План", "", "```sh", "# комментарий", "```", "", "    # отступ", "", "## Итог"].join("\n");
  assert.deepEqual(
    outline(text).map((heading) => heading.text),
    ["План", "Итог"],
  );
});

test("разметка внутри заголовка не попадает в текст", () => {
  assert.equal(outline("## **Важно**: `код` и ~~старое~~")[0].text, "Важно: код и старое");
});

test("закрывающая черта frontmatter — не заголовок", () => {
  const text = ["---", "title: Заметка", "---", "", "# Начало"].join("\n");
  assert.deepEqual(outline(text), [{ level: 1, text: "Начало", line: 5 }]);
});

test("заметка без заголовков", () => {
  assert.deepEqual(outline("Просто текст.\n\n- пункт"), []);
  assert.deepEqual(outline(""), []);
});
