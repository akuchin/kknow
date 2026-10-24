// Commands: the command items of the menus, a run and its output, and the
// commands of the open note for the palette and the run button.
//
// The module does not touch the DOM on import.

import { api, type CommandTarget, type CommandView, type RunOutcome } from "../api";
import { t } from "../i18n";
import type { MenuItem, MenuSection } from "../menu";
import { el } from "./dom";
import { flushPendingSave } from "./note";
import { state } from "./state";
import { fail, setStatus, status } from "./status";

/** Follows the open note and wires the output float. */
export function initCommands(): void {
  // The palette needs the commands of the open note and collects actions
  // synchronously. The refresh drops the old list before its first await, so the
  // run button after it does not offer the command of the previous note, and a
  // closed note leaves no commands.
  state.on("note", () => {
    void refreshNoteCommands();
    renderRunButton();
  });
  el.runHide.onclick = () => showRunOutput(false);
}

/**
 * Menu items for the commands of a target, or `null` if the core failed.
 *
 * The note menu and the standalone file menu share it, so both report a typo
 * in the commands file in the same way.
 */
export async function commandItems(target: CommandTarget): Promise<MenuItem[] | null> {
  let commands: CommandView[] = [];
  try {
    const listed = await api.listCommands(target);
    commands = listed.commands;
    // Report a typo in the commands file. Else the command just does not
    // appear in the menu, and kknow looks broken.
    if (listed.problems.length > 0) setStatus(listed.problems[0], "error");
  } catch (error) {
    fail(error);
    return null;
  }

  return commands.length > 0
    ? commands.map((command, index) => ({
        label: command.name,
        detail: command.kind,
        glyph: "terminal" as const,
        // The first nine commands run by digit: they have no letters to
        // spare, and their names are the user's.
        key: index < 9 ? String(index + 1) : undefined,
        run: () => void runCommand(command, target),
      }))
    : [{ label: t("commands.none"), disabled: true, run: () => {} }];
}

/** The last menu section: the way to the command settings. */
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

/**
 * Runs a command and shows the result.
 *
 * Pending edits are saved first: the command reads the file from disk and
 * must see what the editor shows.
 */
export async function runCommand(command: CommandView, target: CommandTarget): Promise<void> {
  if (command.confirm && !window.confirm(t("commands.confirm", { name: command.name }))) {
    return;
  }

  await flushPendingSave();
  localStorage.setItem(LAST_COMMAND, command.id);
  renderRunButton();
  setStatus(t("commands.running", { name: command.name }), "work");

  try {
    showRunOutcome(command, await api.runCommand(command.id, target));
  } catch (error) {
    fail(error);
  }
}

/**
 * Command result: a line in the status bar and the output in its float.
 *
 * The command does not touch the preview: it shows the open note, not
 * reports, and a run button must not replace it with `git status` output. A
 * success shows in the status bar and a click opens the output. A failure
 * opens the output at once, because the user usually wants to read it.
 *
 * The output goes in as text (`textContent`), not HTML: it comes from an
 * external program, and a `<script>` in it would run with app rights. This is
 * the same reason the preview strips raw HTML.
 */
function showRunOutcome(command: CommandView, outcome: RunOutcome): void {
  const seconds = (outcome.millis / 1000).toFixed(1);
  const failed = outcome.timedOut || outcome.code !== 0;
  const summary = outcome.timedOut
    ? t("panel.run.timedOut", { seconds })
    : outcome.code === null
      ? t("panel.run.killed", { seconds })
      : outcome.code === 0
        ? t("panel.run.ok", { seconds })
        : t("panel.run.failed", { code: outcome.code, seconds });

  el.runTitle.textContent = command.name;
  el.runSummary.textContent = summary;
  el.runFloat.dataset.tone = failed ? "error" : "ok";

  // Both streams in a row: scripts print to both, and we do not judge which
  // one matters more.
  const printed = [outcome.stdout, outcome.stderr].filter((part) => part.length > 0).join("\n");
  const tail = outcome.truncated ? `\n\n[${t("panel.run.truncated")}]` : "";
  el.runOutput.textContent = printed ? `${printed}${tail}` : t("panel.run.empty");

  status.setRun(command.name, failed ? "error" : "ok", t("statusbar.run.title", { summary }));
  setStatus(`${command.name}: ${summary}`, failed ? "error" : "idle");
  showRunOutput(failed);
}

/** Shows or hides the output float. The status bar segment toggles it. */
export function showRunOutput(show: boolean): void {
  el.runFloat.hidden = !show;
  status.setRunOpen(show);
}

/**
 * Command settings are a section of the settings window.
 *
 * Before, this opened Finder to edit the manifest by hand. The form knows more
 * than a text editor: the run kinds, the possible targets and the environment
 * the command gets. It also shows each problem next to the field that caused it.
 */
async function editCommands(): Promise<void> {
  try {
    await api.openSettings("commands");
  } catch (error) {
    fail(error);
  }
}

/**
 * Commands of the open note, shown by the palette.
 *
 * The list loads when the note changes and stays ready, because the palette
 * collects actions synchronously and cannot wait for the core.
 */
let noteCommands: CommandView[] = [];
/** The note the list was loaded for, as a "vault/note" key. */
let noteCommandsFor: string | null = null;

async function refreshNoteCommands(): Promise<void> {
  const target = currentTarget();
  // A standalone file is keyed by its path: it has no vault.
  const key = target ? (target.file ?? `${target.vault}/${target.name}`) : null;
  if (key === noteCommandsFor) return;

  noteCommandsFor = key;
  noteCommands = [];
  if (!target) return;

  try {
    const listed = await api.listCommands(target);
    // Another note can open during the core call; then the answer is stale.
    if (noteCommandsFor === key) {
      noteCommands = listed.commands;
      renderRunButton();
    }
  } catch (error) {
    fail(error);
  }
}

/**
 * The open note as a command target. A standalone file is a note without a
 * vault: the core gives it the commands that are not tied to a vault or a path.
 */
export function currentTarget(): CommandTarget | null {
  if (!state.note) return null;
  if (state.note.file) return { kind: "note", file: state.note.id };
  return state.vault ? { kind: "note", name: state.note.id, vault: state.vault.id } : null;
}

/**
 * The last command run from any menu. The run button of the note toolbar
 * repeats it: usually one command is run again and again, for example a
 * build or a publish.
 */
const LAST_COMMAND = "kknow:command:last";

/** The last command, if the open note has it. */
export function lastCommand(): CommandView | null {
  const id = localStorage.getItem(LAST_COMMAND);
  return noteCommands.find((command) => command.id === id) ?? null;
}

/**
 * Updates the run button of the note toolbar: it names the last command.
 *
 * Call it when the commands of the open note or the last command change.
 */
function renderRunButton(): void {
  if (!state.note) return;
  const last = lastCommand();
  const title = last
    ? t("topbar.commands.run", { name: last.name })
    : t("topbar.commands.title");
  el.runLast.title = title;
  el.runLast.setAttribute("aria-label", title);
}

/** The commands of the open note; empty while they load. */
export function openNoteCommands(): CommandView[] {
  return noteCommands;
}
