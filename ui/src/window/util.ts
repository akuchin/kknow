import { api } from "../api";
import { writeClipboard } from "../clipboard";
import { t } from "../i18n";
import { fail, setStatus } from "./status";

export async function openSettings(section?: string): Promise<void> {
  try {
    await api.openSettings(section);
  } catch (error) {
    fail(error);
  }
}

export async function copyText(text: string | null): Promise<void> {
  if (text === null) return;
  const copied = await writeClipboard(text);
  setStatus(t(copied ? "status.copied" : "status.copyFailed"), copied ? "idle" : "error");
}

export function readList(key: string): string[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(saved) ? saved.filter((item) => typeof item === "string") : [];
  } catch {
    return [];
  }
}
