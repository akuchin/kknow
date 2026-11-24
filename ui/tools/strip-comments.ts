import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { findComments, stripComments } from "./comments.ts";
import { ROOT, checkedScripts } from "./checked-scripts.ts";

let changed = 0;
for (const path of checkedScripts()) {
  const file = join(ROOT, path);
  const text = readFileSync(file, "utf8");
  const comments = findComments(text, path);
  if (comments.length === 0) continue;
  writeFileSync(file, stripComments(text, comments));
  changed++;
  console.log(path);
}
console.log(`comments removed from ${changed} files`);
