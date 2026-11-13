// Error texts from coded core errors. Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { errorText, type Translate } from "../src/error-view.ts";

const english: Translate = (key, params) =>
  key === "error.notFound" ? `note not found: ${params.value}` : undefined;

test("a known code is translated with its values", () => {
  const error = { code: "notFound", params: { value: "plan" }, message: "note not found: plan" };
  assert.equal(errorText(error, english), "note not found: plan");
});

test("an unknown code falls back to the core text", () => {
  const error = { code: "brandNew", params: {}, message: "a brand new error" };
  assert.equal(errorText(error, english), "a brand new error");
});

test("a thrown Error and a string keep their text", () => {
  assert.equal(errorText(new Error("it broke"), english), "it broke");
  assert.equal(errorText("a string", english), "a string");
});
