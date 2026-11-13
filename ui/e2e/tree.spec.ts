import { expect, test } from "@playwright/test";

import { editorText, menuItem, noteRow, openRowMenu, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("a note is created, renamed, deleted and restored from the trash", async ({ page }) => {
  // Create: the sidebar button opens the name field.
  await page.locator("#new-note").click();
  await expect(page.locator("#new-note-form")).toBeVisible();
  await page.locator("#new-note-name").fill("Drafts/From the test");
  await page.keyboard.press("Enter");
  await expect(statusMessage(page)).toHaveText(t("status.created"));
  await expect(page.locator("#note-title")).toHaveText("From the test");
  await expect(noteRow(page, "Drafts/From the test")).toHaveClass(/active/);

  // Rename in place: the input takes the file name, the folder stays.
  await openRowMenu(page, noteRow(page, "Drafts/From the test"));
  await menuItem(page, t("commands.rename")).click();
  const input = page.locator("#note-list input.tree-rename");
  await expect(input).toHaveValue("From the test");
  await input.fill("Renamed");
  await input.press("Enter");
  await expect(statusMessage(page)).toHaveText(t("status.renamed"));
  await expect(noteRow(page, "Drafts/From the test")).toHaveCount(0);
  await expect(noteRow(page, "Drafts/Renamed")).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("Renamed");

  // Delete: the window asks first.
  page.once("dialog", async (dialog) => {
    expect(dialog.message()).toBe(t("confirm.deleteNote", { name: "Drafts/Renamed" }));
    await dialog.accept();
  });
  await openRowMenu(page, noteRow(page, "Drafts/Renamed"));
  await menuItem(page, t("commands.note.delete")).click();
  await expect(statusMessage(page)).toHaveText(t("status.noteDeleted"));
  await expect(noteRow(page, "Drafts/Renamed")).toHaveCount(0);
  await expect(page.locator("#note-title")).toHaveText(t("topbar.note.empty"));

  // The trash node lists it; a click shows it read-only, and "Restore" puts
  // it back and opens it.
  const trash = page.locator('.trash-row[data-vault="notes"] button.trash');
  await expect(trash.locator(".tree-count")).toHaveText("3");
  await trash.click();
  const entry = page.locator("button.trash-entry", { hasText: "Drafts/Renamed" });
  await entry.click();
  await expect(page.locator("#trash-view")).toBeVisible();
  await expect(page.locator("#trash-view-title")).toHaveText("Drafts/Renamed");
  await page.locator("#trash-view-restore").click();
  await expect(statusMessage(page)).toHaveText(t("status.trashRestored", { count: 1 }));
  await expect(page.locator("#trash-view")).toBeHidden();
  await expect(noteRow(page, "Drafts/Renamed")).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("Renamed");
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
  await expect(editorText(page)).toContainText("A project in");
});
