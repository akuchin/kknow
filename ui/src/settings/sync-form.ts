/** The sync form of a vault: the provider, the S3 keys and the folder. */

import { open } from "@tauri-apps/plugin-dialog";

import { api, type ConflictPolicy, type Provider, type SyncSettings, type VaultView } from "../api";
import { t, type Key } from "../i18n";

/**
 * Draws the vault page again after a change. `vaults.ts` owns the page and
 * imports this module, so the call comes in through `initSyncForm`, not
 * through an import back.
 */
let redraw: () => Promise<void> = async () => {};

export function initSyncForm(onChange: () => Promise<void>): void {
  redraw = onChange;
}

/**
 * Changes the provider from the list: "do not sync" turns sync off, folder asks
 * for a folder, S3 switches the provider and shows the key form below.
 */
export async function switchProvider(
  vault: VaultView,
  remote: SyncSettings | null,
  kind: string,
): Promise<void> {
  if (kind === "off") {
    try {
      await api.forgetSyncSettings(vault.id);
    } catch (error) {
      console.error(error);
    }
    await redraw();
    return;
  }

  if (kind === "folder") {
    await pickFolder(vault, remote);
    return;
  }

  // The bucket and keys go into the form below; it appears once the provider
  // is S3. The core does not save an empty bucket, so we use a placeholder
  // name that the user edits right away.
  await saveSync(vault, remote, {
    provider: {
      kind: "s3",
      bucket: remote?.provider.kind === "s3" ? remote.provider.bucket : "kknow",
      endpoint: null,
      region: null,
      prefix: null,
      accessKeyId: null,
    },
  });
}

/**
 * S3 fields: bucket, endpoint, region, prefix, access key and secret.
 *
 * A button saves them, not each keystroke: half a typed endpoint is not a
 * setting yet. The secret goes to the OS keychain and never comes back, so an
 * empty field means "keep the old one".
 */
export function s3Form(vault: VaultView, remote: SyncSettings): HTMLElement {
  const s3 = remote.provider.kind === "s3" ? remote.provider : null;
  const form = document.createElement("div");
  form.className = "settings-s3";

  const fields: Record<string, HTMLInputElement> = {};
  const field = (name: string, label: Key, value: string | null, placeholder = ""): void => {
    const line = document.createElement("label");
    const caption = document.createElement("span");
    caption.textContent = t(label);

    const input = document.createElement("input");
    input.value = value ?? "";
    input.placeholder = placeholder;
    input.autocomplete = "off";
    if (name === "secret") input.type = "password";

    fields[name] = input;
    line.append(caption, input);
    form.append(line);
  };

  field("bucket", "dialog.sync.bucket", s3?.bucket ?? "");
  field("endpoint", "dialog.sync.endpoint", s3?.endpoint ?? null, t("dialog.sync.endpoint.placeholder"));
  field("region", "dialog.sync.region", s3?.region ?? null, t("dialog.sync.region.placeholder"));
  field("prefix", "dialog.sync.prefix", s3?.prefix ?? null, t("dialog.sync.prefix.placeholder"));
  field("accessKeyId", "dialog.sync.key", s3?.accessKeyId ?? null, t("dialog.sync.key.placeholder"));
  field(
    "secret",
    "dialog.sync.secret",
    "",
    t(remote.hasSecret ? "dialog.sync.secret.saved" : "dialog.sync.secret.placeholder"),
  );

  const tools = document.createElement("menu");
  const save = document.createElement("button");
  save.className = "primary";
  save.textContent = t("dialog.sync.save");
  save.onclick = () => {
    const text = (name: string): string | null => fields[name].value.trim() || null;
    void saveSync(
      vault,
      remote,
      {
        provider: {
          kind: "s3",
          bucket: fields.bucket.value.trim(),
          endpoint: text("endpoint"),
          region: text("region"),
          prefix: text("prefix"),
          accessKeyId: text("accessKeyId"),
        },
      },
      text("secret"),
    );
  };

  tools.append(save);
  form.append(tools);
  return form;
}

export async function pickFolder(vault: VaultView, remote: SyncSettings | null): Promise<void> {
  const path = await open({ directory: true, multiple: false });
  if (typeof path !== "string") return;

  await saveSync(vault, remote, { provider: { kind: "folder", path } });
}

/** Writes the vault sync settings with one field changed. */
export async function saveSync(
  vault: VaultView,
  remote: SyncSettings | null,
  change: Partial<{ provider: Provider; policy: ConflictPolicy; autoSyncMinutes: number }>,
  secret: string | null = null,
): Promise<void> {
  const base = {
    provider: remote?.provider ?? { kind: "folder" as const, path: "" },
    policy: remote?.policy ?? ("keepBoth" as ConflictPolicy),
    autoSyncMinutes: remote?.autoSyncMinutes ?? 0,
  };

  try {
    await api.saveSyncSettings({ ...base, ...change }, secret, vault.id);
  } catch (error) {
    console.error(error);
  }
  await redraw();
}
