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

test("the command form adds, edits and removes a command", async ({ page }) => {
  await page.goto("/settings.html#commands");
  const commands = page.locator(".settings-command-list button.settings-command");
  await expect(commands.first()).toBeVisible();
  const before = await commands.count();

  const tools = page.locator(".settings-command-tools");
  await tools.locator("button", { hasText: t("settings.commands.add") }).click();
  await expect(commands).toHaveCount(before + 1);
  const added = commands.last();
  await expect(added).toHaveAttribute("aria-current", "true");
  const name = page.locator("#settings-body input.settings-name");
  await expect(name).toHaveValue(t("settings.commands.new"));

  // Typed text saves on `change` and updates the list on the left.
  await name.fill("Сборка");
  await name.press("Tab");
  await expect(added).toContainText("Сборка");

  // A target chip rebuilds the form: the note variable goes away with it.
  const vars = page.locator("details.settings-vars");
  await vars.locator("summary").click();
  await expect(vars).toContainText("$KKNOW_NOTE");
  await page.locator(".settings-chip", { hasText: t("settings.commands.target.note") }).click();
  await expect(page.locator("details.settings-vars")).not.toContainText("$KKNOW_NOTE");

  page.once("dialog", (dialog) => void dialog.accept());
  await tools.locator("button", { hasText: t("settings.commands.remove") }).click();
  await expect(commands).toHaveCount(before);
  await expect(page.locator(".settings-command-list")).not.toContainText("Сборка");
});

test("the vault page renames, recolors and sets up sync", async ({ page }) => {
  await page.goto("/settings.html#vaults");
  const picked = page.locator('.settings-vault[aria-current="true"]');
  await expect(picked).toHaveCount(1);

  const name = page.locator("#settings-body input.settings-name");
  await name.fill("Мои заметки");
  await name.press("Tab");
  await expect(picked).toContainText("Мои заметки");

  await page.locator('.settings-color[data-color="red"]').click();
  await expect(page.locator('.settings-color[data-color="red"]')).toHaveAttribute("aria-pressed", "true");
  await expect(picked.locator(".vault-label")).toHaveAttribute("data-color", "red");

  // S3 shows its key form; its button saves the fields.
  const provider = page
    .locator(".settings-row", { hasText: t("settings.vaults.provider") })
    .locator("select");
  await provider.selectOption("s3");
  // The bucket is the first field; "Префикс в бакете" also has its name in it.
  const bucket = page.locator(".settings-s3 label").first().locator("input");
  await expect(page.locator(".settings-s3 label").first()).toContainText(t("dialog.sync.bucket"));
  await expect(bucket).toHaveValue("kknow");
  await bucket.fill("notes-bucket");
  await page.locator(".settings-s3 button", { hasText: t("dialog.sync.save") }).click();
  await expect(bucket).toHaveValue("notes-bucket");

  await provider.selectOption("off");
  await expect(page.locator(".settings-s3")).toHaveCount(0);
  await expect(provider).toHaveValue("off");
});

test("git asks for the program first and then connects the vault", async ({ page }) => {
  await page.goto("/settings.html#vaults");
  const provider = page
    .locator(".settings-row", { hasText: t("settings.vaults.provider") })
    .locator("select");
  await provider.selectOption("git");

  const form = page.locator(".settings-git");
  await form.getByPlaceholder(t("dialog.git.url.placeholder")).fill("https://github.com/me/notes.git");
  // Only the field of the picked sign-in shows.
  await expect(form.getByPlaceholder("GITHUB_TOKEN")).toBeHidden();
  const connect = form.getByRole("button", { name: t("dialog.git.connect") });
  await connect.click();
  await expect(form.locator(".settings-form-error")).toHaveText(t("error.gitPathNotSet"));

  // "Find" fills the path to git in the general section.
  await page.goto("/settings.html#general");
  await page.getByRole("button", { name: t("settings.general.git.find") }).click();
  await expect(page.locator(".settings-git-path")).toHaveValue("/opt/homebrew/bin/git");
  await expect(page.getByText("git version 2.47.1")).toBeVisible();

  await page.goto("/settings.html#vaults");
  await provider.selectOption("git");
  await form.getByPlaceholder(t("dialog.git.url.placeholder")).fill("https://github.com/me/notes.git");
  await form.locator("select").selectOption("env");
  await form.getByPlaceholder("GITHUB_TOKEN").fill("GITHUB_TOKEN");
  await connect.click();

  // Connected: the address comes from the repository and does not change.
  await expect(form.getByRole("button", { name: t("dialog.sync.save") })).toBeVisible();
  await expect(form.getByPlaceholder(t("dialog.git.url.placeholder"))).toHaveAttribute("readonly", "");
  await expect(page.locator(".settings-row", { hasText: t("dialog.sync.policy") })).toHaveCount(0);
});
