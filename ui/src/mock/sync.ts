import type {
  EncryptionOn,
  SyncSettings,
  SyncSettingsForm,
  SyncStatus,
} from "../api";
import { MOCK_CODE, coded, gitState, session, vaults } from "./store";
import type { Handlers } from "./handlers";

export const statuses = new Map<string, SyncStatus>([
  [
    "archive-2024",
    {
      vault: "archive-2024",
      state: "conflict",
      done: 0,
      total: 0,
      at: Date.now() - 42 * 60000,
      message: null,
      conflicts: 2,
      location: "/Volumes/Backup/Archive 2024",
      autoSyncMinutes: 15,
    },
  ],
]);

export function configureSync(settings: SyncSettingsForm, secret: boolean): SyncSettings {
  const provider = settings.provider;

  if (provider.kind === "s3" && !provider.bucket.trim()) {
    throw coded("noBucket", "sync settings: no bucket is set");
  }
  if (provider.kind === "folder" && !provider.path.trim()) {
    throw coded("noFolder", "sync settings: no folder is set");
  }

  const location =
    provider.kind === "folder"
      ? provider.path
      : provider.kind === "git"
        ? provider.url
        : `${provider.endpoint ?? "s3:/"}/${provider.bucket}/${provider.prefix ?? ""}`;
  const needsSecret =
    provider.kind === "s3" || (provider.kind === "git" && provider.auth === "token");

  statuses.set(session.activeVault.id, {
    ...statusOf(session.activeVault.id),
    state: "idle",
    location,
    autoSyncMinutes: settings.autoSyncMinutes,
  });

  const same = session.remote?.location === location;
  session.remote = {
    ...settings,
    location,
    needsSecret,
    hasSecret: needsSecret && (secret || (session.remote?.hasSecret ?? false)),
    encrypted: same && session.remote?.encrypted === true,
    hasKey: same && session.remote?.hasKey === true,
  };
  return session.remote;
}


export function opens(secret: string): boolean {
  const plain = (text: string) => text.replace(/[\s-]/g, "").toUpperCase();
  return session.storage !== null && (secret === session.storage.passphrase || plain(secret) === plain(session.storage.code));
}

export function checkPassphrase(passphrase: string): void {
  if ([...passphrase].length < 8) {
    throw coded("passphraseTooShort", "the passphrase must have at least 8 characters", {
      value: "8",
    });
  }
}

export function syncEncrypt(passphrase: string): EncryptionOn {
  if (!session.remote) throw coded("syncNotConfigured", "sync is not set up");
  if (session.storage) throw coded("alreadyEncrypted", "the storage is already encrypted");
  checkPassphrase(passphrase);
  session.storage = { passphrase, code: MOCK_CODE };
  session.remote = { ...session.remote, encrypted: true, hasKey: true };
  return { recoveryCode: MOCK_CODE, settings: session.remote };
}

export function syncUnlock(secret: string): SyncSettings {
  if (!session.remote) throw coded("syncNotConfigured", "sync is not set up");
  if (!session.storage) throw coded("notEncrypted", "the storage is not encrypted");
  if (!opens(secret)) throw coded("wrongPassphrase", "wrong passphrase or recovery code");
  session.remote = { ...session.remote, encrypted: true, hasKey: true };
  return session.remote;
}

export function syncChangePassphrase(current: string, next: string): void {
  if (!session.storage) throw coded("notEncrypted", "the storage is not encrypted");
  if (!opens(current)) throw coded("wrongPassphrase", "wrong passphrase or recovery code");
  checkPassphrase(next);
  session.storage.passphrase = next;
}

export const presetSync = sessionStorage.getItem("kknow:mock:sync");
if (presetSync) configureSync(JSON.parse(presetSync) as SyncSettingsForm, false);

export function statusOf(vault: string): SyncStatus {
  return (
    statuses.get(vault) ?? {
      vault,
      state: "off",
      done: 0,
      total: 0,
      at: null,
      message: null,
      conflicts: 0,
      location: null,
      autoSyncMinutes: 0,
    }
  );
}


export const syncHandlers = {
  sync_status: () => {
    return statusOf(session.activeVault.id);
  },

  sync_statuses: () => {
    return [...vaults.map((vault) => statusOf(vault.id))];
  },

  sync_settings: () => {
    return session.remote;
  },

  save_sync_settings: (args) => {
    return configureSync(args.settings as SyncSettingsForm, Boolean(args.secret));
  },

  sync_encrypt: (args) => {
    return syncEncrypt(String(args.passphrase ?? ""));
  },

  sync_unlock: (args) => {
    return syncUnlock(String(args.secret ?? ""));
  },

  sync_change_passphrase: (args) => {
    syncChangePassphrase(String(args.current ?? ""), String(args.new ?? ""));
    return;
  },

  forget_sync_settings: () => {
    session.remote = null;
    statuses.delete(session.activeVault.id);
    return;
  },

  sync_now: () => {
    if (!session.remote) throw coded("syncNotConfigured", "sync is not set up");
    if (session.storage && !(session.remote.encrypted && session.remote.hasKey)) {
      const error = coded(
        "storageLocked",
        "the storage is encrypted: enter the passphrase or the recovery code",
      );
      statuses.set(session.activeVault.id, { ...statusOf(session.activeVault.id), state: "error", message: error });
      throw error;
    }
    if (session.remote.provider.kind === "git") {
      const pushed = gitState.ahead;
      const downloaded = gitState.behind;
      gitState.ahead = 0;
      gitState.behind = 0;
      statuses.set(session.activeVault.id, {
        ...statusOf(session.activeVault.id),
        state: "idle",
        at: Date.now(),
      });
      return {
        uploaded: 0,
        downloaded,
        removedLocal: 0,
        removedRemote: 0,
        pushed,
        conflicts: [],
      };
    }
    statuses.set(session.activeVault.id, {
      ...statusOf(session.activeVault.id),
      state: "conflict",
      done: 2,
      total: 2,
      at: Date.now(),
      conflicts: 1,
      message: null,
    });
    return {
      uploaded: 1,
      downloaded: 1,
      removedLocal: 0,
      removedRemote: 0,
      pushed: 0,
      conflicts: [{ id: "Index", copy: "Index (conflict 3f9a1c2b)" }],
    };
  },
} satisfies Handlers;
