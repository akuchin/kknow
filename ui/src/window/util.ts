// Small steps that several zones of the window share.

import { api } from "../api";
import { fail } from "./status";

/** The settings window is separate, so the core opens it, not this page. */
export async function openSettings(section?: string): Promise<void> {
  try {
    await api.openSettings(section);
  } catch (error) {
    fail(error);
  }
}
