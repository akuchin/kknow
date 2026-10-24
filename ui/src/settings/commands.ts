/** The "Commands" section: the command list and the form of one command. */

import { api, type CommandManifest, type CommandSpec, type TargetKind } from "../api";
import { describeError, t, type Key } from "../i18n";
import { iconMarkup } from "../icons";
import { chips, field, heading, row, select, toggle } from "./controls";
import { state, view } from "./state";

/** Target kinds in form order. */
const TARGETS: { kind: TargetKind; label: Key }[] = [
  { kind: "note", label: "settings.commands.target.note" },
  { kind: "folder", label: "settings.commands.target.folder" },
  { kind: "vault", label: "settings.commands.target.vault" },
];

/** Command manifest: the form edits all of it and writes the file on each change. */
let manifest: CommandManifest | null = null;
/**
 * The command open in the "Commands" section, by index, not by ID. The form
 * shows empty and duplicate IDs so the user can fix them, and the selection
 * must survive that.
 */
let pickedCommand = 0;

/**
 * Commands section: the list on the left, the selected command form on the
 * right.
 *
 * The manifest used to be edited in a text editor: the menu revealed the file
 * in the file manager. The form knows what a text editor does not: the
 * available runners, the possible targets, and the environment the command
 * gets. It also shows core problems next to the list, not after the first
 * failed run.
 *
 * The form edits the whole manifest and writes it on each change. There is no
 * "save" button, as in other settings. An invalid entry is saved like the
 * others: a command without a script is unfinished, not a reason to block the
 * window from closing.
 */
export async function renderCommands(): Promise<void> {
  const page = document.createElement("div");
  page.className = "settings-page";

  try {
    // The vault list is for the vaults field: a command can apply to some
    // vaults only, and picking them by name is easier.
    [manifest, state.vaults] = await Promise.all([api.editCommands(), api.listVaults()]);
  } catch (error) {
    page.append(heading(t("settings.section.commands")), problem(describeError(error)));
    view.body.replaceChildren(page);
    return;
  }

  // Broken JSON is the only case the form refuses to edit: it could not read
  // the file, and a save would erase what is there.
  if (manifest.broken) {
    page.append(
      heading(t("settings.section.commands")),
      problem(t("settings.commands.broken")),
      problem(manifest.broken),
      commandsFile(),
    );
    view.body.replaceChildren(page);
    return;
  }

  if (pickedCommand >= manifest.commands.length) pickedCommand = 0;

  const layout = document.createElement("div");
  layout.className = "settings-commands";

  const list = document.createElement("div");
  list.className = "settings-command-list";
  layout.append(list, page);
  view.body.replaceChildren(layout);

  fillCommandList(list);
  renderCommandForm(page);
}

/** Command list on the left: label, runner and whether it has problems. */
function fillCommandList(list: HTMLElement): void {
  const count = document.createElement("p");
  count.className = "settings-vault-count";
  count.textContent = t("settings.commands.count", { count: manifest!.commands.length });
  list.replaceChildren(count);

  manifest!.commands.forEach((command, index) => {
    const button = document.createElement("button");
    button.className = "settings-command";
    button.setAttribute("aria-current", String(index === pickedCommand));
    button.onclick = () => {
      pickedCommand = index;
      void renderCommands();
    };

    const text = document.createElement("span");
    text.className = "settings-vault-text";
    const name = document.createElement("span");
    name.textContent = command.name.trim() || t("settings.commands.untitled");
    const detail = document.createElement("span");
    detail.className = "settings-row-detail";
    detail.textContent = [command.kind, ...command.targets.map(targetLabel)].join(" · ");
    text.append(name, detail);
    button.append(text);

    // The core sends problems as "id: what is wrong" strings; the prefix finds
    // the command. The window has no own validation on purpose: it could only
    // differ from the core for the worse.
    if (manifest!.problems.some((entry) => entry.startsWith(`${command.id}:`))) {
      const dot = document.createElement("span");
      dot.className = "status-dot";
      dot.dataset.tone = "error";
      button.append(dot);
    }

    list.append(button);
  });

  const tools = document.createElement("div");
  tools.className = "settings-command-tools";

  const add = document.createElement("button");
  add.className = "ghost";
  add.innerHTML = `${iconMarkup("terminal")}<span>${t("settings.commands.add")}</span>`;
  add.onclick = () => void addCommand();

  const remove = document.createElement("button");
  remove.className = "ghost danger";
  remove.innerHTML = `${iconMarkup("close")}<span>${t("settings.commands.remove")}</span>`;
  remove.disabled = manifest!.commands.length === 0;
  remove.onclick = () => void removeCommand();

  tools.append(add, remove);
  list.append(tools);
}

/** The form of one command, with the file-wide settings below it. */
function renderCommandForm(page: HTMLElement): void {
  page.replaceChildren();

  const command = manifest!.commands[pickedCommand];
  if (!command) {
    page.append(heading(t("settings.section.commands")));
    const empty = document.createElement("p");
    empty.className = "settings-row-detail";
    empty.textContent = t("settings.commands.empty");
    page.append(empty, ...fileSection());
    return;
  }

  const name = document.createElement("input");
  name.className = "settings-name";
  name.value = command.name;
  name.placeholder = t("settings.commands.untitled");
  // On `change`, not on each keystroke: each save writes the file, and one
  // label would mean thirty writes.
  name.onchange = () => {
    command.name = name.value;
    void saveCommands();
  };
  page.append(field(t("settings.commands.name"), name, t("settings.commands.name.detail")));

  const id = document.createElement("input");
  id.className = "settings-mono";
  id.value = command.id;
  id.autocapitalize = "off";
  id.spellcheck = false;
  id.onchange = () => {
    command.id = id.value.trim();
    void saveCommands();
  };
  page.append(
    field(t("settings.commands.id"), id, t("settings.commands.id.detail", { id: command.id })),
  );

  // The runner in the file can be unknown to the core: a typo there is the
  // most common problem. Then it goes first in the list. An empty field would
  // hide exactly what the core complains about, and the user would fix it
  // blind.
  const kinds = manifest!.kinds.includes(command.kind)
    ? manifest!.kinds
    : [command.kind, ...manifest!.kinds];

  page.append(
    row(
      t("settings.commands.kind"),
      select(
        kinds.map((kind) => ({ value: kind, label: kind })),
        command.kind,
        (value) => {
          command.kind = value;
          void saveCommands();
        },
      ),
    ),
  );

  const program = document.createElement("input");
  program.className = "settings-mono";
  program.value = command.program ?? "";
  program.placeholder = t("settings.commands.program.placeholder");
  program.spellcheck = false;
  program.onchange = () => {
    // An empty field means "the runner default", not a program with an empty name.
    command.program = program.value.trim() || null;
    void saveCommands();
  };
  page.append(
    field(t("settings.commands.program"), program, t("settings.commands.program.detail")),
  );

  const script = document.createElement("textarea");
  script.className = "settings-script";
  script.rows = 6;
  script.value = command.script;
  script.spellcheck = false;
  script.onchange = () => {
    command.script = script.value;
    void saveCommands();
  };
  page.append(field(t("settings.commands.script"), script, t("settings.commands.script.detail")));

  page.append(commandVars(command));

  page.append(heading(t("settings.commands.where")));
  page.append(
    row(
      t("settings.commands.targets"),
      chips(
        TARGETS.map((entry) => ({ value: entry.kind, label: t(entry.label) })),
        command.targets,
        (value, on) => {
          const kind = value as TargetKind;
          command.targets = on
            ? TARGETS.map((entry) => entry.kind).filter(
                (entry) => entry === kind || command.targets.includes(entry),
              )
            : command.targets.filter((entry) => entry !== kind);
          void saveCommands(true);
        },
      ),
      t("settings.commands.targets.detail"),
    ),
  );

  page.append(
    row(
      t("settings.commands.vaults"),
      chips(
        state.vaults.map((vault) => ({ value: vault.id, label: vault.name })),
        command.vaults,
        (value, on) => {
          command.vaults = on
            ? [...command.vaults, value]
            : command.vaults.filter((entry) => entry !== value);
          void saveCommands(true);
        },
      ),
      t("settings.commands.vaults.detail"),
    ),
  );

  const patterns = document.createElement("textarea");
  patterns.className = "settings-script";
  patterns.rows = 2;
  patterns.value = command.match.join("\n");
  patterns.placeholder = t("settings.commands.match.placeholder");
  patterns.spellcheck = false;
  patterns.onchange = () => {
    command.match = lines(patterns.value);
    void saveCommands();
  };
  page.append(field(t("settings.commands.match"), patterns, t("settings.commands.match.detail")));

  const requires = document.createElement("input");
  requires.className = "settings-mono";
  requires.value = command.requires.join(", ");
  requires.placeholder = t("settings.commands.requires.placeholder");
  requires.spellcheck = false;
  requires.onchange = () => {
    command.requires = requires.value
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean);
    // Note fields reach the script as variables, so the list under the script
    // is rebuilt right here: this is where the user looks at it.
    void saveCommands(true);
  };
  page.append(
    field(t("settings.commands.requires"), requires, t("settings.commands.requires.detail")),
  );

  page.append(heading(t("settings.commands.how")));
  page.append(
    row(
      t("settings.commands.cwd"),
      select(
        [
          { value: "vault", label: t("settings.commands.cwd.vault") },
          { value: "target", label: t("settings.commands.cwd.target") },
        ],
        command.cwd,
        (value) => {
          command.cwd = value as CommandSpec["cwd"];
          void saveCommands();
        },
      ),
    ),
  );

  const timeout = document.createElement("input");
  timeout.type = "number";
  timeout.min = "1";
  timeout.value = String(command.timeoutSeconds);
  timeout.onchange = () => {
    command.timeoutSeconds = Math.max(1, Math.round(Number(timeout.value) || 0));
    timeout.value = String(command.timeoutSeconds);
    void saveCommands();
  };
  page.append(row(t("settings.commands.timeout"), timeout));

  page.append(
    row(
      t("settings.commands.confirm"),
      toggle(command.confirm, (value) => {
        command.confirm = value;
        void saveCommands(true);
      }),
    ),
  );

  page.append(
    row(
      t("settings.commands.login"),
      toggle(command.login, (value) => {
        command.login = value;
        void saveCommands(true);
      }),
      t("settings.commands.login.detail"),
    ),
  );

  page.append(
    field(
      t("settings.commands.env"),
      envEditor(command.env ?? {}, (env) => {
        command.env = env;
        void saveCommands();
      }),
      t("settings.commands.env.detail"),
    ),
  );

  page.append(...fileSection());
}

/** File-wide part: problems, path and variables shared by all commands. */
function fileSection(): HTMLElement[] {
  // Problems have their own block because they change most often. The core
  // recomputes them on each change, and a fixed command must stop showing
  // problems at once, not after a section switch.
  const problems = document.createElement("div");
  problems.className = "settings-problems";
  fillProblems(problems);

  const parts: HTMLElement[] = [heading(t("settings.commands.file")), problems];

  parts.push(
    field(
      t("settings.commands.env.shared"),
      envEditor(manifest!.env, (env) => {
        manifest!.env = env;
        void saveCommands();
      }),
      t("settings.commands.env.shared.detail"),
    ),
    commandsFile(),
  );

  return parts;
}

/** The file location row with a reveal button. */
function commandsFile(): HTMLElement {
  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = manifest!.path;
  // The path is cut with an ellipsis; the tooltip shows all of it.
  path.title = manifest!.path;

  const finder = document.createElement("button");
  finder.className = "ghost";
  finder.textContent = t("settings.vaults.finder");
  finder.onclick = () => void api.revealCommands().catch(console.error);

  const line = document.createElement("div");
  line.className = "settings-vault-head";
  const names = document.createElement("div");
  names.className = "settings-vault-names";
  const title = document.createElement("span");
  title.textContent = t("settings.commands.file.detail");
  title.className = "settings-row-detail";
  names.append(path, title);
  line.append(names, finder);
  return line;
}

/**
 * The environment the script gets.
 *
 * The list is not decoration. The target path comes as a variable, not as a
 * text substitution, and without the list it is not clear how a command
 * learns its target. The list depends on the command: `$KKNOW_NOTE` shows only
 * if the command targets notes, and the required fields turn into the exact
 * names to type in the script.
 */
function commandVars(command: CommandSpec): HTMLElement {
  const box = document.createElement("details");
  box.className = "settings-vars";

  const title = document.createElement("summary");
  title.textContent = t("settings.commands.vars");
  box.append(title);

  const list = document.createElement("dl");
  const put = (name: string, meaning: string): void => {
    const key = document.createElement("dt");
    key.textContent = `$${name}`;
    const value = document.createElement("dd");
    value.textContent = meaning;
    list.append(key, value);
  };

  put("KKNOW_PATH", t("settings.commands.vars.path"));
  put("KKNOW_TARGET", t("settings.commands.vars.target"));
  put("KKNOW_RELATIVE", t("settings.commands.vars.relative"));
  put("KKNOW_VAULT", t("settings.commands.vars.vault"));
  put("KKNOW_VAULT_ID", t("settings.commands.vars.vaultId"));
  put("KKNOW_VAULT_NAME", t("settings.commands.vars.vaultName"));
  if (command.targets.includes("note")) {
    put("KKNOW_NOTE", t("settings.commands.vars.note"));
  }

  for (const key of command.requires) {
    const name = variableName(key);
    if (name) put(name, t("settings.commands.vars.fm", { field: key }));
  }
  put("KKNOW_FM_…", t("settings.commands.vars.fm.any"));

  box.append(list);
  return box;
}

/**
 * Variable name for a note field: `confluence-id` → `KKNOW_FM_CONFLUENCE_ID`.
 *
 * It repeats `variable_name` from `crates/kknow-run/src/lib.rs`. The copy is
 * intentional and safe: it is a hint in the form, not a rule. The core does
 * the real work, so if the rules differ, only the hint is wrong, not the run.
 */
function variableName(key: string): string | null {
  let name = "";
  for (const symbol of key) {
    if (/[a-z0-9]/i.test(symbol)) name += symbol.toUpperCase();
    else if (!name.endsWith("_")) name += "_";
  }

  const trimmed = name.replace(/^_+|_+$/g, "");
  if (!trimmed || /^\d/.test(trimmed)) return null;
  return `KKNOW_FM_${trimmed}`;
}

/**
 * Name and value pairs.
 *
 * A row without a name is not a variable: an empty row lives in the form, not
 * in the manifest. So "add a variable" appends it straight to the markup, and
 * saves and rebuilds nothing. A rebuild would move the cursor out of the field
 * where the user is about to type the name.
 */
function envEditor(
  env: Record<string, string>,
  onChange: (env: Record<string, string>) => void,
): HTMLElement {
  const box = document.createElement("div");
  box.className = "settings-env";

  const collect = (): Record<string, string> => {
    const next: Record<string, string> = {};
    for (const line of box.querySelectorAll<HTMLElement>(".settings-env-row")) {
      const [name, value] = line.querySelectorAll("input");
      if (name.value.trim()) next[name.value.trim()] = value.value;
    }
    return next;
  };

  // The button goes in first, and rows are inserted before it. It marks the
  // end of the list, so it does not need to move down for each new variable.
  const add = document.createElement("button");
  add.className = "ghost";
  add.type = "button";
  add.textContent = t("settings.commands.env.add");
  box.append(add);

  const addRow = (key: string, value: string): HTMLInputElement => {
    const line = document.createElement("div");
    line.className = "settings-env-row";

    const name = document.createElement("input");
    name.className = "settings-mono";
    name.value = key;
    name.placeholder = t("settings.commands.env.name");
    name.spellcheck = false;

    const text = document.createElement("input");
    text.className = "settings-mono";
    text.value = value;
    text.placeholder = t("settings.commands.env.value");
    text.spellcheck = false;

    // An erased name removes the variable, so a row needs no remove button.
    name.onchange = () => onChange(collect());
    text.onchange = () => onChange(collect());

    line.append(name, text);
    box.insertBefore(line, add);
    return name;
  };

  for (const [key, value] of Object.entries(env)) addRow(key, value);

  add.onclick = () => addRow("", "").focus();
  if (Object.keys(env).length === 0) addRow("", "");

  return box;
}

/** Non-empty, trimmed lines of a text field. */
function lines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function targetLabel(kind: TargetKind): string {
  const found = TARGETS.find((entry) => entry.kind === kind);
  return found ? t(found.label) : kind;
}

function problem(text: string): HTMLElement {
  const note = document.createElement("p");
  note.className = "settings-problem";
  note.textContent = text;
  return note;
}

function fillProblems(box: HTMLElement): void {
  box.replaceChildren(...manifest!.problems.map(problem));
}

/**
 * Writes the manifest and updates what depends on it.
 *
 * `rebuild` rebuilds the form. Toggles and lists that change the form content
 * use it. Typed text does not: a rebuild would move the cursor out of the
 * field.
 */
async function saveCommands(rebuild = false): Promise<void> {
  if (!manifest) return;

  try {
    // Take only the problems from the answer, not the whole manifest. Form
    // fields hold references to entries of this manifest. A new manifest would
    // send the next edit to an entry that is no longer in the list.
    manifest.problems = (await api.saveCommands(manifest.commands, manifest.env)).problems;
  } catch (error) {
    console.error(error);
  }

  const list = view.body.querySelector<HTMLElement>(".settings-command-list");
  if (list) fillCommandList(list);

  const problems = view.body.querySelector<HTMLElement>(".settings-problems");
  if (problems) fillProblems(problems);

  const page = view.body.querySelector<HTMLElement>(".settings-commands .settings-page");
  if (page && rebuild) renderCommandForm(page);
}

async function addCommand(): Promise<void> {
  if (!manifest) return;

  manifest.commands.push({
    id: freshId(),
    name: t("settings.commands.new"),
    // The first runner is `shell`: it exists everywhere and needs nothing extra.
    kind: manifest.kinds[0] ?? "shell",
    script: "",
    targets: ["note", "folder", "vault"],
    vaults: [],
    match: [],
    requires: [],
    login: false,
    program: null,
    cwd: "vault",
    timeoutSeconds: 60,
    confirm: false,
    env: {},
  });

  pickedCommand = manifest.commands.length - 1;
  await saveCommands(true);
}

/** An unused ID. The terminal uses it to call the command, so it is ASCII. */
function freshId(): string {
  const used = new Set(manifest!.commands.map((command) => command.id));
  if (!used.has("command")) return "command";

  for (let number = 2; ; number += 1) {
    if (!used.has(`command-${number}`)) return `command-${number}`;
  }
}

async function removeCommand(): Promise<void> {
  const command = manifest?.commands[pickedCommand];
  if (!command) return;

  const name = command.name.trim() || command.id || t("settings.commands.untitled");
  if (!window.confirm(t("settings.commands.remove.confirm", { name }))) return;

  manifest!.commands.splice(pickedCommand, 1);
  if (pickedCommand >= manifest!.commands.length) {
    pickedCommand = Math.max(0, manifest!.commands.length - 1);
  }
  await saveCommands(true);
}
