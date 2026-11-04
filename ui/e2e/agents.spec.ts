import { expect, test } from "@playwright/test";

import { editorText, noteRow, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
  await noteRow(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
});

test("an accepted edit changes the note and a rejected one does not", async ({ page }) => {
  const toggle = page.locator("#toggle-proposals");
  await expect(toggle).toBeVisible();
  await expect(page.locator("#proposals-count")).toHaveText("1");

  await toggle.click();
  const edits = page.locator("#proposals-list .proposal-edit");
  await expect(edits).toHaveCount(2);
  await expect(edits.first().locator("ins")).toContainText("компилятор ругается");
  // The second edit stays out.
  await edits.nth(1).getByRole("checkbox").uncheck();
  await page.locator(".proposal-feedback").fill("вторую не надо");
  await page.getByRole("button", { name: t("panel.proposals.apply") }).click();

  await expect(statusMessage(page)).toHaveText(t("status.proposalApplied"));
  await expect(editorText(page)).toContainText("компилятор ругается чаще");
  await expect(editorText(page)).not.toContainText("отдельная большая тема");
  await expect(page.locator("#proposals-list")).toContainText(t("panel.proposals.empty"));
});

test("an answer to the question of the agent goes into its thread", async ({ page }) => {
  const toggle = page.locator("#toggle-notes");
  await expect(toggle).toHaveClass(/has-question/);

  await toggle.click();
  const first = page.locator("#notes-list > li").first();
  await expect(first.locator(".thread-status")).toHaveText(t("thread.status.question"));
  await expect(first.locator(".thread-messages")).toContainText("Пример на функции");
  await first.locator("textarea").fill("на функции с вектором");
  await first.getByRole("button", { name: t("panel.notes.send") }).click();

  await expect(statusMessage(page)).toHaveText(t("status.replySent"));
  await expect(first.locator(".thread-status")).toHaveText(t("thread.status.answered"));
  await expect(first.locator(".thread-messages")).toContainText("на функции с вектором");
  await expect(toggle).not.toHaveClass(/has-question/);
});

test("the first answer gives a comment its thread ID", async ({ page }) => {
  await page.locator("#toggle-notes").click();
  const second = page.locator("#notes-list > li").nth(1);
  await second.getByRole("button", { name: t("panel.notes.reply") }).click();
  await second.locator("textarea").fill("можно короче");
  await second.getByRole("button", { name: t("panel.notes.send") }).click();

  await expect(editorText(page)).toContainText("@agent#t2");
  await expect(second.locator(".thread-messages")).toContainText("можно короче");
});
