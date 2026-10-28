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

test("the MCP lamp shows the server and its setup for the active vault", async ({ page }) => {
  const lamp = page.locator("#status-mcp");
  // The mock server listens on 7377.
  await expect(lamp).toHaveAttribute("data-tone", "ready");

  await lamp.click();
  const float = page.locator("#mcp-float");
  await expect(float).toBeVisible();
  await expect(float.locator(".mcp-state")).toHaveText("работает: http://127.0.0.1:7377/mcp");
  const token = "0123456789abcdef0123456789abcdef";
  await expect(float.locator("pre").first()).toHaveText(
    `claude mcp add --transport http kknow 'http://127.0.0.1:7377/mcp?vault=notes' --header 'Authorization: Bearer ${token}'`,
  );

  // Labels of the vault take all vaults with them.
  await float.getByRole("checkbox", { name: "#work" }).click();
  await float.getByRole("checkbox", { name: "#ai" }).click();
  await expect(float.locator("pre").first()).toContainText(
    "kknow-work-ai 'http://127.0.0.1:7377/mcp?label=work&label=ai'",
  );
  await expect(float.locator(".mcp-scope p")).toContainText("notes (мок), Work — ASW");

  // A new port goes to the server and into the snippets.
  await float.locator(".mcp-port input").fill("8123");
  await float.locator(".mcp-port input").press("Enter");
  await expect(float.locator(".mcp-state")).toHaveText("работает: http://127.0.0.1:8123/mcp");
  await expect(float.locator("pre").first()).toContainText("127.0.0.1:8123");

  // Off: the lamp goes gray.
  await float.getByRole("switch").click();
  await expect(float.locator(".mcp-state")).toHaveText("выключен");
  await expect(lamp).toHaveAttribute("data-tone", "off");

  await page.keyboard.press("Escape");
  await expect(float).toBeHidden();
  await expect(lamp).toHaveAttribute("data-open", "false");
});

test("the MCP float offers the labels of all vaults, the active ones first", async ({ page }) => {
  await page.locator("#status-vault").click();
  await menuItem(page, "Work — ASW").click();
  await expect(page.locator("#status-vault")).toContainText("Work — ASW");

  await page.locator("#status-mcp").click();
  const float = page.locator("#mcp-float");
  // Work — ASW has only #work; #ai comes from the other vault.
  await expect(float.locator(".mcp-chip")).toHaveText([
    t("panel.mcp.scope.vault"),
    t("panel.mcp.scope.all"),
    "#work",
    "#ai",
  ]);
  await float.getByRole("checkbox", { name: "#ai" }).click();
  await expect(float.locator(".mcp-scope p")).toContainText("notes (мок)");
});

test("the labels of a vault are chips, and labels of other vaults are offered", async ({ context }) => {
  const settings = await context.newPage();
  await settings.goto("/settings.html#vaults");
  await settings.locator(".settings-vault", { hasText: "Work — ASW" }).click();
  const labels = settings.locator(".settings-labels");
  await expect(labels.locator(".settings-label:not(.settings-label-offer)")).toHaveText(["#work"]);
  await expect(labels.locator(".settings-label-offer")).toHaveText(["+#ai"]);

  await labels.locator(".settings-label-offer").click();
  await labels.locator("input").fill("личное");
  await labels.locator("input").press("Enter");
  await expect(labels.locator(".settings-label:not(.settings-label-offer)")).toHaveText([
    "#work",
    "#ai",
    "#личное",
  ]);
  await expect(labels.locator("input")).toBeFocused();

  await labels.getByRole("button", { name: t("settings.vaults.labels.remove", { label: "work" }) }).click();
  await expect(labels.locator(".settings-label:not(.settings-label-offer)")).toHaveText(["#ai", "#личное"]);
  // #work stays on the other vault, so it is offered back.
  await expect(labels.locator(".settings-label-offer")).toHaveText(["+#work"]);
  await settings.close();
});

test("the label filter narrows the tree and the search, and survives a reload", async ({ page }) => {
  const filter = page.locator("#label-filter");
  await expect(filter.getByRole("checkbox")).toHaveText(["#work", "#ai"]);
  const vaultRows = page.locator(".vault-row");
  await expect(vaultRows).toHaveCount(3);

  // Only notes (мок) has #ai; Work — ASW goes away from the tree.
  await filter.getByRole("checkbox", { name: "#ai" }).click();
  await expect(vaultRows).toHaveCount(1);
  await expect(page.locator('.vault-row[data-vault="work-asw"]')).toHaveCount(0);

  // The palette searches the same vaults and says so.
  await page.locator("#open-palette").click();
  await page.locator("#palette-input").fill("Ретро");
  await expect(page.locator("#palette")).toContainText(t("palette.results.labels", { query: "Ретро", labels: "#ai" }));
  await expect(page.locator("#palette-results .palette-row")).toHaveCount(0);
  await page.keyboard.press("Escape");

  // The pick is remembered.
  await page.reload();
  await expect(page.locator("#label-filter").getByRole("checkbox", { name: "#ai" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.locator(".vault-row")).toHaveCount(1);

  // A second click drops the filter.
  await page.locator("#label-filter").getByRole("checkbox", { name: "#ai" }).click();
  await expect(page.locator(".vault-row")).toHaveCount(3);
});
