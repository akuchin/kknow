import { open } from "@tauri-apps/plugin-dialog";

import {
  api,
  type ConflictPolicy,
  type GitAuth,
  type Provider,
  type SyncSettings,
  type VaultView,
} from "../api";
import { bindDraft, clearDraftsUnder, draftValue, setDraft } from "../drafts";
import { describeError, t, type Key } from "../i18n";
import { showError } from "./errors";

let redraw: () => Promise<void> = async () => {};

export function initSyncForm(onChange: () => Promise<void>): void {
  redraw = onChange;
}

const gitDrafts = new Set<string>();

export function draftsGit(vault: VaultView): boolean {
  return gitDrafts.has(vault.id);
}

function formKey(vault: VaultView, kind: "s3" | "git"): string {
  return `sync:${vault.id}:${kind}:`;
}

export async function switchProvider(
  vault: VaultView,
  remote: SyncSettings | null,
  kind: string,
): Promise<void> {
  gitDrafts.delete(vault.id);
  if (kind !== remote?.provider.kind) {
    clearDraftsUnder(formKey(vault, "s3"));
    clearDraftsUnder(formKey(vault, "git"));
  }
  if (kind === "git") {
    if (remote?.provider.kind !== "git") gitDrafts.add(vault.id);
    await redraw();
    return;
  }

  if (kind === "off") {
    try {
      await api.forgetSyncSettings(vault.id);
    } catch (error) {
      showError(error);
    }
    await redraw();
    return;
  }

  if (kind === "folder") {
    await pickFolder(vault, remote);
    return;
  }

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
    bindDraft(input, formKey(vault, "s3") + name, value ?? "");
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
      () => clearDraftsUnder(formKey(vault, "s3")),
    );
  };

  tools.append(save);
  form.append(tools);
  return form;
}

export function gitForm(vault: VaultView, remote: SyncSettings | null): HTMLElement {
  const git = remote?.provider.kind === "git" ? remote.provider : null;
  const form = document.createElement("div");
  form.className = "settings-s3 settings-git";

  const line = (label: Key, control: HTMLElement): HTMLLabelElement => {
    const item = document.createElement("label");
    const caption = document.createElement("span");
    caption.textContent = t(label);
    item.append(caption, control);
    form.append(item);
    return item;
  };
  const input = (value: string, placeholder: string): HTMLInputElement => {
    const field = document.createElement("input");
    field.value = value;
    field.placeholder = placeholder;
    field.autocomplete = "off";
    field.spellcheck = false;
    return field;
  };

  const drafts = formKey(vault, "git");
  const url = input(git?.url ?? "", t("dialog.git.url.placeholder"));
  if (git === null) bindDraft(url, `${drafts}url`);
  url.readOnly = git !== null;
  line("dialog.git.url", url);

  const auth = document.createElement("select");
  for (const value of ["system", "token", "env"] as GitAuth[]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(`dialog.git.auth.${value}` as Key);
    auth.append(option);
  }
  const savedAuth = git?.auth ?? "system";
  auth.value = draftValue(`${drafts}auth`, savedAuth);
  line("dialog.git.auth", auth);

  const env = input(git?.env ?? "", "GITHUB_TOKEN");
  bindDraft(env, `${drafts}env`, git?.env ?? "");
  const envLine = line("dialog.git.env", env);
  const token = input(
    "",
    t(remote?.hasSecret ? "dialog.sync.secret.saved" : "dialog.git.token.placeholder"),
  );
  token.type = "password";
  bindDraft(token, `${drafts}token`);
  const tokenLine = line("dialog.git.token", token);

  const show = () => {
    envLine.hidden = auth.value !== "env";
    tokenLine.hidden = auth.value !== "token";
  };
  auth.onchange = () => {
    setDraft(`${drafts}auth`, auth.value, savedAuth);
    show();
  };
  show();

  const hint = document.createElement("span");
  hint.className = "settings-form-hint muted";
  hint.textContent = t(git ? "dialog.git.hint.connected" : "dialog.git.hint.new");
  form.append(hint);

  const problem = document.createElement("span");
  problem.className = "settings-form-error";
  problem.hidden = true;
  form.append(problem);

  const tools = document.createElement("menu");
  const save = document.createElement("button");
  save.className = "primary";
  save.textContent = t(git ? "dialog.sync.save" : "dialog.git.connect");
  save.onclick = async () => {
    save.disabled = true;
    problem.hidden = true;
    save.textContent = t("dialog.git.connecting");
    try {
      await api.connectGit(
        vault.id,
        url.value.trim() || null,
        auth.value as GitAuth,
        env.value.trim() || null,
        token.value.trim() || null,
      );
      gitDrafts.delete(vault.id);
      clearDraftsUnder(drafts);
      await redraw();
    } catch (error) {
      problem.textContent = describeError(error);
      problem.hidden = false;
      save.disabled = false;
      save.textContent = t(git ? "dialog.sync.save" : "dialog.git.connect");
    }
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

export async function saveSync(
  vault: VaultView,
  remote: SyncSettings | null,
  change: Partial<{ provider: Provider; policy: ConflictPolicy; autoSyncMinutes: number }>,
  secret: string | null = null,
  onSaved?: () => void,
): Promise<boolean> {
  const base = {
    provider: remote?.provider ?? { kind: "folder" as const, path: "" },
    policy: remote?.policy ?? ("keepBoth" as ConflictPolicy),
    autoSyncMinutes: remote?.autoSyncMinutes ?? 0,
  };

  let saved = false;
  try {
    await api.saveSyncSettings({ ...base, ...change }, secret, vault.id);
    saved = true;
    onSaved?.();
  } catch (error) {
    showError(error);
  }
  await redraw();
  return saved;
}
