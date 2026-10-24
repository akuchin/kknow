import { expect, test } from "@playwright/test";

import { menuItem, openWindow, t } from "./window";

/**
 * Sections in menu order, with the first heading of each. The vaults section
 * and the commands section have a list on the left; their first heading
 * belongs to the picked entry.
 */
const SECTIONS: [string, string][] = [
  ["general", "settings.section.general"],
  ["appearance", "settings.section.appearance"],
  ["vaults", "settings.vaults.sync"],
  ["commands", "settings.commands.where"],
  ["keys", "settings.section.keys"],
  ["editor", "settings.section.editor"],
];

test("each settings section opens", async ({ page }) => {
  await page.goto("/settings.html");
  const buttons = page.locator("#settings-sections button.settings-section");
  await expect(buttons).toHaveCount(SECTIONS.length);

  for (const [index, [id, heading]] of SECTIONS.entries()) {
    await buttons.nth(index).click();
    await expect(page).toHaveURL(new RegExp(`#${id}$`));
    await expect(buttons.nth(index)).toHaveText(t(`settings.section.${id}`));
    await expect(buttons.nth(index)).toHaveAttribute("aria-current", "true");
    await expect(page.locator("#settings-body .settings-heading").first()).toHaveText(t(heading));
    // A section has controls, not only its heading.
    await expect(page.locator("#settings-body .settings-row").first()).toBeVisible();
  }
});

test("a theme picked in the settings reaches the main window", async ({ page, context }) => {
  await openWindow(page);
  const root = page.locator("html");
  await expect(root).toHaveAttribute("data-theme", "light");

  const settings = await context.newPage();
  await settings.goto("/settings.html#appearance");
  const theme = settings
    .locator(".settings-row", { hasText: t("settings.appearance.theme") })
    .locator("select");
  await theme.selectOption("dark");
  await expect(root).toHaveAttribute("data-theme", "dark");

  // A vault theme also paints its variables.
  await theme.selectOption("sepia");
  await expect(root).toHaveAttribute("data-theme", "light");
  await expect(root).toHaveAttribute("style", /--bg:\s*#f6efe0/);
});

test("the gear menu changes the theme of the main window", async ({ page }) => {
  await openWindow(page);
  await page.locator("#status-more").click();
  await menuItem(page, t("theme.dark")).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  // The choice is a setting: a reload keeps it.
  await page.reload();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("a key recorded in the settings replaces the default and resets", async ({ page }) => {
  await page.goto("/settings.html#keys");
  const line = page.locator(".settings-row", { hasText: t("settings.keys.action.palette.open") });
  const reset = line.getByRole("button", { name: t("settings.keys.reset") });
  await expect(reset).toHaveCount(0);

  // Esc while recording cancels the recording, not the window.
  await line.getByRole("button", { name: t("settings.keys.change") }).click();
  await expect(line).toContainText(t("settings.keys.record"));
  await page.keyboard.press("Escape");
  await expect(line.getByRole("button", { name: t("settings.keys.change") })).toBeVisible();

  await line.getByRole("button", { name: t("settings.keys.change") }).click();
  await page.keyboard.press("ControlOrMeta+Shift+KeyY");
  await expect(line).toHaveClass(/custom/);
  await expect(line.locator("kbd")).toHaveCount(1);

  // The key is in the keys file: a reload keeps it.
  await page.reload();
  await expect(line).toHaveClass(/custom/);

  await reset.click();
  await expect(line).not.toHaveClass(/custom/);
  await expect(reset).toHaveCount(0);
});
