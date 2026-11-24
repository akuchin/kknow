import { api, type CommandManifest, type CommandSpec, type TargetKind } from "../api";
import { commandName, commandProblemText, problemCommand } from "../core-text";
import { describeError, t, type Key } from "../i18n";
import { iconMarkup } from "../icons";
import { chips, field, heading, row, select, toggle } from "./controls";
import { state, view } from "./state";

const TARGETS: { kind: TargetKind; label: Key }[] = [
  { kind: "note", label: "settings.commands.target.note" },
  { kind: "folder", label: "settings.commands.target.folder" },
  { kind: "vault", label: "settings.commands.target.vault" },
];

let manifest: CommandManifest | null = null;
let pickedCommand = 0;

export async function renderCommands(): Promise<void> {
  const page = document.createElement("div");
  page.className = "settings-page";

  try {
    [manifest, state.vaults] = await Promise.all([api.editCommands(), api.listVaults()]);
  } catch (error) {
    page.append(heading(t("settings.section.commands")), problem(describeError(error)));
    view.body.replaceChildren(page);
    return;
  }

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
    name.textContent = commandName(command).trim() || t("settings.commands.untitled");
    const detail = document.createElement("span");
    detail.className = "settings-row-detail";
    detail.textContent = [command.kind, ...command.targets.map(targetLabel)].join(" · ");
    text.append(name, detail);
    button.append(text);

    if (manifest!.problems.some((entry) => problemCommand(entry) === command.id)) {
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

function fileSection(): HTMLElement[] {
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

function commandsFile(): HTMLElement {
  const path = document.createElement("span");
  path.className = "settings-path";
  path.textContent = manifest!.path;
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
  box.replaceChildren(...manifest!.problems.map((entry) => problem(commandProblemText(entry))));
}

async function saveCommands(rebuild = false): Promise<void> {
  if (!manifest) return;

  try {
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

  const name = commandName(command).trim() || command.id || t("settings.commands.untitled");
  if (!window.confirm(t("settings.commands.remove.confirm", { name }))) return;

  manifest!.commands.splice(pickedCommand, 1);
  if (pickedCommand >= manifest!.commands.length) {
    pickedCommand = Math.max(0, manifest!.commands.length - 1);
  }
  await saveCommands(true);
}
