import { fileURLToPath } from "node:url";
import { scriptFiles } from "./comments.ts";

export const ROOT = fileURLToPath(new URL("../../", import.meta.url));

export const SCRIPT_DIRS = ["ui/src", "ui/tests", "ui/e2e", "ui/tools", "site/.vitepress"];

export const ROOT_SCRIPTS = ["ui/vite.config.ts", "ui/playwright.config.ts"];

export function checkedScripts(): string[] {
  return [...scriptFiles(ROOT, SCRIPT_DIRS), ...ROOT_SCRIPTS].filter(
    (path, index, all) => all.indexOf(path) === index,
  );
}
