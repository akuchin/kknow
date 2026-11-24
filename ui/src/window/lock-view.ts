import { api, type NoteView } from "../api";
import { isErrorView } from "../error-view";
import { describeError, t, type Key } from "../i18n";
import { icon } from "../icons";
import { el } from "./dom";
import { editor } from "./editor";
import { hasPassword, isUnlocked, onLockChange, refreshLock } from "./locks";
import { applyNote, flushPendingSave, openFound } from "./note";
import { state } from "./state";
import { fail, setStatus } from "./status";
import { LOCK_ALL_EVENT, refreshTree, renderTree } from "./tree";
import { copyText } from "./util";

const MIN_PASSWORD = 8;

type Mode = "unlock" | "setup" | "confirm" | "code" | "reset";

let panel: {
  mode: Mode;
  vault: string;
  note: string;
  then?: () => Promise<void>;
  code?: string;
} | null = null;

export function initLockView(): void {
  state.on("note", (note) => {
    showLock(note);
    if (!note || note.file) {
      hidePanel();
      return;
    }
    if (panel && (panel.note !== note.id || panel.vault !== state.vault?.id)) hidePanel();
    if (note.closed) {
      if (panel?.mode !== "unlock") showPanel("unlock", note.id);
    } else if (panel?.mode === "unlock") {
      hidePanel();
    }
  });
  state.on("vault", (vault) => {
    if (vault) void refreshLock(vault.id).catch(fail);
  });
  onLockChange(() => {
    renderTree();
    showLock(state.note);
  });

  document.addEventListener(LOCK_ALL_EVENT, (event) => {
    void lockAll((event as CustomEvent<string>).detail);
  });
  el.lockForm.onsubmit = (event) => {
    event.preventDefault();
    void submit();
  };
  el.lockCancel.onclick = () => hidePanel();
  el.lockCopy.onclick = () => void copyText(panel?.code ?? null);
  el.noteLock.onclick = () => {
    const note = state.note;
    const vault = state.vault?.id;
    if (!note || !vault) return;
    if (note.closed) el.lockSecret.focus();
    else void lockAll(vault);
  };
  el.lockView.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && panel && panel.mode !== "unlock" && panel.mode !== "code") {
      hidePanel();
    }
  });
}

function showLock(note: NoteView | null): void {
  const locked = note?.locked === true && !note.file;
  el.noteLock.hidden = !locked;
  if (!locked) return;
  const open = !note.closed;
  const label = t(open ? "lock.icon.open" : "lock.icon.closed");
  el.noteLock.replaceChildren(icon(open ? "lockOpen" : "lock"));
  el.noteLock.dataset.open = String(open);
  el.noteLock.title = label;
  el.noteLock.setAttribute("aria-label", label);
}

const TITLES: Record<Mode, Key> = {
  unlock: "lock.unlock.title",
  setup: "lock.setup.title",
  confirm: "lock.confirm.title",
  code: "lock.code.title",
  reset: "lock.reset.title",
};

const TEXTS: Record<Mode, Key> = {
  unlock: "lock.unlock.text",
  setup: "lock.setup.text",
  confirm: "lock.confirm.text",
  code: "lock.code.text",
  reset: "lock.reset.text",
};

function showPanel(
  mode: Mode,
  note: string,
  extra: { then?: () => Promise<void>; code?: string; submit?: Key } = {},
): void {
  const vault = state.vault?.id ?? "";
  panel = { mode, vault, note, then: extra.then, code: extra.code };
  el.lockTitle.textContent = t(TITLES[mode]);
  el.lockText.textContent = t(TEXTS[mode]);
  const asks = mode !== "code";
  const twice = mode === "setup" || mode === "reset";
  el.lockSecret.hidden = !asks;
  el.lockRepeat.hidden = !twice;
  el.lockSecret.value = "";
  el.lockRepeat.value = "";
  el.lockSecret.placeholder = t(
    mode === "unlock" || mode === "confirm" ? "lock.field.secret" : "lock.field.new",
  );
  el.lockRepeat.placeholder = t("lock.field.repeat");
  el.lockCode.hidden = mode !== "code";
  el.lockCode.textContent = extra.code ?? "";
  el.lockCopy.hidden = mode !== "code";
  el.lockCancel.hidden = mode === "unlock" || mode === "code";
  el.lockSubmit.textContent = t(extra.submit ?? submitLabel(mode));
  showError(null);
  el.lockView.dataset.mode = mode;
  el.lockView.hidden = false;
  if (asks) el.lockSecret.focus();
  else el.lockSubmit.focus();
}

function submitLabel(mode: Mode): Key {
  switch (mode) {
    case "unlock":
      return "lock.unlock.submit";
    case "setup":
      return "lock.setup.submit";
    case "confirm":
      return "lock.confirm.submit";
    case "code":
      return "lock.code.done";
    case "reset":
      return "lock.reset.submit";
  }
}

function hidePanel(): void {
  panel = null;
  el.lockView.hidden = true;
  el.lockSecret.value = "";
  el.lockRepeat.value = "";
  if (state.note && !state.note.closed) editor.focus();
}

function showError(text: string | null): void {
  el.lockError.hidden = text === null;
  el.lockError.textContent = text ?? "";
}

function newPassword(): string | null {
  const password = el.lockSecret.value;
  if ([...password].length < MIN_PASSWORD) {
    showError(t("lock.error.short", { count: MIN_PASSWORD }));
    return null;
  }
  if (password !== el.lockRepeat.value) {
    showError(t("lock.error.mismatch"));
    return null;
  }
  return password;
}

async function submit(): Promise<void> {
  const current = panel;
  if (!current) return;
  el.lockSubmit.disabled = true;
  try {
    switch (current.mode) {
      case "unlock":
      case "confirm": {
        const secret = el.lockSecret.value;
        if (!secret) return;
        const by = await api.unlockNotes(secret, current.vault);
        await refreshLock(current.vault);
        if (by === "recoveryCode") {
          showPanel("reset", current.note, { then: current.then });
          await reopen(current.vault, current.note);
          return;
        }
        hidePanel();
        if (current.then) await current.then();
        else await reopen(current.vault, current.note);
        setStatus(t("status.notesUnlocked"));
        return;
      }
      case "setup": {
        const password = newPassword();
        if (!password) return;
        const code = await api.setLockPassword(password, current.vault);
        await refreshLock(current.vault);
        if (current.then) await current.then();
        showPanel("code", current.note, { code });
        return;
      }
      case "reset": {
        const password = newPassword();
        if (!password) return;
        await api.changeLockPassword(password, current.vault);
        hidePanel();
        if (current.then) await current.then();
        setStatus(t("status.lockPasswordChanged"));
        return;
      }
      case "code":
        hidePanel();
        return;
    }
  } catch (error) {
    const wrong = isErrorView(error) && error.code === "wrongPassphrase";
    showError(wrong ? t("lock.error.wrong") : describeError(error));
    el.lockSecret.select();
  } finally {
    el.lockSubmit.disabled = false;
  }
}

async function reopen(vault: string, id: string): Promise<void> {
  if (!state.isOpen(vault, id)) return;
  applyNote(await api.loadNote(id), { reload: true });
}

async function showTarget(id: string, vault: string): Promise<boolean> {
  if (state.isOpen(vault, id) && !state.note?.file) return true;
  await openFound(id, vault);
  return state.isOpen(vault, id);
}

export async function lockNote(id: string, vault: string): Promise<void> {
  if (!(await showTarget(id, vault))) return;
  await flushPendingSave();
  try {
    const lock = await refreshLock(vault);
    const run = async () => {
      const view = await api.lockNote(id, vault);
      if (state.isOpen(vault, id)) {
        applyNote(view, { keepEditor: true });
      }
      await refreshTree(vault);
      setStatus(t("status.noteLocked", { name: leaf(id) }));
    };
    if (lock.unlocked) await run();
    else if (lock.password) showPanel("confirm", id, { then: run, submit: "lock.confirm.lock" });
    else showPanel("setup", id, { then: run });
  } catch (error) {
    fail(error);
  }
}

export async function removeLock(id: string, vault: string): Promise<void> {
  if (!(await showTarget(id, vault))) return;
  await flushPendingSave();
  const run = async () => {
    const view = await api.removeLock(id, vault);
    if (state.isOpen(vault, id)) {
      applyNote(view, { reload: true });
    }
    await refreshTree(vault);
    setStatus(t("status.lockRemoved", { name: leaf(id) }));
  };
  try {
    if (isUnlocked(vault)) await run();
    else showPanel("confirm", id, { then: run, submit: "lock.confirm.remove" });
  } catch (error) {
    fail(error);
  }
}

export async function lockAll(vault: string | undefined = state.vault?.id): Promise<void> {
  if (!vault) return;
  await flushPendingSave();
  try {
    await api.lockAll(vault);
    await refreshLock(vault);
    const note = state.note;
    if (note?.locked && state.vault?.id === vault) await reopen(vault, note.id);
    setStatus(t("status.notesLocked"));
  } catch (error) {
    fail(error);
  }
}

export function canLockAll(): boolean {
  return isUnlocked(state.vault?.id) && hasPassword(state.vault?.id);
}

function leaf(id: string): string {
  return id.slice(id.lastIndexOf("/") + 1);
}
