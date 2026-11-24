import { expect, test } from "@playwright/test";

import { noteRow, openWindow, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
  await noteRow(page, "Embeds").click();
});

test("an embed shows the section in place, read-only", async ({ page }) => {
  const embed = page.locator("#preview .embed:not(.embed-stopped)");
  await expect(embed).toContainText("The Rust book first");
  await expect(embed.locator("input[type=checkbox]")).toBeDisabled();
  await expect(page.locator("#preview .embed-stopped")).toHaveAttribute("title", t("embed.loop"));
});

test("a click on an embed opens its note", async ({ page }) => {
  await page.locator("#preview .embed-body p").click();
  await expect(page.locator("#note-title")).toHaveText("Reading");
});

test("live preview draws an embed alone on its line", async ({ page }) => {
  await page.locator('.mode-switch button[data-mode="edit"]').click();
  const embed = page.locator("#editor .cm-embed").first();
  await expect(embed).toContainText("The Rust book first");
  await expect(page.locator("#editor .cm-embed").nth(1)).toContainText(t("embed.loop"));

  await embed.hover();
  await embed.locator(".cm-diagram-edit").click();
  await expect(page.locator("#editor .cm-content")).toContainText("![[Reading#Books]]");
});

test("a click on a live embed opens its note", async ({ page }) => {
  await page.locator('.mode-switch button[data-mode="edit"]').click();
  await page.locator("#editor .cm-embed .embed-body").first().click();
  await expect(page.locator("#note-title")).toHaveText("Reading");
});
