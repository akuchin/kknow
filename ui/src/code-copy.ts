import { writeClipboard } from "./clipboard";
import { t } from "./i18n";
import { icon } from "./icons";

const DONE_MS = 1500;

export function copyButton(source: () => string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "code-copy";
  showReady(button);

  let timer: number | undefined;
  button.onmousedown = (event) => event.preventDefault();
  button.onclick = async (event) => {
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

export function addCopyButtons(root: HTMLElement): void {
  for (const code of root.querySelectorAll<HTMLElement>("pre:not(.mermaid) > code")) {
    const pre = code.parentElement!;
    if (pre.querySelector(":scope > .code-copy")) continue;
    pre.classList.add("has-copy");
    pre.append(copyButton(() => code.textContent ?? ""));
  }
}
