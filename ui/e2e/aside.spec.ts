import { expect, test } from "@playwright/test";

import { editorText, noteRow, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

const aside = (page: import("@playwright/test").Page) => page.locator("#note-aside");

test("the note panel is on the right in all three modes", async ({ page }) => {
  const app = page.locator("#app");
  await expect(app).toHaveAttribute("data-aside", "shown");
  for (const mode of ["edit", "split", "read"]) {
    await page.locator(`.mode-switch button[data-mode="${mode}"]`).click();
    await expect(app).toHaveAttribute("data-mode", mode);
    await expect(aside(page)).toBeVisible();
    await expect(page.locator("#aside-outline-list")).toContainText("Contents");
  }
});

test("the key hides the panel, and the window remembers it", async ({ page }) => {
  await page.keyboard.press("ControlOrMeta+Alt+KeyP");
  await expect(page.locator("#app")).toHaveAttribute("data-aside", "hidden");
  await expect(aside(page)).toBeHidden();

  await page.reload();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(aside(page)).toBeHidden();

  await page.locator("#toggle-aside").click();
  await expect(aside(page)).toBeVisible();
});

test("the outline follows edits and marks the section on the screen", async ({ page }) => {
  await noteRow(page, "Reading").click();
  const outline = page.locator("#aside-outline-list");
  await expect(outline.locator("button")).toHaveText(["Reading", "Books", "Articles"]);

  await outline.locator("button", { hasText: "Articles" }).click();
  await expect(outline.locator(".is-current")).toHaveText("Articles");

  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n\n## Later");
  await expect(outline.locator("button")).toHaveText(["Reading", "Books", "Articles", "Later"]);
});

test("backlinks show the line of each link and open the note there", async ({ page }) => {
  await noteRow(page, "Rust").click();
  const backlinks = page.locator("#aside-backlinks-list");
  const line = backlinks.locator(".aside-source", { hasText: "Contents" }).locator(".aside-line");
  await expect(line).toContainText("[[Rust]]");

  await line.click();
  await expect(page.locator("#note-title")).toHaveText("Index");
});

test("a mention without a link becomes a link", async ({ page }) => {
  await noteRow(page, "Rust").click();
  const mentions = page.locator("#aside-mentions");
  await mentions.locator("summary").click();
  const row = mentions.locator(".aside-source", { hasText: "Reading" }).locator(".aside-mention");
  await expect(row).toContainText("The Rust book");

  await row.locator(".aside-link").click();
  await expect(statusMessage(page)).toHaveText(t("status.mentionLinked", { name: "Reading" }));
  await expect(mentions.locator(".aside-source", { hasText: "Reading" })).toHaveCount(0);
  await expect(
    page.locator("#aside-backlinks-list .aside-source", { hasText: "Reading" }),
  ).toContainText("[[Rust]]");

  await noteRow(page, "Reading").click();
  await expect(editorText(page)).toContainText("The [[Rust]] book");
});
