import { expect, test, type Page } from "@playwright/test";

import { editorText, noteRow, openWindow, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
  await noteRow(page, "Rust").click();
});

const popup = (page: Page) => page.locator(".hover-preview");

/** Puts the pointer on the element and holds ⌘ (Ctrl elsewhere). */
async function hoverWithKey(page: Page, target: import("@playwright/test").Locator) {
  const box = (await target.boundingBox())!;
  await page.mouse.move(box.x + 4, box.y + box.height / 2);
  await page.keyboard.down("ControlOrMeta");
  await page.mouse.move(box.x + 6, box.y + box.height / 2);
}

test("⌘ and hover on a preview link show the note, Esc hides it", async ({ page }) => {
  await hoverWithKey(page, page.locator("#preview a", { hasText: "Projects/kknow" }));
  await expect(popup(page)).toBeVisible();
  await expect(popup(page).locator(".hover-preview-head")).toContainText("kknow");
  await expect(popup(page).locator(".hover-preview-body")).toContainText("Plans");
  await page.keyboard.up("ControlOrMeta");

  await page.keyboard.press("Escape");
  await expect(popup(page)).toBeHidden();
});

test("a hover without the key shows nothing", async ({ page }) => {
  await page.locator("#preview a", { hasText: "Projects/kknow" }).hover();
  await page.waitForTimeout(500);
  await expect(popup(page)).toBeHidden();
});

test("⌘ and hover on a link in the editor show the note too", async ({ page }) => {
  await hoverWithKey(page, editorText(page).getByText("Projects/kknow"));
  await expect(popup(page)).toBeVisible();
  await expect(popup(page).locator(".hover-preview-body")).toContainText("A project in");

  // The title opens the note.
  await popup(page).locator(".hover-preview-head .link").click();
  await page.keyboard.up("ControlOrMeta");
  await expect(page.locator("#note-title")).toHaveText("kknow");
  await expect(popup(page)).toBeHidden();
});

test("a link to a missing note says so", async ({ page }) => {
  await noteRow(page, "Index").click();
  await hoverWithKey(page, page.locator("#preview a", { hasText: "Ghost" }));
  await expect(popup(page)).toContainText(t("hover.missing", { name: "Ghost" }));
  await page.keyboard.up("ControlOrMeta");
});
