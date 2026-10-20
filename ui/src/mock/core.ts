/**
 * Mock of the Tauri commands for UI work in a plain browser:
 * `KKNOW_MOCK=1 pnpm dev`. The app build does not include this file.
 */
import type {
  AppSettings,
  BookmarkView,
  CommandList,
  CommandManifest,
  CommandSpec,
  CommandView,
  FileEntry,
  GlobalResult,
  MarkdownAssociation,
  RunOutcome,
  NoteSummary,
  NoteView,
  SearchResult,
  SyncReport,
  SyncSettings,
  SyncSettingsForm,
  SyncStatus,
  TargetKind,
  ThemeSet,
  VaultStats,
  VaultInfo,
  VaultView,
} from "../api";

/** Source line mark, the same as the core puts into HTML. */
function line(number: number, block = false): string {
  return `<span class="line-mark${block ? " is-block" : ""}" data-line="${number}"></span>`;
}

interface MockNote {
  id: string;
  title: string;
  folder: string | null;
  tags: string[];
  content: string;
  html: string;
  /** Bookmark lines: the mock simply stores them on the note. */
  bookmarks?: number[];
}

const notes: MockNote[] = [
  {
    id: "Index",
    title: "Оглавление",
    folder: null,
    tags: ["мета"],
    content: "# Оглавление\n\nТочка входа. #мета\n\n- [[Rust]]\n- [[Призрак]]\n",
    html: `<h1>Оглавление</h1><p>Точка входа. <a href="kknow://tag/%D0%BC%D0%B5%D1%82%D0%B0">#мета</a></p><ul><li><a href="kknow://note/Rust">Rust</a></li><li><a href="kknow://missing/%D0%9F%D1%80%D0%B8%D0%B7%D1%80%D0%B0%D0%BA">Призрак</a></li></ul>`,
  },
  {
    id: "Rust",
    title: "Rust",
    folder: null,
    tags: ["rust", "учёба"],
    bookmarks: [5],
    content:
      "# Rust\n\nЯзык, на котором пишу [[Projects/kknow]]. #rust #учёба\n\n" +
      "Владение и заимствование<!-- @agent «заимствование»: добавить пример с &mut --> пока даются тяжело.\n\n" +
      "Времена жизни<!-- @agent «Времена жизни»: расписать на примере структуры, которая держит ссылку на строку, и объяснить, почему компилятор требует аннотацию, а в функции с одним аргументом — нет --> — отдельная тема.\n",
    html:
      // Line marks as in the core: the preview shows bookmarks with them.
      `<h1>${line(1)}Rust</h1><p>${line(3)}Язык, на котором пишу <a href="kknow://note/Projects/kknow">Projects/kknow</a>. <a href="kknow://tag/rust">#rust</a> <a href="kknow://tag/%D1%83%D1%87%D1%91%D0%B1%D0%B0">#учёба</a></p>` +
      `<p>${line(5)}Владение и <mark class="agent-note" title="добавить пример с &amp;mut">заимствование</mark> пока даются тяжело.</p>` +
      `<p>${line(7)}Времена жизни <span class="agent-note is-detached" title="расписать на примере структуры"></span> — отдельная тема.</p>`,
  },
  {
    id: "Projects/kknow",
    title: "kknow",
    folder: "Projects",
    tags: ["rust", "проекты/kknow"],
    content:
      "# kknow\n\nПроект на [[Rust]].\n\n## Планы\n\n- этап 3: поиск\n\n" +
      '```rust\n// точка входа\nfn main() {\n    let name = "kknow";\n    println!("{name}");\n}\n```\n',
    // Code block as the core returns it: pulldown-cmark sets `class="language-…"`.
    html:
      `<h1>kknow</h1><p>Проект на <a href="kknow://note/Rust">Rust</a>.</p><h2>Планы</h2><ul><li>этап 3: поиск</li></ul>` +
      '<pre><code class="language-rust">// точка входа\nfn main() {\n    let name = &quot;kknow&quot;;\n    println!(&quot;{name}&quot;);\n}\n</code></pre>',
  },
  {
    // Nested folder: it shows that the tree has more than one level.
    // The name is long on purpose: the tree cuts it with an ellipsis, not a wrap.
    id: "Projects/Архив/Идеи, до которых руки пока не дошли",
    title: "Идеи, до которых руки пока не дошли",
    folder: "Projects/Архив",
    tags: [],
    content: "# Идеи\n\nЧто не сделано, но жалко забыть.\n",
    html: "<h1>Идеи</h1><p>Что не сделано, но жалко забыть.</p>",
  },
  {
    id: "Работа/Встречи",
    title: "Встречи",
    folder: "Работа",
    tags: [],
    content: "# Встречи\n\nКто, когда и о чём.\n",
    html: "<h1>Встречи</h1><p>Кто, когда и о чём.</p>",
  },
  {
    // Table with markup in cells: it shows editing and the toolbar.
    id: "Ветки",
    title: "Ветки",
    folder: null,
    tags: [],
    content:
      "# Ветки\n\n| Ветка | Где живёт | Зачем |\n| --- | --- | :---: |\n" +
      "| `studio-prod` | Prod Authoring | редакторы работают здесь, источник **правды** |\n" +
      "| `prod` | git | выкладка; контент приезжает из `studio-prod` |\n" +
      "| `feature/*` | форк [[Rust]] | повседневная работа |\n",
    html:
      "<h1>Ветки</h1>\n<table><thead><tr><th>Ветка</th><th>Где живёт</th><th>Зачем</th></tr></thead>" +
      "<tbody><tr><td><code>studio-prod</code></td><td>Prod Authoring</td>" +
      "<td>редакторы работают здесь, источник <strong>правды</strong></td></tr></tbody></table>",
  },
  {
    // Wide table: a narrow ID column and long cells with unbreakable code
    // pieces. It shows whether the table fits the panel or spreads wide.
    id: "Ограничения",
    title: "Ограничения",
    folder: null,
    tags: [],
    content:
      "# Ограничения\n\n| ID | Ограничение | Источник |\n| --- | --- | --- |\n" +
      "| C-1 | Keycloak остаётся единственным источником правды об учётных записях. | ADR-001 |\n" +
      "| C-3 | Токен доступа должен оставаться коротким: только роли пользователя, " +
      "никогда через `full_scope_allowed`. | ADR-004 |\n" +
      "| C-5 | Rego пишем на синтаксисе v1 (`import rego.v1`) для OPA 1.x. | ADR-006 |\n" +
      "| C-6 | Роли площадки следуют схеме `{site}-{level}`, где уровень — " +
      "`reader` или `manager`. | ADR-006 |\n",
    html:
      "<h1>Ограничения</h1>\n<table><thead><tr><th>ID</th><th>Ограничение</th>" +
      "<th>Источник</th></tr></thead><tbody>" +
      "<tr><td>C-1</td><td>Keycloak остаётся единственным источником правды об " +
      "учётных записях.</td><td>ADR-001</td></tr>" +
      "<tr><td>C-3</td><td>Токен доступа должен оставаться коротким: только роли " +
      "пользователя, никогда через <code>full_scope_allowed</code>.</td><td>ADR-004</td></tr>" +
      "<tr><td>C-5</td><td>Rego пишем на синтаксисе v1 (<code>import rego.v1</code>) " +
      "для OPA 1.x.</td><td>ADR-006</td></tr>" +
      "<tr><td>C-6</td><td>Роли площадки следуют схеме <code>{site}-{level}</code>, " +
      "где уровень — <code>reader</code> или <code>manager</code>.</td><td>ADR-006</td></tr>" +
      "</tbody></table>",
  },
  {
    // Long note: on open the parser does not reach the end, and the widgets at
    // the end come from background parsing. It checks that the table and the
    // diagram at the end render without a click.
    id: "Длинная",
    title: "Длинная",
    folder: null,
    tags: [],
    content:
      "# Длинная\n\n" +
      Array.from({ length: 400 }, (_, i) => `Абзац ${i + 1}: **текст** про [[Rust]].\n`).join("\n") +
      "\n| Ветка | Зачем |\n| --- | --- |\n| `dev` | интеграция |\n" +
      "\n```mermaid\nflowchart LR\n  Хвост --> Разобран\n```\n",
    html: "<h1>Длинная</h1>",
  },
  {
    // Two diagrams, a valid one and one with a typo; the second shows the error display.
    id: "Схема",
    title: "Схема",
    folder: null,
    tags: [],
    content:
      "# Схема\n\n```mermaid\nflowchart LR\n  Заметка --> Разбор --> Граф\n```\n\n```mermaid\nflowchart LR\n  A -->\n```\n",
    html: `<h1>${line(1)}Схема</h1>\n${line(3, true)}<pre class="mermaid">flowchart LR\n  Заметка --&gt; Разбор --&gt; Граф\n</pre>\n${line(8, true)}<pre class="mermaid">flowchart LR\n  A --&gt;\n</pre>\n`,
  },
  {
    // Images: wide and tall ones show how CSS shrinks them; also one with a
    // caption size, one by file name and a missing one.
    id: "Картинки",
    title: "Картинки",
    folder: null,
    tags: [],
    content:
      "# Картинки\n\nШирокая:\n\n![панорама](img/wide.svg)\n\nВысокая, по имени файла:\n\n![[tall.svg]]\n\nС размером: ![[wide.svg|200]] и дальше текст.\n\nПропавшая: ![нет](img/missing.png)\n\nИз сети: ![](https://example.com/a.png)\n",
    html: `<h1>Картинки</h1>\n<p>Широкая:</p>\n<p><img class="note-image" data-src="img/wide.svg" alt="панорама"></p>\n<p>Высокая, по имени файла:</p>\n<p><img class="note-image" data-src="tall.svg" alt=""></p>\n<p>С размером: <img class="note-image" data-src="wide.svg" alt="" style="width:200px"> и дальше текст.</p>\n<p>Пропавшая: <img class="note-image" data-src="img/missing.png" alt="нет"></p>\n<p>Из сети: <img class="note-image" data-src="https://example.com/a.png" alt=""></p>\n`,
  },
];

/** Mock image: an SVG of the needed size with a caption. */
function mockImage(target: string): ArrayBuffer {
  if (target.includes("missing")) throw new Error(`картинка не найдена: ${target}`);
  const [width, height] = target.includes("tall") ? [600, 1800] : [2400, 900];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="100%" height="100%" fill="#7aa2f7"/><circle cx="50%" cy="50%" r="${height / 4}" fill="#e0af68"/><text x="24" y="80" font-size="64" fill="#1a1b26">${target}</text></svg>`;
  return new TextEncoder().encode(svg).buffer as ArrayBuffer;
}

const backlinks: Record<string, { id: string; title: string; label: string | null }[]> = {
  Rust: [{ id: "Index", title: "Оглавление", label: null }],
  "Projects/kknow": [{ id: "Rust", title: "Rust", label: null }],
};

/** A mock vault: the list entry plus its notes. */
interface MockVault {
  id: string;
  name: string;
  path: string;
  color: string;
  available: boolean;
  notes: MockNote[];
  /** Folders without notes: the note list cannot restore them, as in the core. */
  folders: string[];
}

/**
 * Three vaults: the main one, a second one with its own notes, and an
 * unavailable one.
 *
 * The unavailable vault is disabled in the menu and skipped by search; there
 * is no other place to see that behaviour.
 */
const vaults: MockVault[] = [
  {
    id: "notes",
    name: "notes (мок)",
    path: "~/notes (мок)",
    color: "blue",
    available: true,
    notes,
    folders: ["Черновики"],
  },
  {
    id: "work-asw",
    name: "Work — ASW",
    path: "~/work (мок)",
    color: "violet",
    available: true,
    notes: [
      {
        id: "Смета",
        title: "Смета",
        folder: null,
        tags: ["работа"],
        content: "# Смета\n\nРабочие расходы и компиляторы. #работа\n",
        html: "<h1>Смета</h1><p>Рабочие расходы и компиляторы.</p>",
      },
      {
        id: "Встречи/Ретро",
        title: "Ретро",
        folder: "Встречи",
        tags: [],
        content: "# Ретро\n\nЧто починить в процессе. Про Rust тоже.\n",
        html: "<h1>Ретро</h1><p>Что починить в процессе.</p>",
      },
    ],
    folders: [],
  },
  {
    id: "arhiv-2024",
    name: "Архив 2024",
    path: "/Volumes/Backup/Архив 2024",
    color: "amber",
    available: false,
    notes: [],
    folders: [],
  },
];

let activeVault = vaults[0];

function vaultView(vault: MockVault): VaultView {
  return {
    id: vault.id,
    name: vault.name,
    path: vault.path,
    color: vault.color,
    active: vault.id === activeVault.id,
    available: vault.available,
  };
}

function vaultInfo(vault: MockVault): VaultInfo {
  return {
    id: vault.id,
    name: vault.name,
    root: vault.path,
    color: vault.color,
    noteCount: vault.notes.length,
  };
}

/** Sync settings live until the page reloads. */
let remote: SyncSettings | null = null;

/** Themes that "Make shared" has already copied to the shared folder. */
const sharedThemes = new Set<string>();

/**
 * App settings.
 *
 * In the app they live in `settings.json`, shared by both windows. In the
 * browser they only need to survive page changes, so `sessionStorage` is enough.
 */
let appSettings: AppSettings = readSettings();

function readSettings(): AppSettings {
  const fallback: AppSettings = {
    locale: localStorage.getItem("kknow:locale") ?? "ru",
    theme: "light",
    viewMode: "split",
    autosaveMs: 600,
    openLastNote: true,
  };

  try {
    const saved: unknown = JSON.parse(sessionStorage.getItem("kknow:mock:settings") ?? "null");
    return saved ? { ...fallback, ...(saved as AppSettings) } : fallback;
  } catch {
    return fallback;
  }
}

/**
 * Sync states per vault.
 *
 * The archive vault starts with conflicts: the status bar chip shows problems
 * in other vaults, and there is no other place to see it.
 */
const statuses = new Map<string, SyncStatus>([
  [
    "arhiv-2024",
    {
      vault: "arhiv-2024",
      state: "conflict",
      done: 0,
      total: 0,
      at: Date.now() - 42 * 60000,
      message: null,
      conflicts: 2,
      location: "/Volumes/Backup/Архив 2024",
      autoSyncMinutes: 15,
    },
  ],
]);

function statusOf(vault: string): SyncStatus {
  return (
    statuses.get(vault) ?? {
      vault,
      state: "off",
      done: 0,
      total: 0,
      at: null,
      message: null,
      conflicts: 0,
      location: null,
      autoSyncMinutes: 0,
    }
  );
}

function view(note: MockNote): NoteView {
  return {
    id: note.id,
    title: note.title,
    content: note.content,
    html: note.html,
    backlinks: backlinks[note.id] ?? [],
    outgoing: [],
    tags: note.tags,
    bookmarks: note.bookmarks ?? [],
  };
}

/** Lines that exist in the text, sorted and without duplicates, as in the core. */
function bookmarkLines(lines: number[], content: string): number[] {
  const count = content.split("\n").length - (content.endsWith("\n") ? 1 : 0);
  return [...new Set(lines)].filter((line) => line >= 1 && line <= count).sort((a, b) => a - b);
}

function find(id: string): MockNote {
  const note = activeVault.notes.find((candidate) => candidate.id === id);
  if (!note) throw new Error(`заметка не найдена: ${id}`);
  return note;
}

/** Hits in one vault, with the same query parsing as single-vault search. */
function hits(vault: MockVault, query: string): { note: MockNote; text: string }[] {
  const { field, text } = parseQuery(query);
  if (!text) return [];

  return vault.notes
    .filter((note) => {
      if (field === "title") return note.title.toLowerCase().includes(text);
      if (field === "tag") return note.tags.some((tag) => tag.toLowerCase().includes(text));
      return (
        note.content.toLowerCase().includes(text) ||
        note.title.toLowerCase().includes(text) ||
        note.tags.some((tag) => tag.toLowerCase().includes(text))
      );
    })
    .map((note) => ({ note, text }));
}

/** Real search returns plain text with highlighted matches. */
function snippetOf(note: MockNote, text: string): string {
  return note.content
    .replace(/[#*`\[\]]/g, "")
    .trim()
    .slice(0, 120)
    .replace(new RegExp(`(${escapeRegExp(text)})`, "gi"), "<b>$1</b>");
}

/**
 * Parses only as much of the query as the palette uses: `tag:(word)`,
 * `title:word` and plain text. tantivy does the real parsing; here the
 * palette chips only need to have some effect.
 */
function parseQuery(query: string): { field: string | null; text: string } {
  const match = /^(title|tag|body|path):\((.*)\)$|^(title|tag|body|path):(\S+)$/.exec(query.trim());
  if (!match) {
    // An unclosed parenthesis is a broken query; the core behaves the same.
    if (/^\w+:\(/.test(query.trim())) throw new Error("не удалось разобрать запрос");
    return { field: null, text: query.trim().toLowerCase() };
  }

  return {
    field: match[1] ?? match[3],
    text: (match[2] ?? match[4]).toLowerCase(),
  };
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Commands attached to targets.
 *
 * The set is varied on purpose: a successful, a failed and a timed-out
 * command. You can see all three panel states without real scripts.
 */
const MOCK_COMMANDS: (CommandSpec & {
  /** What the "command" prints: the browser has no real processes. */
  output: { code: number | null; stdout: string; stderr: string };
})[] = [
  {
    ...blank("confluence", "Отправить в Confluence"),
    script: 'wiki-publish replace $KKNOW_FM_CONFLUENCE "$KKNOW_PATH"',
    targets: ["note"],
    requires: ["confluence"],
    output: {
      code: 0,
      stdout: "загружаю {path}\nстраница обновлена: https://wiki.example/pages/12345\n",
      stderr: "",
    },
  },
  {
    ...blank("lint", "Проверить ссылки"),
    kind: "python",
    script: "import os\nprint(os.environ['KKNOW_PATH'])",
    match: ["Docs/**"],
    output: {
      code: 2,
      stdout: "проверено: {target}\n",
      stderr: "две ссылки ведут в никуда:\n  [[Призрак]]\n  [[Ещё не написано]]\n",
    },
  },
  {
    ...blank("backup", "Сложить архив"),
    kind: "fish",
    script: "tar czf ~/backup.tgz $KKNOW_VAULT",
    targets: ["folder", "vault"],
    confirm: true,
    timeoutSeconds: 300,
    output: { code: null, stdout: "собираю архив…\n", stderr: "" },
  },
  {
    // Broken on purpose: the settings form shows its problem, and the target
    // menu shows that an invalid command does not appear there.
    ...blank("сломанная", "Опечатка в виде"),
    kind: "phyton",
    script: "echo",
    output: { code: 0, stdout: "", stderr: "" },
  },
];

/** A command with all fields the form expects; callers override the rest. */
function blank(id: string, name: string): CommandSpec {
  return {
    id,
    name,
    kind: "shell",
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
  };
}

/** Runners the core supports, the same list as in `kknow-run`. */
const MOCK_KINDS = ["shell", "bash", "fish", "zsh", "python"];

/** Environment shared by all commands in the file. */
let sharedEnv: Record<string, string> = { CONFLUENCE_SERVER: "https://wiki.example" };

/**
 * Entry problems, with the same rules as in the core.
 *
 * They are copied here in full on purpose. The mock exists to show how the
 * form displays an invalid command, so a problem must appear and disappear
 * with edits in the form, not be a static string.
 */
function commandProblems(): string[] {
  const problems: string[] = [];

  for (const command of MOCK_COMMANDS) {
    const issues: string[] = [];
    if (!command.name.trim()) issues.push("нет подписи (name)");
    if (!command.script.trim()) issues.push("пустой скрипт");
    if (!MOCK_KINDS.includes(command.kind)) {
      issues.push(`неизвестный вид «${command.kind}»; известны: ${MOCK_KINDS.join(", ")}`);
    }
    if (command.targets.length === 0) {
      issues.push("пустой targets: команде не на чем появиться");
    }

    if (!command.id.trim()) problems.push("команда без id пропущена");
    else if (issues.length > 0) problems.push(`${command.id}: ${issues.join("; ")}`);
  }

  return problems;
}

/** Commands that can run; invalid ones do not reach the menu. */
function healthyCommands(): typeof MOCK_COMMANDS {
  const broken = new Set(commandProblems().map((problem) => problem.split(":")[0]));
  return MOCK_COMMANDS.filter((command) => !broken.has(command.id));
}

function manifestView(): CommandManifest {
  return {
    path: "~/Library/Application Support/kknow/commands.json",
    commands: MOCK_COMMANDS.map(({ output: _output, ...spec }) => spec),
    env: sharedEnv,
    problems: commandProblems(),
    kinds: MOCK_KINDS,
    broken: null,
  };
}

/**
 * Standalone files: path → text. One is already open, so the list shows at
 * once. In the mock, "Open file…" asks for a path and creates a file there.
 */
const looseFiles = new Map<string, string>([
  [
    "/Users/me/src/kknow/README.md",
    "# kknow\n\nЗаметочник на Rust. См. [[Rust]] — в отдельном файле ссылка оборвана.\n",
  ],
]);

let association: MarkdownAssociation = {
  supported: true,
  current: "com.microsoft.VSCode",
  ours: false,
};

function fileEntry(path: string): FileEntry {
  const slash = path.lastIndexOf("/");
  return { path, name: path.slice(slash + 1), folder: path.slice(0, Math.max(slash, 0)) };
}

function fileView(path: string): NoteView {
  const content = looseFiles.get(path);
  if (content === undefined) throw new Error(`файла больше нет, он убран из списка: ${path}`);
  const heading = /^#\s+(.+)$/m.exec(content)?.[1];
  return {
    id: path,
    title: heading ?? fileEntry(path).name.replace(/\.[^.]+$/, ""),
    content,
    html: `<pre>${content.replace(/&/g, "&amp;").replace(/</g, "&lt;")}</pre>`,
    backlinks: [],
    outgoing: [],
    tags: [],
    file: true,
  };
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const id = args?.id as string;

  switch (command) {
    case "current_vault":
    case "reload_vault":
      return vaultInfo(activeVault) as T;

    case "list_vaults":
      return vaults.map(vaultView) as T;

    case "get_settings":
      return appSettings as T;

    case "set_settings": {
      appSettings = args?.settings as AppSettings;
      sessionStorage.setItem("kknow:mock:settings", JSON.stringify(appSettings));
      return appSettings as T;
    }

    case "open_settings": {
      // The browser has no second window, so open the settings page in a new
      // tab to still show it.
      const section = (args?.section as string) ?? "general";
      window.open(`/settings.html#${section}`, "kknow-settings");
      return undefined as T;
    }

    case "rename_vault": {
      const vault = vaults.find((entry) => entry.id === args?.id);
      if (vault) vault.name = (args?.name as string).trim() || vault.name;
      return vaults.map(vaultView) as T;
    }

    case "set_vault_color": {
      const vault = vaults.find((entry) => entry.id === args?.id);
      if (vault) vault.color = args?.color as string;
      return vaults.map(vaultView) as T;
    }

    case "vault_stats": {
      const vault = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const folders = new Set(
        vault.notes.flatMap((note) =>
          note.folder ? note.folder.split("/").map((_, index, all) => all.slice(0, index + 1).join("/")) : [],
        ),
      );
      return {
        notes: vault.notes.length,
        folders: folders.size,
        bytes: vault.notes.reduce((sum, note) => sum + note.content.length, 0),
      } satisfies VaultStats as T;
    }

    case "add_vault": {
      // The mock does not read folders, so an added vault is empty.
      const path = args?.path as string;
      const added: MockVault = {
        id: `vault-${vaults.length + 1}`,
        name: path.split("/").filter(Boolean).pop() ?? path,
        path,
        color: "green",
        available: true,
        notes: [],
        folders: [],
      };
      vaults.push(added);
      activeVault = added;
      return vaultInfo(added) as T;
    }

    case "remove_vault": {
      const index = vaults.findIndex((vault) => vault.id === args?.id);
      if (index >= 0) vaults.splice(index, 1);
      if (!vaults.includes(activeVault)) activeVault = vaults[0];
      return vaults.map(vaultView) as T;
    }

    case "activate_vault": {
      const found = vaults.find((vault) => vault.id === args?.id);
      if (!found) throw new Error(`хранилище не найдено: ${String(args?.id)}`);
      activeVault = found;
      return vaultInfo(found) as T;
    }

    case "vault_by_hotkey": {
      const number = args?.number as number;
      return (vaults[number - 1]?.id ?? null) as T;
    }

    case "list_notes": {
      // The sidebar tree shows all vaults at once, so notes are also requested
      // for inactive vaults, by name.
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      return asked.notes.map<NoteSummary>((note) => ({
        id: note.id,
        title: note.title,
        folder: note.folder,
        tags: note.tags,
      })) as T;
    }

    case "load_note":
      return view(find(id)) as T;

    case "save_note": {
      const note = find(id);
      note.content = args?.content as string;
      const lines = args?.bookmarks as number[] | null;
      if (lines) note.bookmarks = bookmarkLines(lines, note.content);
      return { status: "saved", note: view(note) } as T;
    }

    case "set_bookmarks": {
      const note = find(id);
      note.bookmarks = bookmarkLines(args?.lines as number[], args?.content as string);
      return note.bookmarks as T;
    }

    case "list_bookmarks":
      return activeVault.notes.flatMap((note) =>
        (note.bookmarks ?? []).map<BookmarkView>((line) => ({
          note: note.id,
          title: note.title,
          folder: note.folder,
          line,
          text: note.content.split("\n")[line - 1] ?? "",
        })),
      ) as T;

    case "preview":
      return find(id).html as T;

    case "note_image":
    case "file_image":
      return mockImage(args?.target as string) as T;

    case "create_note": {
      const name = args?.name as string;
      const note: MockNote = {
        id: name,
        title: name.split("/").pop() ?? name,
        folder: name.includes("/") ? name.slice(0, name.lastIndexOf("/")) : null,
        tags: [],
        content: `# ${name}\n\n`,
        html: `<h1>${name}</h1>`,
      };
      activeVault.notes.push(note);
      return view(note) as T;
    }

    case "search":
      return hits(activeVault, (args?.query as string) ?? "").map<SearchResult>(
        ({ note, text }) => ({
          id: note.id,
          title: note.title,
          folder: note.folder,
          snippet: snippetOf(note, text),
        }),
      ) as T;

    case "search_all": {
      // Same order as in the core: the active vault first, unavailable ones
      // skipped.
      const order = [activeVault, ...vaults.filter((vault) => vault !== activeVault)];
      return order
        .filter((vault) => vault.available)
        .flatMap((vault) =>
          hits(vault, (args?.query as string) ?? "").map<GlobalResult>(({ note, text }) => ({
            vault: vault.id,
            vaultName: vault.name,
            color: vault.color,
            id: note.id,
            title: note.title,
            folder: note.folder,
            snippet: snippetOf(note, text),
          })),
        ) as T;
    }

    case "reveal_note":
      // The browser has no file manager; the mock only shows that the call arrived.
      console.info("reveal_note", id ?? "(каталог)");
      return undefined as T;

    case "delete_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const index = asked.notes.findIndex((note) => note.id === id);
      if (index < 0) throw new Error(`заметка не найдена: ${id}`);
      asked.notes.splice(index, 1);
      return undefined as T;
    }

    case "open_note":
    case "reveal_folder":
      // The browser has no file manager or default apps; a log line is enough.
      console.info(command, args?.name ?? id);
      return undefined as T;

    case "rename_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const to = (args?.to as string).replace(/^\/+|\/+$/g, "");
      const note = asked.notes.find((note) => note.id === id);
      if (!note) throw new Error(`заметка не найдена: ${id}`);
      if (asked.notes.some((other) => other !== note && other.id === to)) {
        throw new Error(`заметка уже существует: ${to}`);
      }
      note.id = to;
      note.folder = to.includes("/") ? to.slice(0, to.lastIndexOf("/")) : null;
      return to as T;
    }

    case "duplicate_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const note = asked.notes.find((note) => note.id === id);
      if (!note) throw new Error(`заметка не найдена: ${id}`);
      // As in the core: `copy`, then `copy 2`, `copy 3`, and so on; the first free name.
      let copy = `${note.id} copy`;
      for (let n = 2; asked.notes.some((other) => other.id === copy); n += 1) {
        copy = `${note.id} copy ${n}`;
      }
      asked.notes.push({ ...note, id: copy, tags: [...note.tags] });
      return copy as T;
    }

    case "rename_folder": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const name = args?.name as string;
      const to = (args?.to as string).replace(/^\/+|\/+$/g, "");
      const prefix = `${name}/`;
      const moved = (path: string) => `${to}/${path.slice(prefix.length)}`;
      for (const note of asked.notes) {
        if (!note.id.startsWith(prefix)) continue;
        note.id = moved(note.id);
        note.folder = note.id.slice(0, note.id.lastIndexOf("/"));
      }
      asked.folders = asked.folders.map((folder) =>
        folder === name ? to : folder.startsWith(prefix) ? moved(folder) : folder,
      );
      return undefined as T;
    }

    case "list_folders": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      // As in the core: note folders (with all ancestors) and empty folders.
      const all = new Set<string>(asked.folders);
      for (const note of asked.notes) {
        let path = "";
        for (const part of note.folder?.split("/") ?? []) {
          path = path ? `${path}/${part}` : part;
          all.add(path);
        }
      }
      return [...all] as T;
    }

    case "create_folder": {
      const name = (args?.name as string).replace(/^\/+|\/+$/g, "");
      if (!activeVault.folders.includes(name)) activeVault.folders.push(name);
      return undefined as T;
    }

    case "delete_folder": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const name = args?.name as string;
      const prefix = `${name}/`;
      asked.notes = asked.notes.filter((note) => !note.id.startsWith(prefix));
      asked.folders = asked.folders.filter(
        (folder) => folder !== name && !folder.startsWith(prefix),
      );
      return undefined as T;
    }

    case "list_themes":
      return {
        themes: [
          {
            id: "sepia",
            name: "Сепия",
            author: "kknow",
            base: "light",
            source: "vault",
            shadowsGlobal: sharedThemes.has("sepia"),
            variables: {
              bg: "#f6efe0",
              "bg-soft": "#efe6d3",
              "bg-raised": "#fbf6ea",
              border: "#ddd0b4",
              text: "#3f3527",
              "text-muted": "#8a7c62",
              accent: "#9c5b28",
              "accent-soft": "#f0e0c9",
            },
            warnings: [],
          },
          {
            id: "gruvbox-light",
            name: "Gruvbox светлая",
            author: "kknow",
            base: "light",
            source: "global",
            shadowsGlobal: false,
            variables: {
              bg: "#fbf1c7",
              "bg-soft": "#f2e5bc",
              "bg-raised": "#f9f5d7",
              border: "#d5c4a1",
              text: "#3c3836",
              "text-muted": "#7c6f64",
              heading: "#b57614",
              accent: "#af3a03",
              "accent-soft": "#f2ddb0",
              danger: "#9d0006",
              selection: "#d5c4a1",
              "code-keyword": "#9d0006",
              "code-string": "#79740e",
              "code-comment": "#928374",
              "code-number": "#8f3f71",
              "code-type": "#b57614",
              "code-function": "#427b58",
            },
            warnings: [],
          },
        ],
        problems: [],
      } satisfies ThemeSet as T;

    case "list_commands": {
      const kind = args?.kind as TargetKind;
      return {
        commands: healthyCommands()
          .filter((command) => command.targets.includes(kind))
          .map(({ id, name, kind: runner, confirm }) => ({
            id,
            name,
            kind: runner,
            confirm,
          })) satisfies CommandView[],
        problems: commandProblems(),
      } satisfies CommandList as T;
    }

    case "run_command": {
      const command = MOCK_COMMANDS.find((entry) => entry.id === args?.command);
      if (!command) throw new Error(`команда не найдена: ${String(args?.command)}`);

      // The browser has no real processes: the mock returns prepared output.
      // Its job is to show the panel.
      return {
        command: command.id,
        code: command.output.code,
        stdout: command.output.stdout
          .replace("{path}", `/Users/kknow/${(args?.target as string) ?? "(каталог)"}`)
          .replace("{target}", (args?.kind as string) ?? "vault"),
        stderr: command.output.stderr,
        timedOut: command.output.code === null,
        truncated: false,
        millis: 1240,
      } satisfies RunOutcome as T;
    }

    case "edit_commands":
      return manifestView() as T;

    case "save_commands": {
      // There is no file, so a "write" replaces the set in memory. Prepared
      // output stays with the command of the same id: otherwise a label edit
      // would remove its panel report.
      const outputs = new Map(MOCK_COMMANDS.map((command) => [command.id, command.output]));
      const incoming = (args?.commands as CommandSpec[]).map((spec) => ({
        ...spec,
        output: outputs.get(spec.id) ?? { code: 0, stdout: "готово\n", stderr: "" },
      }));

      MOCK_COMMANDS.splice(0, MOCK_COMMANDS.length, ...incoming);
      sharedEnv = args?.env as Record<string, string>;
      return manifestView() as T;
    }

    case "share_theme":
      sharedThemes.add(args?.id as string);
      return undefined as T;

    case "reveal_themes":
      console.info("reveal_themes: в браузере файлового менеджера нет");
      return undefined as T;

    case "reveal_commands":
      console.info("reveal_commands: в браузере файлового менеджера нет");
      return undefined as T;

    case "sync_status":
      return statusOf(activeVault.id) as T;

    case "sync_statuses":
      return [...vaults.map((vault) => statusOf(vault.id))] as T;

    case "sync_settings":
      return remote as T;

    case "save_sync_settings": {
      const settings = args?.settings as SyncSettingsForm;
      const provider = settings.provider;

      if (provider.kind === "s3" && !provider.bucket.trim()) {
        throw new Error("не задан бакет");
      }
      if (provider.kind === "folder" && !provider.path.trim()) {
        throw new Error("не задана папка");
      }

      const location =
        provider.kind === "folder"
          ? provider.path
          : `${provider.endpoint ?? "s3:/"}/${provider.bucket}/${provider.prefix ?? ""}`;

      statuses.set(activeVault.id, {
        ...statusOf(activeVault.id),
        state: "idle",
        location,
        autoSyncMinutes: settings.autoSyncMinutes,
      });

      remote = {
        ...settings,
        location,
        // A folder never has secrets; the core answers the same way.
        needsSecret: provider.kind === "s3",
        hasSecret:
          provider.kind === "s3" && (Boolean(args?.secret) || (remote?.hasSecret ?? false)),
      };
      return remote as T;
    }

    case "forget_sync_settings":
      remote = null;
      statuses.delete(activeVault.id);
      return undefined as T;

    case "sync_now": {
      if (!remote) throw new Error("синхронизация не настроена");
      statuses.set(activeVault.id, {
        ...statusOf(activeVault.id),
        state: "conflict",
        done: 2,
        total: 2,
        at: Date.now(),
        conflicts: 1,
      });
      // The mock always reports a conflict: there is no other place to see it.
      return {
        uploaded: 1,
        downloaded: 1,
        removedLocal: 0,
        removedRemote: 0,
        conflicts: [{ id: "Index", copy: "Index (конфликт 3f9a1c2b)" }],
      } satisfies SyncReport as T;
    }

    case "list_files":
      return [...looseFiles.keys()].map(fileEntry) as T;

    case "open_file": {
      const path = args?.path as string;
      if (!/\.(md|markdown|mdown)$/i.test(path)) throw new Error(`это не markdown-файл: ${path}`);
      if (!looseFiles.has(path)) looseFiles.set(path, `# ${fileEntry(path).name}\n\nОткрыт в заглушке.\n`);
      return { kind: "file", path } as T;
    }

    case "take_opened_file":
      return null as T;

    case "load_file":
      return fileView(args?.path as string) as T;

    case "save_file": {
      const path = args?.path as string;
      looseFiles.set(path, args?.content as string);
      return { status: "saved", note: fileView(path) } as T;
    }

    case "preview_file": {
      const path = args?.path as string;
      return fileView(path).html as T;
    }

    case "forget_file":
      looseFiles.delete(args?.path as string);
      return [...looseFiles.keys()].map(fileEntry) as T;

    case "reveal_file":
      return undefined as T;

    case "markdown_association":
      return association as T;

    case "claim_markdown":
      association = { supported: true, current: "dev.kknow.desktop", ours: true };
      return association as T;

    default:
      throw new Error(`мок не знает команду ${command}`);
  }
}
