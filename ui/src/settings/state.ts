/**
 * What the sections of the settings window share.
 *
 * Three values, so no store: a plain object is enough. The sections read and
 * write it directly. They do not import `main.ts`, which imports them; the
 * window parts they need come through `view`, which `main.ts` fills before
 * the first render.
 */

import { api, type AppSettings, type SyncStatus, type VaultView } from "../api";

export const state = {
  settings: null as AppSettings | null,
  vaults: [] as VaultView[],
  statuses: new Map<string, SyncStatus>(),
};

export const view = {
  /** The right part of the window, which a section fills. */
  body: null as unknown as HTMLElement,
  /** The open section. */
  section: (): string => "general",
  /** Draws the whole window again. */
  render: (): void => {},
};

/** Writes settings to the core; both windows learn about them from an event. */
export async function patch(change: Partial<AppSettings>): Promise<void> {
  if (!state.settings) return;
  const next = { ...state.settings, ...change };
  try {
    state.settings = await api.setSettings(next);
  } catch (error) {
    console.error(error);
  }
  view.render();
}
