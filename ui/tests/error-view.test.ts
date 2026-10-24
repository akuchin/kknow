// Error texts from coded core errors. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { errorText, type Translate } from "../src/error-view.ts";

const english: Translate = (key, params) =>
  key === "error.notFound" ? `note not found: ${params.value}` : undefined;

test("a known code is translated with its values", () => {
  const error = { code: "notFound", params: { value: "план" }, message: "заметка не найдена: план" };
  assert.equal(errorText(error, english), "note not found: план");
});

test("an unknown code falls back to the Russian text", () => {
  const error = { code: "brandNew", params: {}, message: "новая ошибка" };
  assert.equal(errorText(error, english), "новая ошибка");
});

test("a thrown Error and a string keep their text", () => {
  assert.equal(errorText(new Error("сломалось"), english), "сломалось");
  assert.equal(errorText("строка", english), "строка");
});
