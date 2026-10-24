/**
 * Shared steps of the smoke tests: open the window, find a tree row, read the
 * status bar.
 *
 * Tests find elements by the ids and classes of the markup and by the visible
 * strings. The strings come from `locales/ru.json`, so a changed wording does
 * not break a test, but a lost string does.
 */
import { expect, type Locator, type Page } from "@playwright/test";

import ru from "../../locales/ru.json" with { type: "json" };

const strings: Record<string, string> = ru;
const plurals = new Intl.PluralRules("ru");

/** The same lookup as `i18n.ts`: `{{name}}` substitution and plural suffixes. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const count = params.count;
  const text =
    (typeof count === "number" ? strings[`${key}_${plurals.select(count)}`] : undefined) ??
    strings[`${key}_other`] ??
    strings[key];
  if (text === undefined) throw new Error(`no string ${key} in ru.json`);
  return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(params[name] ?? ""));
}

/** Opens the main window and waits for the end of boot: the first note is open. */
export async function openWindow(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(noteRow(page, "Rust")).toBeVisible();
}

/** The button of a note row in the tree. */
export function noteRow(page: Page, id: string, vault = "notes"): Locator {
  return page.locator(
    `.note-row[data-vault="${vault}"][data-id="${id}"] button.note`,
  );
}

/** The message segment of the status bar. */
export function statusMessage(page: Page): Locator {
  return page.locator("#status-message");
}

/** The text of the editor, line by line, as CodeMirror shows it. */
export function editorText(page: Page): Locator {
  return page.locator("#editor .cm-content");
}

/** An item of the open menu, by its label. */
export function menuItem(page: Page, label: string): Locator {
  return page.locator(".menu .menu-item", { hasText: label });
}

/** Opens the "⋯" menu of a tree row. The button shows on hover. */
export async function openRowMenu(page: Page, row: Locator): Promise<void> {
  const line = row.locator("xpath=..");
  await line.hover();
  await line.locator(".row-menu").click();
  await expect(page.locator(".menu")).toBeVisible();
}
