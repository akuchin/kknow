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
  await expect(editorText(page)).toContainText("A notes app in Rust");

  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("A file edit");
  await expect(statusMessage(page)).toHaveText(t("status.saved"));

  await noteRow(page, "Rust").click();
  await expect(file).not.toHaveClass(/active/);
  await file.click();
  await expect(editorText(page)).toContainText("A file edit");
});

test("a file picked in the dialog joins the list and opens", async ({ page }) => {
  page.once("dialog", (dialog) => dialog.accept("/Users/me/Desktop/draft.md"));
  await page.locator("#open-file").click();

  const file = page.locator('#file-list button.note[data-path="/Users/me/Desktop/draft.md"]');
  await expect(file).toHaveClass(/active/);
  await expect(page.locator("#note-title")).toHaveText("draft");
  await expect(editorText(page)).toContainText("Opened in the mock");
});

test("the Files toggle folds the list and keeps the choice", async ({ page }) => {
  const toggle = page.locator("#files-toggle");
  const list = page.locator("#file-list");
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(list).toBeVisible();
  await expect(toggle).toHaveAttribute("title", t("sidebar.files.collapse"));

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(list).toBeHidden();
  await expect(toggle).toHaveAttribute("title", t("sidebar.files.expand"));
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator('#file-list button.note[data-path="/Users/me/src/kknow/README.md"]')).toBeVisible();
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
});

test("a folded Files list gives its height to the tree", async ({ page }) => {
  const tree = page.locator("#note-list");
  const open = (await tree.boundingBox())!.height;
  await page.locator("#files-toggle").click();
  await expect.poll(async () => (await tree.boundingBox())!.height).toBeGreaterThan(open);
});

test("the Files toggle works from the keyboard", async ({ page }) => {
  const toggle = page.locator("#files-toggle");
  await toggle.focus();
  await page.keyboard.press("Enter");
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await page.keyboard.press("Space");
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
});

test("the Files count stays visible when folded", async ({ page }) => {
  const rows = await page.locator("#file-list > li").count();
  await expect(page.locator("#files-count")).toHaveText(String(rows));
  await page.locator("#files-toggle").click();
  await expect(page.locator("#files-count")).toBeVisible();
  await expect(page.locator("#files-count")).toHaveText(String(rows));
});

test("a file picked in the dialog opens a folded Files list", async ({ page }) => {
  await page.locator("#files-toggle").click();
  page.once("dialog", (dialog) => dialog.accept("/Users/me/Desktop/draft.md"));
  await page.locator("#open-file").click();

  await expect(page.locator("#files-toggle")).toHaveAttribute("aria-expanded", "true");
  const file = page.locator('#file-list button.note[data-path="/Users/me/Desktop/draft.md"]');
  await expect(file).toHaveClass(/active/);
});
