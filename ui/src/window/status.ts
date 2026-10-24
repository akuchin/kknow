// The status bar of the main window: messages, errors and the update button.
//
// The segment handlers live in higher layers, so `main.ts` passes them to
// `initStatus`. The module does not touch the DOM on import.

import { api, type UpdateStatus } from "../api";
import { describeError, t } from "../i18n";
import { StatusBar, type StatusBarActions } from "../statusbar";
import { el } from "./dom";

/**
 * Status bar: app state and quick toggles.
 *
 * Handlers get the button because the menu attaches to it, not to the window.
 */
export let status: StatusBar;

/** Writes the open note before a restart into a new version. */
let flushBeforeUpdate: () => Promise<void> = async () => {};

/**
 * Builds the status bar. `flush` writes pending edits: "Restart" calls it
 * before the app goes away.
 */
export function initStatus(
  actions: Omit<StatusBarActions, "update">,
  flush: () => Promise<void>,
): void {
  flushBeforeUpdate = flush;
  status = new StatusBar(el.statusbar, { ...actions, update: () => void restartToUpdate() });
}

/** Window messages go to the status bar through this one entry point. */
export function setStatus(text: string, tone: "idle" | "work" | "error" = "idle"): void {
  status.setMessage(text, tone);
}

export function fail(error: unknown): void {
  setStatus(describeError(error), "error");
}

export function showUpdate(update: UpdateStatus): void {
  status.setUpdate(update.state === "ready" ? update.version : null);
}

/**
 * "Restart" in the status bar. The note goes to disk first: the restart does
 * not wait for the autosave timer. On success the app is gone, so only a
 * failure comes back.
 */
async function restartToUpdate(): Promise<void> {
  await flushBeforeUpdate();
  setStatus(t("status.updating"), "work");
  try {
    await api.restartToUpdate();
  } catch (error) {
    fail(error);
  }
}
