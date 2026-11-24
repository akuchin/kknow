import { api, type SyncSettings, type SyncStatus, type VaultView } from "../api";
import {
  bindDraft,
  bindDraftCheck,
  clearDraftsUnder,
  draftChecked,
} from "../drafts";
import { describeError, t, type Key } from "../i18n";
import { row } from "./controls";
import { showError } from "./errors";

let redraw: () => Promise<void> = async () => {};

export function initEncryption(onChange: () => Promise<void>): void {
  redraw = onChange;
}

const forms = new Map<string, "on" | "change">();

const codes = new Map<string, string>();

const LOCKED = new Set(["storageLocked", "wrongStorageKey"]);

function drafts(vault: VaultView): string {
  return `encryption:${vault.id}:`;
}

export function encryptionSection(
  vault: VaultView,
  remote: SyncSettings,
  status: SyncStatus | undefined,
): HTMLElement[] {
  const code = codes.get(vault.id);
  if (code) return [recoveryPanel(vault, code)];

  const locked =
    (remote.encrypted === true && !remote.hasKey) ||
    LOCKED.has(status?.message?.code ?? "");
  if (locked) return [unlockForm(vault)];

  const on = remote.encrypted === true;
  const open = forms.get(vault.id);
  const toggle = document.createElement("button");
  toggle.className = "ghost";
  toggle.textContent = t(on ? "settings.encryption.change" : "settings.encryption.turnOn");
  toggle.disabled = open !== undefined;
  toggle.onclick = () => {
    forms.set(vault.id, on ? "change" : "on");
    void redraw();
  };

  const items = [
    row(
      t("settings.encryption"),
      toggle,
      t(on ? "settings.encryption.on" : "settings.encryption.off"),
    ),
  ];
  if (open === "on") items.push(turnOnForm(vault));
  if (open === "change") items.push(changeForm(vault));
  return items;
}

function form(className: string, hint: Key): {
  root: HTMLElement;
  field: (name: string, label: Key) => HTMLInputElement;
  problem: HTMLElement;
  tools: HTMLElement;
} {
  const root = document.createElement("div");
  root.className = `settings-s3 settings-encryption ${className}`;

  const text = document.createElement("span");
  text.className = "settings-form-hint muted";
  text.textContent = t(hint);
  root.append(text);

  const problem = document.createElement("span");
  problem.className = "settings-form-error";
  problem.hidden = true;

  const tools = document.createElement("menu");
  return {
    root,
    problem,
    tools,
    field: (name, label) => {
      const line = document.createElement("label");
      const caption = document.createElement("span");
      caption.textContent = t(label);
      const input = document.createElement("input");
      input.type = "password";
      input.autocomplete = "new-password";
      input.dataset.field = name;
      line.append(caption, input);
      root.append(line);
      return input;
    },
  };
}

function button(label: Key, primary = false): HTMLButtonElement {
  const item = document.createElement("button");
  item.textContent = t(label);
  if (primary) item.className = "primary";
  return item;
}

async function submit(
  action: HTMLButtonElement,
  problem: HTMLElement,
  work: () => Promise<void>,
): Promise<void> {
  action.disabled = true;
  problem.hidden = true;
  try {
    await work();
  } catch (error) {
    problem.textContent = describeError(error);
    problem.hidden = false;
    action.disabled = false;
  }
}

function mismatch(problem: HTMLElement): void {
  problem.textContent = t("settings.encryption.mismatch");
  problem.hidden = false;
}

function cancelButton(vault: VaultView): HTMLButtonElement {
  const cancel = button("dialog.sync.cancel");
  cancel.onclick = () => {
    forms.delete(vault.id);
    clearDraftsUnder(drafts(vault));
    void redraw();
  };
  return cancel;
}

function turnOnForm(vault: VaultView): HTMLElement {
  const { root, field, problem, tools } = form("settings-encryption-on", "settings.encryption.hint");
  const passphrase = field("passphrase", "settings.encryption.passphrase");
  const repeat = field("repeat", "settings.encryption.repeat");
  bindDraft(passphrase, `${drafts(vault)}passphrase`);
  bindDraft(repeat, `${drafts(vault)}repeat`);

  const action = button("settings.encryption.encrypt", true);
  action.onclick = () => {
    if (passphrase.value !== repeat.value) return mismatch(problem);
    void submit(action, problem, async () => {
      action.textContent = t("settings.encryption.encrypting");
      try {
        const on = await api.syncEncrypt(passphrase.value, vault.id);
        codes.set(vault.id, on.recoveryCode);
        forms.delete(vault.id);
        clearDraftsUnder(drafts(vault));
        await redraw();
      } finally {
        action.textContent = t("settings.encryption.encrypt");
      }
    });
  };
  tools.append(cancelButton(vault), action);
  root.append(problem, tools);
  return root;
}

function changeForm(vault: VaultView): HTMLElement {
  const { root, field, problem, tools } = form(
    "settings-encryption-change",
    "settings.encryption.change.hint",
  );
  const current = field("current", "settings.encryption.current");
  const passphrase = field("passphrase", "settings.encryption.new");
  const repeat = field("repeat", "settings.encryption.repeat");
  current.autocomplete = "current-password";
  bindDraft(current, `${drafts(vault)}current`);
  bindDraft(passphrase, `${drafts(vault)}new`);
  bindDraft(repeat, `${drafts(vault)}repeat`);

  const action = button("dialog.sync.save", true);
  action.onclick = () => {
    if (passphrase.value !== repeat.value) return mismatch(problem);
    void submit(action, problem, async () => {
      await api.syncChangePassphrase(current.value, passphrase.value, vault.id);
      forms.delete(vault.id);
      clearDraftsUnder(drafts(vault));
      await redraw();
    });
  };
  tools.append(cancelButton(vault), action);
  root.append(problem, tools);
  return root;
}

function unlockForm(vault: VaultView): HTMLElement {
  const { root, field, problem, tools } = form(
    "settings-encryption-unlock",
    "settings.encryption.locked",
  );
  const secret = field("secret", "settings.encryption.secret");
  secret.autocomplete = "current-password";
  bindDraft(secret, `${drafts(vault)}secret`);

  const action = button("settings.encryption.unlock", true);
  action.onclick = () =>
    void submit(action, problem, async () => {
      await api.syncUnlock(secret.value, vault.id);
      clearDraftsUnder(drafts(vault));
      try {
        await api.syncNow(vault.id);
      } catch (error) {
        showError(error);
      }
      await redraw();
    });
  secret.onkeydown = (event) => {
    if (event.key === "Enter") action.click();
  };
  tools.append(action);
  root.append(problem, tools);
  return root;
}

function recoveryPanel(vault: VaultView, code: string): HTMLElement {
  const root = document.createElement("div");
  root.className = "settings-s3 settings-encryption settings-recovery";

  const hint = document.createElement("span");
  hint.className = "settings-form-hint";
  hint.textContent = t("settings.encryption.code.hint");

  const value = document.createElement("code");
  value.className = "settings-recovery-code";
  value.textContent = code;

  const copy = button("settings.encryption.code.copy");
  copy.onclick = () => {
    void navigator.clipboard.writeText(code).then(
      () => (copy.textContent = t("settings.encryption.code.copied")),
      showError,
    );
  };

  const confirm = document.createElement("label");
  confirm.className = "settings-recovery-confirm";
  const box = document.createElement("input");
  box.type = "checkbox";
  const key = `${drafts(vault)}saved`;
  bindDraftCheck(box, key, false);
  const caption = document.createElement("span");
  caption.textContent = t("settings.encryption.code.saved");
  confirm.append(box, caption);

  const done = button("settings.encryption.code.done", true);
  done.disabled = !draftChecked(key, false);
  box.addEventListener("change", () => (done.disabled = !box.checked));
  done.onclick = () => {
    codes.delete(vault.id);
    clearDraftsUnder(drafts(vault));
    void redraw();
  };

  const tools = document.createElement("menu");
  tools.append(copy, done);
  root.append(hint, value, confirm, tools);
  return root;
}
