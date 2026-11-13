import { expect, test } from "@playwright/test";

import { editorText, menuItem, noteRow, openRowMenu, openWindow, statusMessage, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
});

test("an edit is saved and stays after the note opens again", async ({ page }) => {
  await noteRow(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
  await expect(editorText(page)).toContainText("Lifetimes");

  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("A new line from the test");
  await expect(statusMessage(page)).toHaveText(t("status.notSaved"));
  await expect(statusMessage(page)).toHaveText(t("status.saved"));

  // Another note and back: the text comes from the mock core again.
  await noteRow(page, "Index").click();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(editorText(page)).not.toContainText("A new line from the test");
  await noteRow(page, "Rust").click();
  await expect(editorText(page)).toContainText("A new line from the test");
});

test("back and forward walk the opened notes", async ({ page }) => {
  const back = page.locator("#nav-back");
  const forward = page.locator("#nav-forward");
  const title = page.locator("#note-title");
  await expect(back).toBeDisabled();
  await expect(forward).toBeDisabled();

  await noteRow(page, "Rust").click();
  await expect(title).toHaveText("Rust");
  await noteRow(page, "Branches").click();
  await expect(title).toHaveText("Branches");

  await back.click();
  await expect(title).toHaveText("Rust");
  await back.click();
  await expect(title).toHaveText("Index");
  await expect(back).toBeDisabled();

  await forward.click();
  await expect(title).toHaveText("Rust");
  // The keys do the same as the buttons.
  await page.keyboard.press("ControlOrMeta+Alt+ArrowRight");
  await expect(title).toHaveText("Branches");
  await expect(forward).toBeDisabled();
  await page.keyboard.press("ControlOrMeta+Alt+ArrowLeft");
  await expect(title).toHaveText("Rust");
});

test("a link in the preview opens its note", async ({ page }) => {
  await page.locator("#preview a", { hasText: "Rust" }).click();
  await expect(page.locator("#note-title")).toHaveText("Rust");
  await expect(page.locator("#aside-backlinks-list")).toContainText("Contents");
});

test("the mode switch shows the editor, both panes or the preview", async ({ page }) => {
  const app = page.locator("#app");
  const pressed = (mode: string) => page.locator(`.mode-switch button[data-mode="${mode}"]`);
  await expect(app).toHaveAttribute("data-mode", "split");
  await expect(page.locator("#editor")).toBeVisible();
  await expect(page.locator("#preview")).toBeVisible();

  await pressed("read").click();
  await expect(app).toHaveAttribute("data-mode", "read");
  await expect(pressed("read")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#editor")).toBeHidden();
  await expect(page.locator("#preview h1")).toHaveText("Contents");

  await pressed("edit").click();
  await expect(app).toHaveAttribute("data-mode", "edit");
  await expect(page.locator("#editor")).toBeVisible();
  await expect(page.locator(".preview-pane")).toBeHidden();

  // The mode is kept for the note: another note gets the default, this one
  // gets its own back.
  await noteRow(page, "Rust").click();
  await expect(app).toHaveAttribute("data-mode", "split");
  await noteRow(page, "Index").click();
  await expect(app).toHaveAttribute("data-mode", "edit");
});

test("in Read mode the blocks show their line numbers while the setting is on", async ({ page }) => {
  // "Meetings" has a heading, a paragraph and a nested list.
  await noteRow(page, "Team/Meetings").click();
  await page.locator('.mode-switch button[data-mode="read"]').click();
  const mark = (line: number) => page.locator(`#preview .line-mark[data-line="${line}"]`);
  const number = (line: number) =>
    mark(line).evaluate((el) => getComputedStyle(el, "::after").content);

  // The mock starts with the line numbers on.
  await expect(page.locator("#app")).toHaveAttribute("data-line-numbers", "true");
  expect(await number(5)).toBe('"5"');
  // A nested list item gets its number too.
  expect(await number(7)).toBe('"7"');

  await page.locator("#status-lines").click();
  await expect(page.locator("#app")).toHaveAttribute("data-line-numbers", "false");
  expect(await number(5)).toBe("none");
});

test("the font size of a note changes from the status bar", async ({ page }) => {
  const value = page.locator("#status-scale-value");
  await expect(value).toHaveText("100%");

  await page.locator("#status-scale-larger").click();
  await expect(value).not.toHaveText("100%");
  const larger = await value.textContent();
  expect(parseInt(larger ?? "", 10)).toBeGreaterThan(100);
  await expect(page.locator("#preview")).toHaveCSS("font-size", /px$/);
  expect(await page.locator("#preview").evaluate((node) => node.style.fontSize)).toBe(larger);

  // The size belongs to the note: another note has the default size.
  await noteRow(page, "Rust").click();
  await expect(value).toHaveText("100%");
  await noteRow(page, "Index").click();
  await expect(value).toHaveText(larger!);

  // A click on the value resets it.
  await value.click();
  await expect(value).toHaveText("100%");
});

test("the palette offers the commands of the open note only", async ({ page }) => {
  const palette = page.locator("#palette");
  const command = palette.locator(".palette-action", { hasText: "Send to Confluence" });
  await page.locator("#open-palette").click();
  await expect(command).toBeVisible();
  await page.keyboard.press("Escape");

  // The open note is deleted: nothing is open, so no note command is left.
  page.once("dialog", (dialog) => void dialog.accept());
  await openRowMenu(page, noteRow(page, "Index"));
  await menuItem(page, t("commands.note.delete")).click();
  await expect(page.locator("#note-title")).toHaveText(t("topbar.note.empty"));

  await page.locator("#open-palette").click();
  await expect(palette).toBeVisible();
  await expect(palette.locator(".palette-action").first()).toBeVisible();
  await expect(command).toHaveCount(0);
});

test("the note toolbar counts bookmarks and repeats the last command", async ({ page }) => {
  const count = page.locator("#bookmarks-count");
  const run = page.locator("#run-last");
  const command = "Send to Confluence";
  await expect(count).toHaveText("");
  await expect(run).toHaveAttribute("title", t("topbar.commands.title"));

  await noteRow(page, "Rust").click();
  await expect(count).toHaveText("2");

  // Nothing to repeat yet: the button opens the menu, and a run makes the
  // command the last one.
  await run.click();
  await menuItem(page, command).click();
  await expect(statusMessage(page)).toContainText(command);
  await expect(run).toHaveAttribute("title", t("topbar.commands.run", { name: command }));

  // Another note with the same command keeps the button.
  await noteRow(page, "Index").click();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(run).toHaveAttribute("title", t("topbar.commands.run", { name: command }));

  // A bookmark from the toolbar menu shows in the count at once.
  await editorText(page).click();
  await page.locator("#note-bookmarks").click();
  await menuItem(page, t("palette.action.bookmark")).click();
  await expect(count).toHaveText("1");
});

test("a long command shows its output as it goes and stops on request", async ({ page }) => {
  const float = page.locator("#run-float");
  const output = page.locator("#run-output");
  const stop = page.locator("#run-stop");
  page.on("dialog", (dialog) => void dialog.accept());

  const folder = page.locator('.folder-row[data-vault="notes"][data-path="Projects"] button.folder');
  await openRowMenu(page, folder);
  await menuItem(page, "Pack an archive").click();

  // The run is long: the float opens by itself, before the end.
  await expect(float).toBeVisible();
  await expect(stop).toBeVisible();
  await expect(output).toContainText("packing the archive…");
  await expect(output).not.toContainText("archive ready");

  await stop.click();
  await expect(page.locator("#run-summary")).toContainText(t("panel.run.stopped", { seconds: "" }).split(" ")[0]);
  await expect(stop).toBeHidden();
  await expect(output).not.toContainText("archive ready");
});

test("the copy button of a code block copies the code, in the preview and the editor", async ({
  page,
}) => {
  // The clipboard of the test browser is not the same in each engine; the
  // stub records what the window asks to copy.
  await page.addInitScript(() => {
    const copied: string[] = [];
    (window as unknown as { copied: string[] }).copied = copied;
    navigator.clipboard.writeText = (text: string) => {
      copied.push(text);
      return Promise.resolve();
    };
  });
  // Again: the stub must be in place before the window code starts.
  await openWindow(page);
  const copied = () => page.evaluate(() => (window as unknown as { copied: string[] }).copied);
  const code = '// entry point\nfn main() {\n    let name = "kknow";\n    println!("{name}");\n}';

  await noteRow(page, "Projects/kknow").click();
  await page.locator('.mode-switch button[data-mode="read"]').click();
  const block = page.locator("#preview pre");
  await block.hover();
  const inPreview = block.locator(".code-copy");
  await inPreview.click();
  await expect(inPreview).toHaveAttribute("data-state", "copied");
  expect(await copied()).toEqual([code]);

  await page.locator('.mode-switch button[data-mode="edit"]').click();
  const widget = page.locator("#editor .cm-code-widget");
  await widget.hover();
  await widget.locator(".code-copy").click();
  expect(await copied()).toEqual([code, code]);
  // The click copies; it does not open the block for editing.
  await expect(widget).toBeVisible();
});
