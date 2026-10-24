import { expect, test } from "@playwright/test";

import { editorText, noteRow, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("an edit is saved and stays after the note opens again", async ({ page }) => {
  await noteRow(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
  await expect(editorText(page)).toContainText("Времена жизни");

  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("Новая строка из теста");
  await expect(statusMessage(page)).toHaveText(t("status.notSaved"));
  await expect(statusMessage(page)).toHaveText(t("status.saved"));

  // Another note and back: the text comes from the mock core again.
  await noteRow(page, "Index").click();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(editorText(page)).not.toContainText("Новая строка из теста");
  await noteRow(page, "Rust").click();
  await expect(editorText(page)).toContainText("Новая строка из теста");
});

test("back and forward walk the opened notes", async ({ page }) => {
  const back = page.locator("#nav-back");
  const forward = page.locator("#nav-forward");
  const title = page.locator("#note-title");
  await expect(back).toBeDisabled();
  await expect(forward).toBeDisabled();

  await noteRow(page, "Rust").click();
  await expect(title).toHaveText("Rust");
  await noteRow(page, "Ветки").click();
  await expect(title).toHaveText("Ветки");

  await back.click();
  await expect(title).toHaveText("Rust");
  await back.click();
  await expect(title).toHaveText("Index");
  await expect(back).toBeDisabled();

  await forward.click();
  await expect(title).toHaveText("Rust");
  // The keys do the same as the buttons.
  await page.keyboard.press("ControlOrMeta+Alt+ArrowRight");
  await expect(title).toHaveText("Ветки");
  await expect(forward).toBeDisabled();
  await page.keyboard.press("ControlOrMeta+Alt+ArrowLeft");
  await expect(title).toHaveText("Rust");
});

test("a link in the preview opens its note", async ({ page }) => {
  await page.locator("#preview a", { hasText: "Rust" }).click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
  await expect(page.locator("#backlink-list")).toContainText("Оглавление");
});

test("the mode switch shows the editor, both panes or the preview", async ({ page }) => {
  const app = page.locator("#app");
  const pressed = (mode: string) => page.locator(`.mode-switch button[data-mode="${mode}"]`);
  await expect(app).toHaveAttribute("data-mode", "split");
  await expect(page.locator("#editor")).toBeVisible();
  await expect(page.locator("#preview")).toBeVisible();

  await pressed("read").click();
  await expect(app).toHaveAttribute("data-mode", "read");
  await expect(pressed("read")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#editor")).toBeHidden();
  await expect(page.locator("#preview h1")).toHaveText("Оглавление");

  await pressed("edit").click();
  await expect(app).toHaveAttribute("data-mode", "edit");
  await expect(page.locator("#editor")).toBeVisible();
  await expect(page.locator(".preview-pane")).toBeHidden();

  // The mode is kept for the note: another note gets the default, this one
  // gets its own back.
  await noteRow(page, "Rust").click();
  await expect(app).toHaveAttribute("data-mode", "split");
  await noteRow(page, "Index").click();
  await expect(app).toHaveAttribute("data-mode", "edit");
});

test("the font size of a note changes from the status bar", async ({ page }) => {
  const value = page.locator("#status-scale-value");
  await expect(value).toHaveText("100%");

  await page.locator("#status-scale-larger").click();
  await expect(value).not.toHaveText("100%");
  const larger = await value.textContent();
  expect(parseInt(larger ?? "", 10)).toBeGreaterThan(100);
  await expect(page.locator("#preview")).toHaveCSS("font-size", /px$/);
  expect(await page.locator("#preview").evaluate((node) => node.style.fontSize)).toBe(larger);

  // The size belongs to the note: another note has the default size.
  await noteRow(page, "Rust").click();
  await expect(value).toHaveText("100%");
  await noteRow(page, "Index").click();
  await expect(value).toHaveText(larger!);

  // A click on the value resets it.
  await value.click();
  await expect(value).toHaveText("100%");
});
