import { expect, test } from "@playwright/test";

import { editorText, noteRow, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("a standalone file from the list opens, saves and gives way to a note", async ({ page }) => {
  const file = page.locator('#file-list button.note[data-path="/Users/me/src/kknow/README.md"]');
  await file.click();
  await expect(page.locator("#note-title")).toHaveText("README");
  await expect(page.locator("#note-folder-text")).toHaveText("kknow");
  await expect(file).toHaveClass(/active/);
  await expect(editorText(page)).toContainText("Заметочник на Rust");

  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("Правка файла");
  await expect(statusMessage(page)).toHaveText(t("status.saved"));

  await noteRow(page, "Rust").click();
  await expect(file).not.toHaveClass(/active/);
  await file.click();
  await expect(editorText(page)).toContainText("Правка файла");
});

test("a file picked in the dialog joins the list and opens", async ({ page }) => {
  // The mock dialog is a prompt with the path.
  page.once("dialog", (dialog) => dialog.accept("/Users/me/Desktop/черновик.md"));
  await page.locator("#open-file").click();

  const file = page.locator('#file-list button.note[data-path="/Users/me/Desktop/черновик.md"]');
  await expect(file).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("черновик");
  await expect(editorText(page)).toContainText("Открыт в заглушке");
});
