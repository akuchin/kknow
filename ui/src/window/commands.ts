import { api, type CommandTarget, type CommandView, type RunLine, type RunOutcome } from "../api";
import { commandProblemText } from "../core-text";
import { t } from "../i18n";
import type { MenuItem, MenuSection } from "../menu";
import { el } from "./dom";
import { flushPendingSave } from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";

export function initCommands(): void {
  state.on("note", () => {
    void refreshNoteCommands();
    renderRunButton();
  });
  el.runHide.onclick = () => showRunOutput(false);
  el.runStop.onclick = () => {
    if (live) void api.stopCommand(live.run).catch(fail);
  };
}

export async function commandItems(target: CommandTarget): Promise<MenuItem[] | null> {
  let commands: CommandView[] = [];
  try {
    const listed = await api.listCommands(target);
    commands = listed.commands;
    if (listed.problems.length > 0) setStatus(commandProblemText(listed.problems[0]), "error");
  } catch (error) {
    fail(error);
    return null;
  }

  return commands.length > 0
    ? commands.map((command, index) => ({
        label: command.name,
        detail: command.kind,
        glyph: "terminal" as const,
        key: index < 9 ? String(index + 1) : undefined,
        run: () => void runCommand(command, target),
      }))
    : [{ label: t("commands.none"), disabled: true, run: () => {} }];
}

export function editCommandsSection(): MenuSection {
  return {
    items: [
      {
        label: t("commands.edit"),
        detail: t("commands.edit.detail"),
        glyph: "code",
        key: "E",
        run: () => void editCommands(),
      },
    ],
  };
}

let live: { run: string; command: CommandView; started: number; timer: number } | null = null;
let runs = 0;

const OPEN_LIVE_AFTER_MS = 1000;

export async function runCommand(command: CommandView, target: CommandTarget): Promise<void> {
  if (command.confirm && !window.confirm(t("commands.confirm", { name: command.name }))) {
    return;
  }

  await flushPendingSave();
  localStorage.setItem(LAST_COMMAND, command.id);
  renderRunButton();
  setStatus(t("commands.running", { name: command.name }), "work");

  const run = `${Date.now()}-${++runs}`;
  startLive(run, command);
  try {
    const outcome = await api.runCommand(command.id, target, run, (line) => {
      if (live?.run === run) appendLine(line);
    });
    showRunOutcome(command, outcome, live?.run === run);
  } catch (error) {
    if (live?.run === run) endLive();
    fail(error);
  }
}

function startLive(run: string, command: CommandView): void {
  if (live) window.clearInterval(live.timer);
  const started = Date.now();
  const tick = () => {
    const seconds = ((Date.now() - started) / 1000).toFixed(0);
    el.runSummary.textContent = t("panel.run.running", { seconds });
  };
  live = { run, command, started, timer: window.setInterval(tick, 1000) };
  window.setTimeout(() => {
    if (live?.run === run) showRunOutput(true);
  }, OPEN_LIVE_AFTER_MS);

  el.runTitle.textContent = command.name;
  el.runFloat.dataset.tone = "work";
  el.runOutput.replaceChildren();
  el.runStop.hidden = false;
  tick();
  status.setRun(command.name, "work", t("statusbar.run.title", { summary: t("panel.run.running", { seconds: 0 }) }));
}

function endLive(): void {
  if (live) window.clearInterval(live.timer);
  live = null;
  el.runStop.hidden = true;
}

function appendLine(line: RunLine): void {
  const output = el.runOutput;
  const atEnd = output.scrollHeight - output.scrollTop - output.clientHeight < 4;

  const row = document.createElement("span");
  if (line.stream === "stderr") row.className = "run-stderr";
  row.textContent = `${line.text}\n`;
  output.append(row);

  if (atEnd) output.scrollTop = output.scrollHeight;
}

function showRunOutcome(command: CommandView, outcome: RunOutcome, current: boolean): void {
  const seconds = (outcome.millis / 1000).toFixed(1);
  const failed = outcome.timedOut || outcome.stopped || outcome.code !== 0;
  const summary = outcome.stopped
    ? t("panel.run.stopped", { seconds })
    : outcome.timedOut
      ? t("panel.run.timedOut", { seconds })
      : outcome.code === null
        ? t("panel.run.killed", { seconds })
        : outcome.code === 0
          ? t("panel.run.ok", { seconds })
          : t("panel.run.failed", { code: outcome.code, seconds });

  setStatus(`${command.name}: ${summary}`, failed ? "error" : "idle");
  if (!current) return;
  endLive();

  el.runSummary.textContent = summary;
  el.runFloat.dataset.tone = failed ? "error" : "ok";
  if (outcome.truncated) {
    const note = document.createElement("span");
    note.className = "run-note";
    note.textContent = `\n[${t("panel.run.truncated")}]`;
    el.runOutput.append(note);
  } else if (el.runOutput.childElementCount === 0) {
    el.runOutput.textContent = t("panel.run.empty");
  }

  status.setRun(command.name, failed ? "error" : "ok", t("statusbar.run.title", { summary }));
  if (failed && !outcome.stopped) showRunOutput(true);
}

export function showRunOutput(show: boolean): void {
  el.runFloat.hidden = !show;
  status.setRunOpen(show);
}

async function editCommands(): Promise<void> {
  try {
    await api.openSettings("commands");
  } catch (error) {
    fail(error);
  }
}

let noteCommands: CommandView[] = [];
let noteCommandsFor: string | null = null;

async function refreshNoteCommands(): Promise<void> {
  const target = currentTarget();
  const key = target ? (target.file ?? `${target.vault}/${target.name}`) : null;
  if (key === noteCommandsFor) return;

  noteCommandsFor = key;
  noteCommands = [];
  if (!target) return;

  try {
    const listed = await api.listCommands(target);
    if (noteCommandsFor === key) {
      noteCommands = listed.commands;
      renderRunButton();
    }
  } catch (error) {
    fail(error);
  }
}

export function currentTarget(): CommandTarget | null {
  if (!state.note) return null;
  if (state.note.file) return { kind: "note", file: state.note.id };
  return state.vault ? { kind: "note", name: state.note.id, vault: state.vault.id } : null;
}

const LAST_COMMAND = "kknow:command:last";

export function lastCommand(): CommandView | null {
  const id = localStorage.getItem(LAST_COMMAND);
  return noteCommands.find((command) => command.id === id) ?? null;
}

function renderRunButton(): void {
  if (!state.note) return;
  const last = lastCommand();
  const title = last
    ? t("topbar.commands.run", { name: last.name })
    : t("topbar.commands.title");
  el.runLast.title = title;
  el.runLast.setAttribute("aria-label", title);
}

export function openNoteCommands(): CommandView[] {
  return noteCommands;
}
