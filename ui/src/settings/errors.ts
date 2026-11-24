import { describeError, t } from "../i18n";

interface ErrorLine {
  root: HTMLElement;
  text: HTMLElement;
}

let line: ErrorLine | null = null;

function createLine(): ErrorLine {
  const root = document.createElement("div");
  root.className = "settings-error";
  root.setAttribute("role", "alert");
  root.hidden = true;

  const text = document.createElement("span");
  text.className = "settings-error-text";

  const close = document.createElement("button");
  close.className = "settings-error-close";
  close.textContent = "×";
  close.title = t("settings.error.close");
  close.setAttribute("aria-label", t("settings.error.close"));
  close.onclick = () => {
    root.hidden = true;
  };

  root.append(text, close);
  document.querySelector("#settings")?.append(root);
  return { root, text };
}

export function showError(error: unknown): void {
  line ??= createLine();
  line.text.textContent = describeError(error);
  line.root.hidden = false;
}
