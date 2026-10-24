// Small steps that several zones of the window share.

import { api } from "../api";
import { t } from "../i18n";
import { fail, setStatus } from "./status";

/** The settings window is separate, so the core opens it, not this page. */
export async function openSettings(section?: string): Promise<void> {
  try {
    await api.openSettings(section);
  } catch (error) {
    fail(error);
  }
}

/**
 * Puts text on the clipboard.
 *
 * First try `navigator.clipboard`. If the webview blocks it (no permission or
 * an insecure context), fall back to the old `execCommand("copy")` with a
 * hidden field: it works with any user gesture, and a menu click is one.
 */
export async function copyText(text: string | null): Promise<void> {
  if (text === null) return;
  try {
    await navigator.clipboard.writeText(text);
    setStatus(t("status.copied"));
    return;
  } catch {
    // Fall back below.
  }

  const field = document.createElement("textarea");
  field.value = text;
  field.setAttribute("readonly", "");
  field.style.position = "fixed";
  field.style.opacity = "0";
  document.body.append(field);
  field.select();
  const copied = document.execCommand("copy");
  field.remove();
  setStatus(t(copied ? "status.copied" : "status.copyFailed"), copied ? "idle" : "error");
}

/** A list of strings from browser storage; a corrupt entry reads as empty. */
export function readList(key: string): string[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(saved) ? saved.filter((item) => typeof item === "string") : [];
  } catch {
    // Silently start over on a corrupt entry: it must not break the tree.
    return [];
  }
}
