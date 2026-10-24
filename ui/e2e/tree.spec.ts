import { expect, test } from "@playwright/test";

import { editorText, menuItem, noteRow, openRowMenu, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("a note is created, renamed, deleted and restored from the trash", async ({ page }) => {
  // Create: the sidebar button opens the name field.
  await page.locator("#new-note").click();
  await expect(page.locator("#new-note-form")).toBeVisible();
  await page.locator("#new-note-name").fill("Черновики/Из теста");
  await page.keyboard.press("Enter");
  await expect(statusMessage(page)).toHaveText(t("status.created"));
  await expect(page.locator("#note-title")).toHaveText("Из теста");
  await expect(noteRow(page, "Черновики/Из теста")).toHaveClass(/active/);

  // Rename in place: the input takes the file name, the folder stays.
  await openRowMenu(page, noteRow(page, "Черновики/Из теста"));
  await menuItem(page, t("commands.rename")).click();
  const input = page.locator("#note-list input.tree-rename");
  await expect(input).toHaveValue("Из теста");
  await input.fill("Переименована");
  await input.press("Enter");
  await expect(statusMessage(page)).toHaveText(t("status.renamed"));
  await expect(noteRow(page, "Черновики/Из теста")).toHaveCount(0);
  await expect(noteRow(page, "Черновики/Переименована")).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("Переименована");

  // Delete: the window asks first.
  page.once("dialog", async (dialog) => {
    expect(dialog.message()).toBe(t("confirm.deleteNote", { name: "Черновики/Переименована" }));
    await dialog.accept();
  });
  await openRowMenu(page, noteRow(page, "Черновики/Переименована"));
  await menuItem(page, t("commands.note.delete")).click();
  await expect(statusMessage(page)).toHaveText(t("status.noteDeleted"));
  await expect(noteRow(page, "Черновики/Переименована")).toHaveCount(0);
  await expect(page.locator("#note-title")).toHaveText(t("topbar.note.empty"));

  // The trash node lists it; a click shows it read-only, and "Restore" puts
  // it back and opens it.
  const trash = page.locator('.trash-row[data-vault="notes"] button.trash');
  await expect(trash.locator(".tree-count")).toHaveText("3");
  await trash.click();
  const entry = page.locator("button.trash-entry", { hasText: "Черновики/Переименована" });
  await entry.click();
  await expect(page.locator("#trash-view")).toBeVisible();
  await expect(page.locator("#trash-view-title")).toHaveText("Черновики/Переименована");
  await page.locator("#trash-view-restore").click();
  await expect(statusMessage(page)).toHaveText(t("status.trashRestored", { count: 1 }));
  await expect(page.locator("#trash-view")).toBeHidden();
  await expect(noteRow(page, "Черновики/Переименована")).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("Переименована");
  await expect(trash.locator(".tree-count")).toHaveText("2");
});

test("a folder collapses and a note opens in a collapsed branch", async ({ page }) => {
  const folder = page.locator('.folder-row[data-vault="notes"][data-path="Projects"] button.folder');
  await expect(folder).toHaveAttribute("aria-expanded", "true");
  await expect(noteRow(page, "Projects/kknow")).toBeVisible();

  await folder.click();
  await expect(folder).toHaveAttribute("aria-expanded", "false");
  await expect(noteRow(page, "Projects/kknow")).toHaveCount(0);

  // A link opens the note; the tree expands to show it.
  await noteRow(page, "Rust").click();
  await page.locator("#preview a", { hasText: "Projects/kknow" }).click();
  await expect(page.locator("#note-title")).toHaveText("kknow");
  await expect(folder).toHaveAttribute("aria-expanded", "true");
  await expect(noteRow(page, "Projects/kknow")).toHaveClass(/active/);
  await expect(editorText(page)).toContainText("Проект на");
});
