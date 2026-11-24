import { expect, type Locator, type Page } from "@playwright/test";

import ru from "../../locales/ru.json" with { type: "json" };
import { fill, lookup, type Dict, type Params } from "../src/translate";

const dictionaries = [ru as Dict];
const plurals = new Intl.PluralRules("ru");

export function t(key: string, params: Params = {}): string {
  const text = lookup(dictionaries, plurals, key, params);
  if (text === undefined) throw new Error(`no string ${key} in ru.json`);
  return fill(text, params);
}

export async function openWindow(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(noteRow(page, "Rust")).toBeVisible();
}

export function noteRow(page: Page, id: string, vault = "notes"): Locator {
  return page.locator(
    `.note-row[data-vault="${vault}"][data-id="${id}"] button.note`,
  );
}

export function statusMessage(page: Page): Locator {
  return page.locator("#status-message");
}

export function editorText(page: Page): Locator {
  return page.locator("#editor .cm-content");
}

export function menuItem(page: Page, label: string): Locator {
  return page.locator(".menu .menu-item", { hasText: label });
}

export async function openRowMenu(page: Page, row: Locator): Promise<void> {
  const line = row.locator("xpath=..");
  await line.hover();
  await line.locator(".row-menu").click();
  await expect(page.locator(".menu")).toBeVisible();
}
