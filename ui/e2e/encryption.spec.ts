import { expect, test, type Page } from "@playwright/test";

import { t } from "./window";

/** The recovery code of the mock storage (`MOCK_CODE` in `src/mock/core.ts`). */
const CODE = "K7QF-2MXA-PL4D-WN6E-ZR3T-YB5H-JC";

/**
 * The vault page with a folder storage. `encrypted` makes the storage one
 * that another device already encrypted: this device has no key yet.
 */
async function openVault(page: Page, encrypted = false): Promise<void> {
  await page.addInitScript((encrypted) => {
    sessionStorage.setItem(
      "kknow:mock:sync",
      JSON.stringify({
        provider: { kind: "folder", path: "/Volumes/Shared/notes" },
        policy: "keepBoth",
        autoSyncMinutes: 0,
      }),
    );
    if (encrypted) sessionStorage.setItem("kknow:mock:encrypted", "1");
  }, encrypted);
  await page.goto("/settings.html#vaults");
}

const encryptionRow = (page: Page) =>
  page.locator(".settings-row", { hasText: t("settings.encryption") });

const field = (page: Page, label: string) =>
  page.locator(".settings-encryption label", { hasText: t(label) }).locator("input");

test("encryption turns on and shows the recovery code one time", async ({ page }) => {
  await openVault(page);
  await expect(encryptionRow(page)).toContainText(t("settings.encryption.off"));
  await encryptionRow(page).getByRole("button", { name: t("settings.encryption.turnOn") }).click();

  const encrypt = page.getByRole("button", { name: t("settings.encryption.encrypt") });
  await field(page, "settings.encryption.passphrase").fill("correct horse");
  await field(page, "settings.encryption.repeat").fill("correct hose");
  await encrypt.click();
  await expect(page.locator(".settings-form-error")).toHaveText(t("settings.encryption.mismatch"));

  // The core checks the length: its error comes in the language of the window.
  await field(page, "settings.encryption.passphrase").fill("short");
  await field(page, "settings.encryption.repeat").fill("short");
  await encrypt.click();
  await expect(page.locator(".settings-form-error")).toHaveText(
    t("error.passphraseTooShort", { value: 8 }),
  );

  await field(page, "settings.encryption.passphrase").fill("correct horse");
  await field(page, "settings.encryption.repeat").fill("correct horse");
  await encrypt.click();

  await expect(page.locator(".settings-recovery-code")).toHaveText(CODE);
  const done = page.getByRole("button", { name: t("settings.encryption.code.done") });
  await expect(done).toBeDisabled();
  await page.getByLabel(t("settings.encryption.code.saved")).check();
  await done.click();

  await expect(page.locator(".settings-recovery-code")).toHaveCount(0);
  await expect(encryptionRow(page)).toContainText(t("settings.encryption.on"));
});

test("a second device unlocks an encrypted storage with the recovery code", async ({ page }) => {
  await openVault(page, true);
  await page.getByRole("button", { name: t("palette.action.sync") }).click();

  // The sync said "locked": the page asks for the passphrase.
  const secret = field(page, "settings.encryption.secret");
  await expect(secret).toBeVisible();
  await secret.fill("wrong horse");
  await page.getByRole("button", { name: t("settings.encryption.unlock") }).click();
  await expect(page.locator(".settings-form-error")).toHaveText(t("error.wrongPassphrase"));

  // The code works in any case and without dashes.
  await secret.fill(CODE.replaceAll("-", "").toLowerCase());
  await page.getByRole("button", { name: t("settings.encryption.unlock") }).click();
  await expect(secret).toHaveCount(0);
  await expect(encryptionRow(page)).toContainText(t("settings.encryption.on"));
});

test("the passphrase changes with the old one", async ({ page }) => {
  await openVault(page, true);
  await page.getByRole("button", { name: t("palette.action.sync") }).click();
  await field(page, "settings.encryption.secret").fill("correct horse");
  await page.getByRole("button", { name: t("settings.encryption.unlock") }).click();

  await encryptionRow(page).getByRole("button", { name: t("settings.encryption.change") }).click();
  await field(page, "settings.encryption.current").fill("wrong horse");
  await field(page, "settings.encryption.new").fill("battery staple");
  await field(page, "settings.encryption.repeat").fill("battery staple");
  await page.locator(".settings-encryption").getByRole("button", { name: t("dialog.sync.save") }).click();
  await expect(page.locator(".settings-form-error")).toHaveText(t("error.wrongPassphrase"));

  await field(page, "settings.encryption.current").fill("correct horse");
  await page.locator(".settings-encryption").getByRole("button", { name: t("dialog.sync.save") }).click();
  await expect(page.locator(".settings-encryption")).toHaveCount(0);
  await expect(encryptionRow(page)).toContainText(t("settings.encryption.on"));
});
