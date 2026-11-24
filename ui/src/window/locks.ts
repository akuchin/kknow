import { api, type LockStateView } from "../api";

const sessions = new Map<string, LockStateView>();

const handlers = new Set<() => void>();

export function isUnlocked(vaultId: string | undefined): boolean {
  return vaultId !== undefined && sessions.get(vaultId)?.unlocked === true;
}

export function hasPassword(vaultId: string | undefined): boolean {
  return vaultId !== undefined && sessions.get(vaultId)?.password === true;
}

export async function refreshLock(vaultId: string): Promise<LockStateView> {
  const lock = await api.lockState(vaultId);
  sessions.set(vaultId, lock);
  for (const handler of [...handlers]) handler();
  return lock;
}

export function onLockChange(handler: () => void): () => void {
  handlers.add(handler);
  return () => handlers.delete(handler);
}
