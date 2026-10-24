import { expect, test } from "@playwright/test";

import { menuItem, noteRow, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("the vault switches from the status bar menu", async ({ page }) => {
  const vault = page.locator("#status-vault");
  await expect(vault).toContainText("notes (мок)");

  await vault.click();
  await expect(page.locator(".menu .menu-title").first()).toHaveText(t("menu.vaults"));
  // An unavailable vault is in the list but cannot be picked.
  await expect(menuItem(page, "Архив 2024")).toBeDisabled();
  await menuItem(page, "Work — ASW").click();

  await expect(vault).toContainText("Work — ASW");
  await expect(page.locator("#note-title")).toHaveText("Смета");
  await expect(page.locator('.vault-row[data-vault="work-asw"]')).toHaveAttribute("data-active", "true");
  await expect(noteRow(page, "Смета", "work-asw")).toHaveClass(/active/);
});

test("⌘1…⌘9 switch the vault and return to the note left there", async ({ page }) => {
  await noteRow(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Rust");

  await page.keyboard.press("ControlOrMeta+2");
  await expect(page.locator("#status-vault")).toContainText("Work — ASW");
  await expect(page.locator("#note-title")).toHaveText("Смета");

  await page.keyboard.press("ControlOrMeta+1");
  await expect(page.locator("#status-vault")).toContainText("notes (мок)");
  await expect(page.locator("#note-title")).toHaveText("Rust");
});

test("a note of another vault opens from the tree and switches the vault", async ({ page }) => {
  // Other vaults start collapsed; the first expand loads their notes.
  const work = page.locator('.vault-row[data-vault="work-asw"] button.vault');
  await expect(work).toHaveAttribute("aria-expanded", "false");
  await expect(noteRow(page, "Смета", "work-asw")).toHaveCount(0);
  await work.click();
  await expect(work).toHaveAttribute("aria-expanded", "true");
  await noteRow(page, "Встречи/Ретро", "work-asw").click();
  await expect(page.locator("#status-vault")).toContainText("Work — ASW");
  await expect(page.locator("#note-title")).toHaveText("Ретро");
});

test("the palette finds notes in all vaults and opens one", async ({ page }) => {
  await page.locator("#open-palette").click();
  const palette = page.locator("#palette");
  await expect(palette).toBeVisible();

  await page.locator("#palette-input").fill("Rust");
  const rows = palette.locator("#palette-results .palette-row");
  // The active vault comes first; a hit in another vault carries its label.
  await expect(rows.first()).toContainText("Rust");
  await expect(palette.locator(".palette-row", { hasText: "Ретро" })).toContainText("Work — ASW");

  await page.locator("#palette-input").fill("Ретро");
  await expect(rows).toHaveCount(1);
  await page.keyboard.press("Enter");
  await expect(palette).toBeHidden();
  await expect(page.locator("#status-vault")).toContainText("Work — ASW");
  await expect(page.locator("#note-title")).toHaveText("Ретро");

  // The keys open it too, and Escape closes it.
  await page.keyboard.press("ControlOrMeta+k");
  await expect(palette).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(palette).toBeHidden();
});

test("sync from the status bar shows the report and the conflict", async ({ page }) => {
  // Sync is set up before the page loads: in the mock the settings window has
  // its own copy of the core and cannot do it for this tab.
  await page.addInitScript(() =>
    sessionStorage.setItem(
      "kknow:mock:sync",
      JSON.stringify({
        provider: { kind: "folder", path: "/Volumes/Shared/notes" },
        policy: "keepBoth",
        autoSyncMinutes: 0,
      }),
    ),
  );
  await openWindow(page);

  const sync = page.locator("#status-sync");
  await expect(sync).toHaveAttribute("data-tone", "ready");
  await expect(sync).toContainText("notes");

  await sync.click();
  await expect(statusMessage(page)).toHaveText(t("status.conflicts", { count: 1 }));
  const panel = page.locator("#sync-panel");
  await expect(panel).toBeVisible();
  await expect(page.locator("#sync-summary")).toContainText(t("sync.report.uploaded"));
  await expect(page.locator("#sync-list")).toContainText("Index (конфликт 3f9a1c2b)");
  await expect(sync).toHaveAttribute("data-tone", "conflict");

  await page.locator("#sync-hide").click();
  await expect(panel).toBeHidden();
});

test("sync without settings opens the vault settings", async ({ page, context }) => {
  await expect(page.locator("#status-sync")).toHaveAttribute("data-tone", "off");
  const [settings] = await Promise.all([
    context.waitForEvent("page"),
    page.locator("#status-sync").click(),
  ]);
  await expect(settings).toHaveURL(/\/settings\.html#vaults$/);
  await expect(
    settings.locator("#settings-sections button[aria-current=true]"),
  ).toHaveText(t("settings.section.vaults"));
});

test("the chip reports a conflict in another vault and leads to it", async ({ page }) => {
  const chip = page.locator("#status-chip");
  await expect(chip).toHaveText(t("statusbar.chip.conflicts", { name: "Архив 2024", count: 2 }));
  await expect(chip).toHaveAttribute("data-tone", "warning");

  await chip.click();
  await expect(menuItem(page, t("palette.action.sync"))).toBeVisible();
  await menuItem(page, t("statusbar.chip.resolve")).click();

  // "Resolve" switches to that vault and searches for the conflict copies.
  await expect(page.locator("#status-vault")).toContainText("Архив 2024");
  await expect(page.locator("#palette")).toBeVisible();
  await expect(page.locator("#palette-input")).toHaveValue(t("statusbar.chip.query"));
});
