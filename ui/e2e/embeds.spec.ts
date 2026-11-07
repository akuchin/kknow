import { expect, test } from "@playwright/test";

import { noteRow, openWindow, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
  await noteRow(page, "Embeds").click();
});

test("an embed shows the section in place, read-only", async ({ page }) => {
  const embed = page.locator("#preview .embed:not(.embed-stopped)");
  await expect(embed).toContainText("The Rust book first");
  // A task of a different note: no click can change the open note.
  await expect(embed.locator("input[type=checkbox]")).toBeDisabled();
  // A loop stays a link and says why.
  await expect(page.locator("#preview .embed-stopped")).toHaveAttribute("title", t("embed.loop"));
});

test("a click on an embed opens its note", async ({ page }) => {
  await page.locator("#preview .embed-body p").click();
  await expect(page.locator("#note-title")).toHaveText("Reading");
});
