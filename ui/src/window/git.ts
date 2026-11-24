import { api, type ChangeKind, type GitChange, type GitStatus } from "../api";
import { iconMarkup } from "../icons";
import { describeError, t, type Key } from "../i18n";
import { el } from "./dom";
import { flushPendingSave, openNote } from "./note";
import { state } from "./state";
import { fail, setStatus } from "./status";

export interface GitHooks {
  sync: () => Promise<void>;
  closeTrashView: () => void;
  refreshTree: (vaultId: string) => Promise<void>;
}

let hooks: GitHooks;

let message = "";

let picked: { side: Side; path: string } | null = null;

let diffRequest = 0;

type Side = "staged" | "unstaged";

export function initGit(calls: GitHooks): void {
  hooks = calls;
  el.gitHide.onclick = () => hideGit();
  el.gitSync.onclick = () => void hooks.sync();
  document.addEventListener("kknow:note-saved", () => void refreshGit());
  window.addEventListener("focus", () => void refreshGit());
}

export async function toggleGit(): Promise<void> {
  if (!el.gitView.hidden) {
    hideGit();
    return;
  }
  await showGit();
}

export async function showGit(): Promise<void> {
  hooks.closeTrashView();
  el.gitView.hidden = false;
  await refreshGit();
}

export function hideGit(): void {
  if (el.gitView.hidden) return;
  el.gitView.hidden = true;
  picked = null;
  el.gitDiffHead.replaceChildren();
  el.gitDiffBody.replaceChildren();
}

export async function refreshGit(): Promise<void> {
  if (el.gitView.hidden) return;
  try {
    paint(await api.gitStatus());
  } catch (error) {
    paintError(error);
  }
}

async function act(action: () => Promise<GitStatus>): Promise<void> {
  try {
    paint(await action());
  } catch (error) {
    fail(error);
    await refreshGit();
  }
}

function paintError(error: unknown): void {
  el.gitSummary.textContent = describeError(error);
  el.gitBody.replaceChildren();
  pick(null);
}

function paint(status: GitStatus): void {
  const parts = [status.branch ?? t("panel.git.detached")];
  if (status.ahead > 0) parts.push(t("panel.git.ahead", { count: status.ahead }));
  if (status.behind > 0) parts.push(t("panel.git.behind", { count: status.behind }));
  el.gitSummary.textContent = parts.join(" · ");

  const staged = status.changes.filter((change) => change.staged);
  const unstaged = status.changes.filter((change) => change.unstaged);

  if (staged.length === 0 && unstaged.length === 0) {
    const empty = document.createElement("p");
    empty.className = "muted";
    empty.textContent = t("panel.git.clean");
    el.gitBody.replaceChildren(empty);
    pick(null);
    return;
  }

  el.gitBody.replaceChildren(
    group("panel.git.staged", staged, "staged"),
    group("panel.git.changes", unstaged, "unstaged"),
    commitForm(staged.length > 0),
  );

  const sides: [Side, GitChange[]][] = [
    ["staged", staged],
    ["unstaged", unstaged],
  ];
  const next =
    sides.find(([side, list]) => side === picked?.side && list.some((c) => c.path === picked?.path)) ??
    sides.find(([, list]) => list.some((c) => c.path === picked?.path)) ??
    sides.find(([, list]) => list.length > 0)!;
  const change = next[1].find((c) => c.path === picked?.path) ?? next[1][0];
  pick({ side: next[0], change });
}

function pick(target: { side: Side; change: GitChange } | null): void {
  picked = target && { side: target.side, path: target.change.path };
  for (const item of el.gitBody.querySelectorAll<HTMLLIElement>("li[data-path]")) {
    const on = item.dataset.side === picked?.side && item.dataset.path === picked?.path;
    item.classList.toggle("picked", on);
    item.setAttribute("aria-selected", String(on));
  }
  void paintDiff(target);
}

async function paintDiff(target: { side: Side; change: GitChange } | null): Promise<void> {
  const request = ++diffRequest;
  if (!target) {
    el.gitDiffHead.replaceChildren();
    el.gitDiffBody.replaceChildren(muted(t("panel.git.pick")));
    return;
  }

  const { side, change } = target;
  const title = document.createElement("span");
  title.className = "git-diff-title";
  title.textContent = change.from ? `${change.from} → ${change.path}` : change.path;
  const where = document.createElement("span");
  where.className = "muted";
  where.textContent = t(side === "staged" ? "panel.git.staged" : "panel.git.changes");

  let text: string;
  try {
    text = await api.gitDiff(change.path, change.from ?? null, side === "staged");
  } catch (error) {
    text = "";
    if (request === diffRequest) fail(error);
  }
  if (request !== diffRequest) return;

  el.gitDiffHead.replaceChildren(title, where);
  el.gitDiffBody.replaceChildren(...diffLines(text));
  el.gitDiffBody.scrollTop = 0;
}

function diffLines(text: string): Node[] {
  const lines = text.replace(/\n$/, "").split("\n");
  if (text.trim() === "") return [muted(t("panel.git.noDiff"))];
  let header = true;
  return lines.map((line) => {
    const span = document.createElement("span");
    if (line.startsWith("@@")) {
      header = false;
      span.className = "hunk";
    } else if (header) {
      span.className = "meta";
    } else if (line.startsWith("+")) {
      span.className = "add";
    } else if (line.startsWith("-")) {
      span.className = "del";
    }
    span.textContent = `${line}\n`;
    return span;
  });
}

function muted(text: string): HTMLElement {
  const span = document.createElement("span");
  span.className = "muted";
  span.textContent = text;
  return span;
}

function group(title: Key, changes: GitChange[], side: Side): HTMLElement {
  const section = document.createElement("section");
  section.className = "git-group";
  section.dataset.side = side;

  const head = document.createElement("h3");
  const label = document.createElement("span");
  label.textContent = `${t(title)} · ${changes.length}`;
  head.append(label);

  const paths = changes.map((change) => change.path);
  if (changes.length > 0) {
    const all = document.createElement("button");
    all.className = "ghost link";
    all.textContent = t(side === "staged" ? "panel.git.unstageAll" : "panel.git.stageAll");
    all.onclick = () =>
      void act(() => (side === "staged" ? api.gitUnstage(paths) : api.gitStage(paths)));
    head.append(all);
  }
  section.append(head);

  const list = document.createElement("ul");
  for (const change of changes) {
    list.append(changeItem(change, side));
  }
  section.append(list);
  return section;
}

const MARKS: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "!",
};

function changeItem(change: GitChange, side: Side): HTMLLIElement {
  const kind = (side === "staged" ? change.staged : change.unstaged)!;
  const item = document.createElement("li");
  item.dataset.kind = kind;
  item.dataset.side = side;
  item.dataset.path = change.path;

  const mark = document.createElement("span");
  mark.className = "git-mark";
  mark.textContent = MARKS[kind];
  mark.title = t(`panel.git.kind.${kind}` as Key);

  const note = kind === "deleted" ? null : noteOf(change.path);
  const name = document.createElement("button");
  name.className = "git-path";
  name.textContent = change.from ? `${change.from} → ${change.path}` : change.path;
  name.title = note ? `${change.path}\n${t("panel.git.openNote")}` : change.path;
  name.onclick = () => pick({ side, change });
  if (note) name.ondblclick = () => void openNote(note);

  const move = document.createElement("button");
  move.className = "ghost bare git-move";
  const label = t(side === "staged" ? "panel.git.unstage" : "panel.git.stage");
  move.title = label;
  move.setAttribute("aria-label", `${label}: ${change.path}`);
  move.innerHTML = side === "staged" ? "−" : iconMarkup("plus");
  move.onclick = () =>
    void act(() =>
      side === "staged" ? api.gitUnstage([change.path]) : api.gitStage([change.path]),
    );

  const discard = document.createElement("button");
  discard.className = "ghost bare git-move git-discard";
  discard.title = t("panel.git.discard");
  discard.setAttribute("aria-label", `${t("panel.git.discard")}: ${change.path}`);
  discard.innerHTML = iconMarkup("discard");
  discard.hidden = kind === "conflicted";
  discard.onclick = () => void discardChange(change, side);

  item.append(mark, name, discard, move);
  return item;
}

async function discardChange(change: GitChange, side: Side): Promise<void> {
  const question = side === "staged" ? "panel.git.discard.staged" : "panel.git.discard.unstaged";
  if (!window.confirm(t(question, { name: change.path }))) return;
  await flushPendingSave();
  await act(async () => {
    const status = await api.gitDiscard(change.path, change.from ?? null, side === "staged");
    setStatus(t("status.discarded"));
    if (state.vault) await hooks.refreshTree(state.vault.id);
    return status;
  });
}

function noteOf(path: string): string | null {
  const match = /^(.*)\.(md|markdown)$/i.exec(path);
  return match ? match[1] : null;
}

function commitForm(hasStaged: boolean): HTMLElement {
  const form = document.createElement("form");
  form.className = "git-commit";

  const field = document.createElement("textarea");
  field.rows = 2;
  field.placeholder = t("panel.git.message");
  field.value = message;

  const button = document.createElement("button");
  button.type = "submit";
  button.className = "primary";
  button.textContent = t("panel.git.commit");

  const update = () => {
    message = field.value;
    button.disabled = !hasStaged || message.trim() === "";
    button.title = hasStaged ? "" : t("panel.git.nothingStaged");
  };
  field.oninput = update;
  field.onkeydown = (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      form.requestSubmit();
    }
  };
  update();

  form.onsubmit = (event) => {
    event.preventDefault();
    if (button.disabled) return;
    void act(async () => {
      const status = await api.gitCommit(message);
      message = "";
      setStatus(t("status.committed"));
      return status;
    });
  };

  form.append(field, button);
  return form;
}
