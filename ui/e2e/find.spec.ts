import { expect, test, type Page } from "@playwright/test";

import { noteRow, openWindow, t } from "./window";

async function openMeetings(page: Page, mode: string): Promise<void> {
  await openWindow(page);
  await noteRow(page, "Team/Meetings").click();
  await expect(page.locator("#note-title")).toHaveText("Meetings");
  await page.locator(`.mode-switch button[data-mode="${mode}"]`).click();
  await expect(page.locator("#app")).toHaveAttribute("data-mode", mode);
  await expect(page.locator("#editor .cm-content")).toContainText("meeting room");
}

const bar = (page: Page) => page.locator(".preview-pane .find-bar");

test("Read mode: the find bar highlights, steps and closes without a trace", async ({ page }) => {
  await openMeetings(page, "read");
  const before = await page.locator("#preview").innerHTML();

  await page.keyboard.press("ControlOrMeta+f");
  await expect(bar(page)).toBeVisible();
  const input = bar(page).locator(".find-input");
  await expect(input).toBeFocused();

  await input.fill("meeting");
  const count = bar(page).locator(".find-count");
  await expect(count).toHaveText(t("find.count", { current: 1, total: 2 }));
  await input.press("Enter");
  await expect(count).toHaveText(t("find.count", { current: 2, total: 2 }));
  await input.press("Enter");
  await expect(count).toHaveText(t("find.count", { current: 1, total: 2 }));
  await input.press("Shift+Enter");
  await expect(count).toHaveText(t("find.count", { current: 2, total: 2 }));

  await input.fill("zzz");
  await expect(count).toHaveText(t("find.none"));
  await bar(page).getByLabel(t("editor.search.regexp")).check();
  await input.fill("(");
  await expect(input).toHaveAttribute("aria-invalid", "true");
  await expect(count).toHaveText(t("find.none"));

  await input.press("Escape");
  await expect(bar(page)).toHaveCount(0);
  expect(await page.locator("#preview").innerHTML()).toBe(before);

  const box = page.locator("#preview li input[type=checkbox]").first();
  await box.click();
  await expect(box).toBeChecked();
});

test("Side by side: the pane that was clicked last gets the search", async ({ page }) => {
  await openMeetings(page, "split");
  await page.locator("#editor .cm-content").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(page.locator(".cm-search")).toBeVisible();
  await page.keyboard.press("Escape");

  await page.locator("#preview h1").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(bar(page)).toBeVisible();
});

test("a search follows the change of the mode", async ({ page }) => {
  await openMeetings(page, "read");
  await page.keyboard.press("ControlOrMeta+f");
  await bar(page).locator(".find-input").fill("meeting");
  await page.locator('.mode-switch button[data-mode="edit"]').click();
  await expect(bar(page)).toHaveCount(0);
  await expect(page.locator(".cm-search")).toBeVisible();
  await expect(page.locator(".cm-search input[name=search]")).toHaveValue("meeting");
});

test("Live preview: the editor panel has the replace field", async ({ page }) => {
  await openMeetings(page, "edit");
  await page.locator("#editor .cm-content").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(page.locator(".cm-search")).toBeVisible();
  await expect(page.locator(".cm-search input[name=replace]")).toBeVisible();
});

test("the trash view has the bar, and Esc closes the bar first", async ({ page }) => {
  await openWindow(page);
  await page.locator('.trash-row[data-vault="notes"] button.trash').click();
  await page.locator("button.trash-entry").first().click();
  await expect(page.locator("#trash-view")).toBeVisible();

  await page.keyboard.press("ControlOrMeta+f");
  const trashBar = page.locator("#trash-view .find-bar");
  await expect(trashBar).toBeVisible();
  await expect(trashBar.locator(".find-input")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(trashBar).toHaveCount(0);
  await expect(page.locator("#trash-view")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#trash-view")).toBeHidden();
});
