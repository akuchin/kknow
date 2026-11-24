import { expect, test, type Page } from "@playwright/test";

import { editorText, openWindow, t } from "./window";

test.beforeEach(async ({ page }) => {
  await openWindow(page);
  await page.locator('.mode-switch button[data-mode="edit"]').click();
  await expect(page.locator("#app")).toHaveAttribute("data-mode", "edit");
});

async function typeAtEnd(page: Page, text: string): Promise<void> {
  await editorText(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.press("Enter");
  await page.keyboard.type(text);
}

const completion = (page: Page) => page.locator(".cm-tooltip-autocomplete");

const PICK_DELAY = 150;

async function pick(page: Page, text: string): Promise<void> {
  await expect(completion(page)).toContainText(text);
  await page.waitForTimeout(PICK_DELAY);
  await page.keyboard.press("Enter");
  await expect(completion(page)).toBeHidden();
}

const liveLink = (page: Page, text: string) =>
  page.locator("#editor .cm-live-link", { hasText: text }).last();

async function lines(page: Page): Promise<string[]> {
  return page.locator("#editor .cm-line").allTextContents();
}

test("[[ offers notes, and the pick closes the link", async ({ page }) => {
  await typeAtEnd(page, "See [[kkn");
  await pick(page, "kknow");
  await page.keyboard.type(" done");

  await expect(liveLink(page, "kknow")).toBeVisible();
  await expect(editorText(page)).toContainText("See kknow done");
});

test("[[note# offers the headings of that note", async ({ page }) => {
  await typeAtEnd(page, "[[kknow#Pl");
  await pick(page, "Plans");
  await expect(editorText(page)).toContainText("kknow#Plans");
  await page.keyboard.type(" next");
  await expect(liveLink(page, "kknow#Plans")).toBeVisible();
});

test("#ta offers the tags of the vault", async ({ page }) => {
  await typeAtEnd(page, "text #ru");
  await expect(completion(page)).toContainText("rust");
});

test("⌘-click on a link opens its note, a plain click edits it", async ({ page }) => {
  await liveLink(page, "Rust").click();
  await expect(page.locator("#note-title")).toHaveText("Index");
  await expect(editorText(page)).toContainText("[[Rust]]");

  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Alt+Enter");
  await expect(page.locator("#note-title")).toHaveText("Rust");

  await page.keyboard.press("ControlOrMeta+Alt+ArrowLeft");
  await expect(page.locator("#note-title")).toHaveText("Index");
  await liveLink(page, "Rust").click({ modifiers: ["ControlOrMeta"] });
  await expect(page.locator("#note-title")).toHaveText("Rust");
});

test("⌘-click on a link to a missing note creates the note", async ({ page }) => {
  await liveLink(page, "Ghost").click({ modifiers: ["ControlOrMeta"] });
  await expect(page.locator("#note-title")).toHaveText("Ghost");
  await expect(page.locator("#status-message")).toHaveText(t("status.created"));
});

test("Tab nests a list item and ⇧Tab takes it back", async ({ page }) => {
  await typeAtEnd(page, "- first");
  await page.keyboard.press("Enter");
  await page.keyboard.type("second");
  await expect(page.locator("#editor .cm-list-bullet", { hasText: "◦" })).toHaveCount(0);

  await page.keyboard.press("Tab");
  await expect(page.locator("#editor .cm-list-bullet", { hasText: "◦" })).toHaveCount(1);
  await page.keyboard.type("!");
  await expect(editorText(page)).toContainText("second!");

  await page.keyboard.press("Shift+Tab");
  await expect(page.locator("#editor .cm-list-bullet", { hasText: "◦" })).toHaveCount(0);
});

test("⌥↑ moves a list item with its children", async ({ page }) => {
  await typeAtEnd(page, "- alpha");
  await page.keyboard.press("Enter");
  await page.keyboard.type("beta");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Tab");
  await page.keyboard.type("child");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Alt+ArrowUp");

  const text = (await lines(page)).join("\n");
  expect(text.indexOf("beta")).toBeLessThan(text.indexOf("alpha"));
  expect(text.indexOf("child")).toBeLessThan(text.indexOf("alpha"));
});

test("a task shows a box, and a click on it marks the task", async ({ page }) => {
  await typeAtEnd(page, "- [ ] buy milk");
  await page.keyboard.press("Enter");
  await page.keyboard.type("next");

  const box = page.locator("#editor .cm-task-box").first();
  await expect(box).not.toBeChecked();
  await box.click();
  await expect(page.locator("#editor .cm-task-box").first()).toBeChecked();
});

test("a link with an alias shows the alias alone", async ({ page }) => {
  await typeAtEnd(page, "Read [[Rust|the language]] today");
  await expect(liveLink(page, "the language")).toBeVisible();
  await expect(editorText(page)).toContainText("Read the language today");
  await expect(editorText(page)).not.toContainText("Rust|");
});

test("a wide code block scrolls on its own, not the whole editor", async ({ page }) => {
  await typeAtEnd(page, "```\n" + "x".repeat(300) + "\n```\n\nafter");
  await page.keyboard.press("ControlOrMeta+Home");
  await expect(page.locator("#editor .cm-code-block")).toBeVisible();

  const overflow = () =>
    page.evaluate(() => {
      const scroller = document.querySelector("#editor .cm-scroller") as HTMLElement;
      return scroller.scrollWidth - scroller.clientWidth;
    });
  expect(await overflow()).toBe(0);
  await page.locator("#status-lines").click();
  expect(await overflow()).toBe(0);
});
