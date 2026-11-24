import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { findComments, lineOf, stripComments } from "../tools/comments.ts";
import { ROOT, checkedScripts } from "../tools/checked-scripts.ts";

function found(text: string): string[] {
  return findComments(text).map(({ start, end }) => text.slice(start, end));
}

function strip(text: string): string {
  return stripComments(text, findComments(text));
}

test("the scripts have no comments", () => {
  const offenders = checkedScripts().flatMap((path) => {
    const text = readFileSync(join(ROOT, path), "utf8");
    return findComments(text, path).map(({ start }) => `${path}:${lineOf(text, start)}`);
  });
  assert.deepEqual(
    offenders,
    [],
    "comments are not allowed; express the intent in code or in ARCHITECTURE.md. `pnpm comments:strip` removes them.",
  );
});

test("finds line, block and JSDoc comments", () => {
  const text = "/** doc */\nconst a = 1; // tail\n/* block */ const b = 2;\n";
  assert.deepEqual(found(text), ["/** doc */", "// tail", "/* block */"]);
});

test("ignores comment markers inside strings, templates and regular expressions", () => {
  const text = 'const a = "http://x";\nconst b = `/* ${a} // */`;\nconst c = /\\/\\//g;\nconst d = 4 / 2 / 1;\n';
  assert.deepEqual(found(text), []);
});

test("keeps compiler directives", () => {
  const text = '/// <reference types="vite/client" />\n// @ts-expect-error: wrong on purpose\nconst a: number = "x";\nimport(/* @vite-ignore */ a);\n';
  assert.deepEqual(found(text), []);
});

test("strip removes whole lines and the blank line they leave", () => {
  const text = "// header\n\nimport x from \"x\";\n\n/** doc */\nfunction a() {\n  // inside\n\n  b(); // tail\n}\n";
  assert.equal(strip(text), "import x from \"x\";\n\nfunction a() {\n  b();\n}\n");
});

test("strip keeps code around inline block comments", () => {
  assert.equal(strip("call(/* a */ x, y /* b */);\n"), "call(x, y);\n");
});
