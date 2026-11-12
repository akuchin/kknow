// The lock session of each vault: does it have a password for locked notes,
// and are they open now. The core keeps the key; the window keeps only these
// two flags, to draw the lock icons.
//
// A low zone: the tree reads it to draw the icons, and `lock-view.ts` (the
// panel and the actions) writes it. The module does not touch the DOM.

import { api, type LockStateView } from "../api";

const sessions = new Map<string, LockStateView>();

/** Handlers of a change of any session. */
const handlers = new Set<() => void>();

/** True if the locked notes of the vault open now. */
export function isUnlocked(vaultId: string | undefined): boolean {
  return vaultId !== undefined && sessions.get(vaultId)?.unlocked === true;
}

/** True if the vault has a password for locked notes. */
export function hasPassword(vaultId: string | undefined): boolean {
  return vaultId !== undefined && sessions.get(vaultId)?.password === true;
}

/** Reads the session of the vault from the core and tells the handlers. */
export async function refreshLock(vaultId: string): Promise<LockStateView> {
  const lock = await api.lockState(vaultId);
  sessions.set(vaultId, lock);
  for (const handler of [...handlers]) handler();
  return lock;
}

/** Calls `handler` after each change of a session. */
export function onLockChange(handler: () => void): () => void {
  handlers.add(handler);
  return () => handlers.delete(handler);
}
