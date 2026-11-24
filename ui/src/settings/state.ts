import { api, type AppSettings, type SyncStatus, type VaultView } from "../api";
import { showError } from "./errors";

export const state = {
  settings: null as AppSettings | null,
  vaults: [] as VaultView[],
  statuses: new Map<string, SyncStatus>(),
};

export const view = {
  body: null as unknown as HTMLElement,
  section: (): string => "general",
  render: (): void => {},
};

export async function patch(change: Partial<AppSettings>): Promise<void> {
  if (!state.settings) return;
  const next = { ...state.settings, ...change };
  try {
    state.settings = await api.setSettings(next);
  } catch (error) {
    showError(error);
  }
  view.render();
}
