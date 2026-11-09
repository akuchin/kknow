// Small steps that several zones of the window share.

import { api } from "../api";
import { writeClipboard } from "../clipboard";
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

/** Puts text on the clipboard and says so in the status bar. */
export async function copyText(text: string | null): Promise<void> {
  if (text === null) return;
  const copied = await writeClipboard(text);
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
