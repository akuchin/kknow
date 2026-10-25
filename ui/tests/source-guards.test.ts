// Guards for the layout of the frontend code in `ui/src`: the rules from
// `docs/refactor-ui-2026.md` that the type system does not check.
// Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

/** The TypeScript files of `ui/src`, as paths relative to it. */
function sources(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((path) => path.endsWith(".ts"))
    .sort();
}

function read(path: string): string {
  return readFileSync(join(SRC, path), "utf8");
}

/** `import`/`export … from` statements and bare `import "…"`. */
const STATEMENT = /^\s*(?:import|export)\s+(?:([^'";]*?)\s+from\s+)?["']([^"']+)["']/gms;

/**
 * True when the build removes the statement: `import type …`, or a list of
 * `type` names only. Such an import does not run the other module.
 */
function typeOnly(clause: string): boolean {
  if (/^type\s/.test(clause)) return true;
  const names = /^\{([^}]*)\}$/.exec(clause.trim());
  if (!names) return false;
  const parts = names[1].split(",").map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 && parts.every((part) => part.startsWith("type "));
}

/** The modules of `ui/src` that the file imports at run time. */
function importsOf(path: string, text: string): string[] {
  const result: string[] = [];
  for (const [, clause, specifier] of text.matchAll(STATEMENT)) {
    if (!specifier.startsWith(".") || (clause && typeOnly(clause))) continue;
    let target = resolve(SRC, dirname(path), specifier);
    if (!target.endsWith(".ts")) target += ".ts";
    if (existsSync(target)) result.push(relative(SRC, target));
  }
  return result;
}

/** Each cycle in the graph once, as the path that closes it. */
function findCycles(graph: Map<string, string[]>): string[][] {
  const cycles: string[][] = [];
  const done = new Set<string>();
  const stack: string[] = [];
  const visit = (node: string): void => {
    const at = stack.indexOf(node);
    if (at >= 0) {
      cycles.push([...stack.slice(at), node]);
      return;
    }
    if (done.has(node)) return;
    stack.push(node);
    for (const next of graph.get(node) ?? []) visit(next);
    stack.pop();
    done.add(node);
  };
  for (const node of graph.keys()) visit(node);
  return cycles;
}

test("findCycles finds a cycle and accepts a tree", () => {
  const cyclic = new Map([
    ["a", ["b"]],
    ["b", ["c"]],
    ["c", ["a"]],
  ]);
  assert.deepEqual(findCycles(cyclic), [["a", "b", "c", "a"]]);
  const tree = new Map([
    ["a", ["b", "c"]],
    ["b", ["c"]],
    ["c", []],
  ]);
  assert.deepEqual(findCycles(tree), []);
});

test("importsOf skips type imports and packages", () => {
  const text = [
    'import type { A } from "./api";',
    'import { type B, type C } from "./api";',
    'import { el, type D } from "./window/dom";',
    'import {\n  state,\n} from "./window/state.ts";',
    'import "./styles.css";',
    'import { EditorView } from "@codemirror/view";',
    'export { status } from "./window/status";',
  ].join("\n");
  assert.deepEqual(importsOf("main.ts", text), [
    "window/dom.ts",
    "window/state.ts",
    "window/status.ts",
  ]);
});

test("ui/src has no import cycles", () => {
  const graph = new Map(sources().map((path) => [path, importsOf(path, read(path))]));
  const cycles = findCycles(graph).map((cycle) => cycle.join(" → "));
  assert.deepEqual(cycles, [], "a lower layer must not import a higher one");
});

const MAX_LINES = 800;

/**
 * Files that may be longer than MAX_LINES. Each one was already this long
 * before the refactor of the window code, or the build writes it.
 */
const LONG_FILES: Record<string, string> = {
  "bindings.ts": "generated from the Rust types; nobody edits it",
  "table-widget.ts": "one widget: the drawn table and its cell editor",
  "live-preview.ts": "one CodeMirror extension: the decorations of live preview",
  "mock/core.ts": "the stubs of all Tauri commands for `pnpm ui:mock`",
};

function lines(text: string): number {
  return text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
}

test(`each file in ui/src has at most ${MAX_LINES} lines`, () => {
  const long = sources()
    .filter((path) => !(path in LONG_FILES))
    .map((path) => ({ path, lines: lines(read(path)) }))
    .filter((file) => file.lines > MAX_LINES)
    .map((file) => `${file.path}: ${file.lines}`);
  assert.deepEqual(long, [], "split the file into modules");
});

test("the list of long files names only files that exist", () => {
  const all = new Set(sources());
  assert.deepEqual(Object.keys(LONG_FILES).filter((path) => !all.has(path)), []);
});
