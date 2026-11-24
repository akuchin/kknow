import { api, type UpdateStatus } from "../api";
import { describeError, t } from "../i18n";
import { StatusBar, type StatusBarActions } from "../statusbar";
import { el } from "./dom";

export let status: StatusBar;

let flushBeforeUpdate: () => Promise<void> = async () => {};

export function initStatus(
  actions: Omit<StatusBarActions, "update">,
  flush: () => Promise<void>,
): void {
  flushBeforeUpdate = flush;
  status = new StatusBar(el.statusbar, { ...actions, update: () => void restartToUpdate() });
}

export function setStatus(text: string, tone: "idle" | "work" | "error" = "idle"): void {
  status.setMessage(text, tone);
}

export function fail(error: unknown): void {
  setStatus(describeError(error), "error");
}

export function showUpdate(update: UpdateStatus): void {
  status.setUpdate(update.state === "ready" ? update.version : null);
}

async function restartToUpdate(): Promise<void> {
  await flushBeforeUpdate();
  setStatus(t("status.updating"), "work");
  try {
    await api.restartToUpdate();
  } catch (error) {
    fail(error);
  }
}
