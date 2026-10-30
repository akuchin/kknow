// The changes view of a git vault: what is staged, what is changed, the
// commit message, the sync button and the diff of the picked file.
//
// It covers the editor and the preview, as the trash view does: a narrow list
// on the left and the diff on the right, which needs the width. The view asks
// the core for the state each time it opens and after each action; it keeps
// only the picked file and the message. The diff is plain `git diff` text for
// now. The module does not touch the DOM on import.

import { api, type ChangeKind, type GitChange, type GitStatus } from "../api";
import { iconMarkup } from "../icons";
import { describeError, t, type Key } from "../i18n";
import { el } from "./dom";
import { openNote } from "./note";
import { fail, setStatus } from "./status";

/** Steps of higher zones that the panel starts. */
export interface GitHooks {
  /** The full sync of the vault: fetch, merge, push. */
  sync: () => Promise<void>;
  /** The trash view takes the same place: the two do not stack. */
  closeTrashView: () => void;
}

let hooks: GitHooks;

/** The message survives a redraw: the list redraws after each click. */
let message = "";

/** The file whose diff is shown: its side and path. */
let picked: { side: Side; path: string } | null = null;

/** Only the last diff request paints: a fast second click wins. */
let diffRequest = 0;

type Side = "staged" | "unstaged";

/** Takes the hooks and wires the fixed buttons. */
export function initGit(calls: GitHooks): void {
  hooks = calls;
  el.gitHide.onclick = () => hideGit();
  el.gitSync.onclick = () => void hooks.sync();
  // A note saved here or a file changed outside: the list must follow.
  document.addEventListener("kknow:note-saved", () => void refreshGit());
  window.addEventListener("focus", () => void refreshGit());
}

/** Shows or hides the view. */
export async function toggleGit(): Promise<void> {
  if (!el.gitView.hidden) {
    hideGit();
    return;
  }
  await showGit();
}

/** Opens the view with a fresh state. */
export async function showGit(): Promise<void> {
  hooks.closeTrashView();
  el.gitView.hidden = false;
  await refreshGit();
}

/** Closes the view; the open note under it shows again. */
export function hideGit(): void {
  if (el.gitView.hidden) return;
  el.gitView.hidden = true;
  picked = null;
  el.gitDiffHead.replaceChildren();
  el.gitDiffBody.replaceChildren();
}

/** Asks the core again, if the view is open. */
export async function refreshGit(): Promise<void> {
  if (el.gitView.hidden) return;
  try {
    paint(await api.gitStatus());
  } catch (error) {
    paintError(error);
  }
}

/** Runs one action and paints the state that it returns. */
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

  // The picked file may have moved to the other list or gone with a commit.
  // Then the diff follows it, or the first file of the lists.
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

/** Marks a file in the lists and shows its diff; `null` clears the diff. */
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

/**
 * One span per line, with a class for the kind: added, removed, a hunk
 * header or the file header. Text nodes only, so the file text is never HTML.
 */
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

/**
 * One list: the staged files with "−", or the changed ones with "+". The
 * header button moves the whole list.
 */
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

/** The one-letter mark of a change, as in `git status --short`. */
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

  // A click shows the diff. A double click on a note opens it: the view
  // closes, as it does for any note the user opens.
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

  item.append(mark, name, move);
  return item;
}

/** A vault note opens by its ID: the path without `.md`. */
function noteOf(path: string): string | null {
  const match = /^(.*)\.(md|markdown)$/i.exec(path);
  return match ? match[1] : null;
}

/** The message and the button. Mod+Enter in the field commits too. */
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
