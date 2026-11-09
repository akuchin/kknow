/**
 * The copy button of a code block: in the preview and on the editor widget.
 *
 * It copies the code only, without the fences and the language name, and
 * without the last line break: a pasted snippet should not bring an empty
 * line with it. For a moment the icon turns into a check mark, so the user
 * sees that the click worked without a look at the status bar.
 */

import { writeClipboard } from "./clipboard";
import { t } from "./i18n";
import { icon } from "./icons";

/** How long the check mark stays after a copy. */
const DONE_MS = 1500;

/** A button that copies what `source` returns at the moment of the click. */
export function copyButton(source: () => string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "code-copy";
  showReady(button);

  let timer: number | undefined;
  // The press must not take the focus or move the selection: in the editor
  // the cursor stays where it was, in the preview the selected text stays.
  button.onmousedown = (event) => event.preventDefault();
  button.onclick = async (event) => {
    // The block itself opens for editing on a click (`live-preview.ts`), and
    // the preview handles clicks on its content.
    event.preventDefault();
    event.stopPropagation();
    const copied = await writeClipboard(source().replace(/\n$/, ""));
    button.dataset.state = copied ? "copied" : "failed";
    const label = t(copied ? "editor.code.copied" : "editor.code.copyFailed");
    button.title = label;
    button.setAttribute("aria-label", label);
    button.replaceChildren(icon(copied ? "check" : "close"));
    window.clearTimeout(timer);
    timer = window.setTimeout(() => showReady(button), DONE_MS);
  };
  return button;
}

function showReady(button: HTMLButtonElement): void {
  delete button.dataset.state;
  const label = t("editor.code.copy");
  button.title = label;
  button.setAttribute("aria-label", label);
  button.replaceChildren(icon("copy"));
}

/**
 * Adds a copy button to each code block of rendered HTML. A mermaid block is
 * a picture, so it gets none.
 *
 * The button goes into the `<pre>`, after the `<code>`: then the block stays
 * one element, and the line marks before it still point at the `<pre>`
 * (`preview-lines.ts`). The `<code>` scrolls, not the `<pre>`, so the button
 * stays in the corner when a long line scrolls.
 */
export function addCopyButtons(root: HTMLElement): void {
  for (const code of root.querySelectorAll<HTMLElement>("pre:not(.mermaid) > code")) {
    const pre = code.parentElement!;
    if (pre.querySelector(":scope > .code-copy")) continue;
    pre.classList.add("has-copy");
    pre.append(copyButton(() => code.textContent ?? ""));
  }
}
