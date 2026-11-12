import { expect, test, type Page } from "@playwright/test";

import { editorText, menuItem, noteRow, openRowMenu, openWindow, statusMessage, t } from "./window";

const PASSWORD = "correct horse";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

/** The lock button of a note row in the tree. */
function rowLock(page: Page, id: string) {
  return page.locator(`.note-row[data-vault="notes"][data-id="${id}"] .note-lock`);
}

/** Locks Rust: the first password, the recovery code, then the open note. */
async function lockRust(page: Page): Promise<void> {
  await noteRow(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
  await openRowMenu(page, noteRow(page, "Rust"));
  await menuItem(page, t("commands.note.lock")).click();

  const panel = page.locator("#lock-view");
  await expect(panel).toBeVisible();
  await expect(page.locator("#lock-view-title")).toHaveText(t("lock.setup.title"));
  // A short password and two different ones are refused in the window.
  await page.locator("#lock-secret").fill("short");
  await page.locator("#lock-repeat").fill("short");
  await page.locator("#lock-submit").click();
  await expect(page.locator("#lock-error")).toHaveText(t("lock.error.short", { count: 8 }));
  await page.locator("#lock-secret").fill(PASSWORD);
  await page.locator("#lock-repeat").fill("another horse");
  await page.locator("#lock-submit").click();
  await expect(page.locator("#lock-error")).toHaveText(t("lock.error.mismatch"));
  await page.locator("#lock-repeat").fill(PASSWORD);
  await page.locator("#lock-submit").click();

  // The recovery code shows once; the note is locked behind it.
  await expect(page.locator("#lock-view-title")).toHaveText(t("lock.code.title"));
  await expect(page.locator("#lock-code")).not.toBeEmpty();
  await page.locator("#lock-submit").click();
  await expect(panel).toBeHidden();
  await expect(statusMessage(page)).toHaveText(t("status.noteLocked", { name: "Rust" }));
}

test("a note is locked, closed with Lock all and opened with the password", async ({ page }) => {
  await lockRust(page);

  // The note stays open, with an open lock in the tree and in the header.
  await expect(editorText(page)).toContainText("Rust");
  await expect(rowLock(page, "Rust")).toHaveAttribute("data-open", "true");
  await expect(page.locator("#note-lock")).toHaveAttribute("data-open", "true");
  await expect(page.locator("#note-bookmarks")).toBeHidden();

  // A click on the open lock closes all locked notes.
  await page.locator("#note-lock").click();
  await expect(statusMessage(page)).toHaveText(t("status.notesLocked"));
  await expect(rowLock(page, "Rust")).toHaveAttribute("data-open", "false");
  await expect(page.locator("#note-lock")).toHaveAttribute("data-open", "false");
  await expect(page.locator("#lock-view")).toBeVisible();
  await expect(page.locator("#lock-view-title")).toHaveText(t("lock.unlock.title"));

  // A wrong password shows an error and lets the person try again.
  await page.locator("#lock-secret").fill("wrong horse");
  await page.locator("#lock-submit").click();
  await expect(page.locator("#lock-error")).toHaveText(t("lock.error.wrong"));
  await page.locator("#lock-secret").fill(PASSWORD);
  await page.locator("#lock-submit").click();
  await expect(page.locator("#lock-view")).toBeHidden();
  await expect(editorText(page)).toContainText("Rust");
  await expect(rowLock(page, "Rust")).toHaveAttribute("data-open", "true");
});

test("a locked note opens closed from the tree, and Remove lock makes it plain", async ({ page }) => {
  await lockRust(page);
  await page.locator("#note-lock").click();
  await noteRow(page, "Index").click();
  await expect(page.locator("#lock-view")).toBeHidden();

  // A closed note shows the password field, not its text.
  await noteRow(page, "Rust").click();
  await expect(page.locator("#lock-view")).toBeVisible();
  await expect(editorText(page)).not.toContainText("заимствование");

  // The recovery code opens the notes and asks for a new password.
  const code = "K7QF-2M9X-ABCD-EFGH-JKLM-NPQR-ST";
  await page.locator("#lock-secret").fill(code.toLowerCase());
  await page.locator("#lock-submit").click();
  await expect(page.locator("#lock-view-title")).toHaveText(t("lock.reset.title"));
  await page.locator("#lock-secret").fill("battery staple");
  await page.locator("#lock-repeat").fill("battery staple");
  await page.locator("#lock-submit").click();
  await expect(statusMessage(page)).toHaveText(t("status.lockPasswordChanged"));
  await expect(page.locator("#lock-view")).toBeHidden();

  await openRowMenu(page, noteRow(page, "Rust"));
  await menuItem(page, t("commands.note.removeLock")).click();
  await expect(statusMessage(page)).toHaveText(t("status.lockRemoved", { name: "Rust" }));
  await expect(rowLock(page, "Rust")).toHaveCount(0);
  await expect(page.locator("#note-lock")).toBeHidden();
});
