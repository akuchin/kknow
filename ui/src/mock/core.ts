/**
 * Mock of the Tauri commands for UI work in a plain browser:
 * `KKNOW_MOCK=1 pnpm dev`. The app build does not include this file.
 */
import type {
  AppSettings,
  CommandProblem,
  BookmarkMark,
  BookmarkView,
  CommandList,
  CommandManifest,
  CommandSpec,
  CommandView,
  FileEntry,
  GitAuth,
  GitStatus,
  KeymapView,
  GlobalResult,
  MarkdownAssociation,
  McpStatus,
  RunLine,
  RunOutcome,
  NoteSummary,
  NoteView,
  SearchResult,
  SyncReport,
  EncryptionOn,
  SyncSettings,
  SyncSettingsForm,
  SyncStatus,
  TargetKind,
  TaskView,
  LinkTargetView,
  BacklinkLineView,
  MentionView,
  LinkPreview,
  LinkRef,
  ThemeSet,
  TrashEntryView,
  UpdateStatus,
  VaultStats,
  VaultInfo,
  VaultView,
  ProposalView,
  Review,
  Thread,
} from "../api";
import { setTaskBox, taskBox } from "../tasks";
import type { ErrorView } from "../error-view";
import { emit, listen } from "./event";

/** An error in the same shape as the real commands send it. */
function coded(code: string, message: string, params: Record<string, string> = {}): ErrorView {
  return { code, params, message };
}

/** Source line mark, the same as the core puts into HTML. */
function line(number: number, block = false): string {
  return `<span class="line-mark${block ? " is-block" : ""}" data-line="${number}"></span>`;
}

/** A task box, the same as the core renders it. */
function box(done: boolean): string {
  return `<input disabled="" type="checkbox"${done ? ' checked=""' : ""}/>\n`;
}

interface MockNote {
  id: string;
  title: string;
  folder: string | null;
  tags: string[];
  content: string;
  html: string;
  /** Bookmarks: the mock simply stores them on the note. */
  bookmarks?: BookmarkMark[];
  /** A locked note. The mock keeps the text plain and only hides it. */
  locked?: boolean;
}

/**
 * Agent threads and proposals of the mock vault. The core keeps them in
 * `.kknow/threads` and `.kknow/proposals`; the mock keeps them here, with
 * one question of the agent and one proposal, so the window shows both.
 */
const threads: Thread[] = [
  {
    id: "k3f",
    note: "Rust",
    anchor: "borrowing",
    task: "add an example with &mut",
    status: "question",
    messages: [
      { from: "me", text: "add an example with &mut", at: 1_730_000_000 },
      {
        from: "claude",
        text: "An example with a function that changes a vector, or with a struct that holds a reference?",
        at: 1_730_000_600,
      },
    ],
    created: 1_730_000_000,
    updated: 1_730_000_600,
  },
];

const proposals: ProposalView[] = [
  {
    id: "p7f3a",
    note: "Rust",
    reason: "Made the wording clearer",
    author: "claude",
    create: false,
    created: 1_730_001_000,
    updated: 1_730_001_000,
    status: "pending",
    stale: [false, false],
    edits: [
      { old: "are still hard for me", new: "are still hard for me: the compiler complains more often than I would like", state: "pending" },
      { old: "are a separate topic", new: "are a separate big topic", state: "pending" },
    ],
  },
];

/** The proposal again with its status and stale marks, as the core computes them. */
function recount(proposal: ProposalView, content: string): ProposalView {
  const pending = proposal.edits.filter((edit) => edit.state === "pending").length;
  const accepted = proposal.edits.filter((edit) => edit.state === "accepted").length;
  const status =
    pending > 0 ? "pending" : accepted === proposal.edits.length ? "accepted" : accepted === 0 ? "rejected" : "partial";
  const stale = proposal.edits.map(
    (edit) => edit.state === "pending" && content.split(edit.old).length !== 2,
  );
  return { ...proposal, status, stale };
}

const notes: MockNote[] = [
  {
    id: "Index",
    title: "Contents",
    folder: null,
    tags: ["meta"],
    content: "# Contents\n\nThe entry point. #meta\n\n- [[Rust]]\n- [[Ghost]]\n",
    html: `<h1>Contents</h1><p>The entry point. <a href="kknow://tag/meta">#meta</a></p><ul><li><a href="kknow://note/Rust">Rust</a></li><li><a href="kknow://missing/Ghost">Ghost</a></li></ul>`,
  },
  {
    id: "Rust",
    title: "Rust",
    folder: null,
    tags: ["rust", "study"],
    bookmarks: [{ line: 3 }, { line: 5, name: "ownership" }],
    content:
      "# Rust\n\nThe language I write [[Projects/kknow]] in. #rust #study\n\n" +
      "Ownership and borrowing<!-- @agent#k3f «borrowing»: add an example with &mut --> are still hard for me.\n\n" +
      "Lifetimes<!-- @agent «Lifetimes»: explain them with a struct that holds a reference to a string, and say why the compiler needs an annotation there but not in a function with one argument --> are a separate topic.\n",
    html:
      // Line marks as in the core: the preview shows bookmarks with them.
      `<h1>${line(1)}Rust</h1><p>${line(3)}The language I write <a href="kknow://note/Projects/kknow">Projects/kknow</a> in. <a href="kknow://tag/rust">#rust</a> <a href="kknow://tag/study">#study</a></p>` +
      `<p>${line(5)}Ownership and <mark class="agent-note" title="add an example with &amp;mut">borrowing</mark> are still hard for me.</p>` +
      `<p>${line(7)}Lifetimes <span class="agent-note is-detached" title="explain them with a struct"></span> are a separate topic.</p>`,
  },
  {
    id: "Projects/kknow",
    title: "kknow",
    folder: "Projects",
    tags: ["rust", "projects/kknow"],
    content:
      "<!-- confluence: 987654321 -->\n# kknow\n\nA project in [[Rust]].\n\n## Plans\n\n- stage 3: search\n\n" +
      '```rust\n// entry point\nfn main() {\n    let name = "kknow";\n    println!("{name}");\n}\n```\n',
    // Code block as the core returns it: pulldown-cmark sets `class="language-…"`.
    html:
      `<h1>kknow</h1><p>A project in <a href="kknow://note/Rust">Rust</a>.</p><h2>Plans</h2><ul><li>stage 3: search</li></ul>` +
      '<pre><code class="language-rust">// entry point\nfn main() {\n    let name = &quot;kknow&quot;;\n    println!(&quot;{name}&quot;);\n}\n</code></pre>',
  },
  {
    // Nested folder: it shows that the tree has more than one level.
    // The name is long on purpose: the tree cuts it with an ellipsis, not a wrap.
    id: "Projects/Archive/Ideas I have not got around to yet",
    title: "Ideas I have not got around to yet",
    folder: "Projects/Archive",
    tags: [],
    content: "# Ideas\n\nThings not done yet, but too good to forget.\n",
    html: "<h1>Ideas</h1><p>Things not done yet, but too good to forget.</p>",
  },
  {
    id: "Team/Meetings",
    title: "Meetings",
    folder: "Team",
    tags: [],
    // Tasks: the palette lists the open ones, and a click on a box in the
    // preview marks it.
    content:
      "# Meetings\n\nWho, when and what about.\n\n" +
      "- [ ] send out the planning notes\n- [x] book a meeting room\n" +
      "  - [ ] invite Ira from support\n",
    html:
      `<h1>${line(1)}Meetings</h1><p>${line(3)}Who, when and what about.</p>` +
      `<ul><li>${line(5)}${box(false)}send out the planning notes</li>` +
      `<li>${line(6)}${box(true)}book a meeting room<ul>` +
      `<li>${line(7)}${box(false)}invite Ira from support</li></ul></li></ul>`,
  },
  {
    // Table with markup in cells: it shows editing and the toolbar.
    id: "Branches",
    title: "Branches",
    folder: null,
    tags: [],
    content:
      "# Branches\n\n| Branch | Where it lives | Why |\n| --- | --- | :---: |\n" +
      "| `studio-prod` | Prod Authoring | editors work here, the source of **truth** |\n" +
      "| `prod` | git | deployment; content comes from `studio-prod` |\n" +
      "| `feature/*` | fork of [[Rust]] | everyday work |\n",
    html:
      "<h1>Branches</h1>\n<table><thead><tr><th>Branch</th><th>Where it lives</th><th>Why</th></tr></thead>" +
      "<tbody><tr><td><code>studio-prod</code></td><td>Prod Authoring</td>" +
      "<td>editors work here, the source of <strong>truth</strong></td></tr></tbody></table>",
  },
  {
    // Wide table: a narrow ID column and long cells with unbreakable code
    // pieces. It shows whether the table fits the panel or spreads wide.
    id: "Constraints",
    title: "Constraints",
    folder: null,
    tags: [],
    content:
      "# Constraints\n\n| ID | Constraint | Source |\n| --- | --- | --- |\n" +
      "| C-1 | Keycloak stays the single source of truth for user accounts. | ADR-001 |\n" +
      "| C-3 | The access token must stay short: only the roles of the user, " +
      "never through `full_scope_allowed`. | ADR-004 |\n" +
      "| C-5 | We write Rego in the v1 syntax (`import rego.v1`) for OPA 1.x. | ADR-006 |\n" +
      "| C-6 | Site roles follow the pattern `{site}-{level}`, where the level is " +
      "`reader` or `manager`. | ADR-006 |\n",
    html:
      "<h1>Constraints</h1>\n<table><thead><tr><th>ID</th><th>Constraint</th>" +
      "<th>Source</th></tr></thead><tbody>" +
      "<tr><td>C-1</td><td>Keycloak stays the single source of truth for " +
      "user accounts.</td><td>ADR-001</td></tr>" +
      "<tr><td>C-3</td><td>The access token must stay short: only the roles " +
      "of the user, never through <code>full_scope_allowed</code>.</td><td>ADR-004</td></tr>" +
      "<tr><td>C-5</td><td>We write Rego in the v1 syntax (<code>import rego.v1</code>) " +
      "for OPA 1.x.</td><td>ADR-006</td></tr>" +
      "<tr><td>C-6</td><td>Site roles follow the pattern <code>{site}-{level}</code>, " +
      "where the level is <code>reader</code> or <code>manager</code>.</td><td>ADR-006</td></tr>" +
      "</tbody></table>",
  },
  {
    // Long note: on open the parser does not reach the end, and the widgets at
    // the end come from background parsing. It checks that the table and the
    // diagram at the end render without a click.
    id: "Long",
    title: "Long",
    folder: null,
    tags: [],
    content:
      "# Long\n\n" +
      Array.from({ length: 400 }, (_, i) => `Paragraph ${i + 1}: **text** about [[Rust]].\n`).join("\n") +
      "\n| Branch | Why |\n| --- | --- |\n| `dev` | integration |\n" +
      "\n```mermaid\nflowchart LR\n  Tail --> Parsed\n```\n",
    html: "<h1>Long</h1>",
  },
  {
    // Two diagrams, a valid one and one with a typo; the second shows the error display.
    id: "Diagram",
    title: "Diagram",
    folder: null,
    tags: [],
    content:
      "# Diagram\n\n```mermaid\nflowchart LR\n  Note --> Parse --> Graph\n```\n\n```mermaid\nflowchart LR\n  A -->\n```\n",
    html: `<h1>${line(1)}Diagram</h1>\n${line(3, true)}<pre class="mermaid">flowchart LR\n  Note --&gt; Parse --&gt; Graph\n</pre>\n${line(8, true)}<pre class="mermaid">flowchart LR\n  A --&gt;\n</pre>\n`,
  },
  {
    // Images: wide and tall ones show how CSS shrinks them; also one with a
    // caption size, one by file name and a missing one.
    id: "Images",
    title: "Images",
    folder: null,
    tags: [],
    content:
      "# Images\n\nWide:\n\n![panorama](img/wide.svg)\n\nTall, by file name:\n\n![[tall.svg]]\n\nWith a size: ![[wide.svg|200]] and more text.\n\nMissing: ![none](img/missing.png)\n\nFrom the web: ![](https://example.com/a.png)\n",
    html: `<h1>Images</h1>\n<p>Wide:</p>\n<p><img class="note-image" data-src="img/wide.svg" alt="panorama"></p>\n<p>Tall, by file name:</p>\n<p><img class="note-image" data-src="tall.svg" alt=""></p>\n<p>With a size: <img class="note-image" data-src="wide.svg" alt="" style="width:200px"> and more text.</p>\n<p>Missing: <img class="note-image" data-src="img/missing.png" alt="none"></p>\n<p>From the web: <img class="note-image" data-src="https://example.com/a.png" alt=""></p>\n`,
  },
  {
    // Embeds as the core renders them: a section of another note with a task
    // box in it, and a loop that stays a link.
    id: "Embeds",
    title: "Embeds",
    folder: null,
    tags: [],
    content: "# Embeds\n\n![[Reading#Books]]\n\n![[Embeds]]\n",
    html:
      `<h1>${line(1)}Embeds</h1>${line(3, true)}` +
      `<div class="embed" data-note="Reading" data-sub="Books"><div class="embed-head">` +
      `<a href="kknow://note/Reading#Books">Reading#Books</a></div><div class="embed-body">` +
      `<h2>Books</h2><p>The Rust book first, then the kknow docs.</p>` +
      `<ul><li><input disabled="" type="checkbox"> read more</li></ul></div></div>` +
      `${line(5, true)}<div class="embed embed-stopped" data-reason="loop" data-note="Embeds">` +
      `<a href="kknow://note/Embeds">Embeds</a></div>`,
  },
  {
    // Plain-text names of other notes: the note panel lists them as mentions
    // without a link. Sections and a long body give the outline a section to
    // follow on scroll.
    id: "Reading",
    title: "Reading",
    folder: null,
    tags: [],
    content:
      "# Reading\n\n## Books\n\nThe Rust book first, then the kknow docs.\n\n" +
      Array.from({ length: 60 }, (_, i) => `Note ${i + 1} about books.\n`).join("\n") +
      "\n## Articles\n\nNothing yet.\n",
    html:
      `<h1>${line(1)}Reading</h1><h2>${line(3)}Books</h2><p>${line(5)}The Rust book first, then the kknow docs.</p>` +
      Array.from({ length: 60 }, (_, i) => `<p>${line(7 + 2 * i)}Note ${i + 1} about books.</p>`).join("") +
      `<h2>${line(127)}Articles</h2><p>${line(129)}Nothing yet.</p>`,
  },
];

/** Mock image: an SVG of the needed size with a caption. */
function mockImage(target: string): ArrayBuffer {
  if (target.includes("missing")) throw coded("imageNotFound", `image not found: ${target}`, { value: target });
  const [width, height] = target.includes("tall") ? [600, 1800] : [2400, 900];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="100%" height="100%" fill="#7aa2f7"/><circle cx="50%" cy="50%" r="${height / 4}" fill="#e0af68"/><text x="24" y="80" font-size="64" fill="#1a1b26">${target}</text></svg>`;
  return new TextEncoder().encode(svg).buffer as ArrayBuffer;
}

/** A mock vault: the list entry plus its notes. */
/**
 * The lock session of a mock vault: the password, the recovery code and
 * whether the notes are open. The core keeps a key; the mock compares text.
 */
interface MockLock {
  password: string | null;
  code: string;
  unlocked: boolean;
}

/** The lock of each mock vault by ID; a vault without one has no password. */
const locks = new Map<string, MockLock>();

function lockOf(vault: MockVault = activeVault): MockLock {
  let lock = locks.get(vault.id);
  if (!lock) {
    lock = { password: null, code: "K7QF-2M9X-ABCD-EFGH-JKLM-NPQR-ST", unlocked: false };
    locks.set(vault.id, lock);
  }
  return lock;
}

/** The vault of a command: the one it names, else the active one. */
function vaultOf(args: Record<string, unknown> | undefined): MockVault {
  return vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
}

/** Opens the session as the core does; the code opens it too. */
function unlockMock(vault: MockVault, secret: string): "password" | "recoveryCode" {
  const lock = lockOf(vault);
  if (!lock.password) throw coded("noLockPassword", "the vault has no password for locked notes");
  const typed = secret.trim().toUpperCase().replace(/[\s-]/g, "");
  if (typed === lock.code.replace(/-/g, "")) {
    lock.unlocked = true;
    return "recoveryCode";
  }
  if (secret !== lock.password) throw coded("wrongPassphrase", "wrong passphrase or recovery code");
  lock.unlocked = true;
  return "password";
}

function checkPassword(password: string): void {
  if ([...password].length < 8) {
    throw coded("passphraseTooShort", "the passphrase must have at least 8 characters", { value: "8" });
  }
}

interface MockVault {
  id: string;
  name: string;
  path: string;
  color: string;
  /** Text labels, in the clean form of the core. */
  labels: string[];
  available: boolean;
  notes: MockNote[];
  /** Folders without notes: the note list cannot restore them, as in the core. */
  folders: string[];
  /** Trash entries from the oldest to the newest, with the notes they hold. */
  trash: MockTrashEntry[];
}

interface MockTrashEntry {
  id: string;
  path: string;
  deleted: number;
  notes: MockNote[];
  folders: string[];
}

/** A deleted note for the mock trash. */
function trashedNote(id: string, content: string): MockNote {
  const title = id.split("/").pop()!;
  return {
    id,
    title,
    folder: id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : null,
    tags: [],
    content,
    html: `<h1>${title}</h1>${content
      .split("\n")
      .slice(2)
      .filter(Boolean)
      .map((line) => `<p>${line}</p>`)
      .join("")}`,
  };
}

/** Moves notes and folders into a new trash entry, as `Vault::remove` does. */
function putInTrash(vault: MockVault, path: string, notes: MockNote[], folders: string[]): void {
  const now = Math.floor(Date.now() / 1000);
  const taken = new Set(vault.trash.map((entry) => entry.id));
  let id = String(now);
  for (let attempt = 2; taken.has(id); attempt++) id = `${now}-${attempt}`;
  vault.trash.push({ id, path, deleted: now, notes, folders });
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
    name: "notes (mock)",
    path: "~/notes (mock)",
    color: "blue",
    labels: ["work", "ai"],
    available: true,
    notes,
    folders: ["Drafts"],
    // Two old deletions: a folder and a note, so the trash has something to show.
    trash: [
      {
        id: "1760000000",
        path: "Old",
        deleted: Math.floor(Date.now() / 1000) - 12 * 24 * 60 * 60,
        notes: [
          trashedNote("Old/Ideas", "# Ideas\n\nOld ideas.\n"),
          trashedNote("Old/Plans", "# Plans\n\nOld plans.\n"),
        ],
        folders: ["Old"],
      },
      {
        id: "1761000000",
        path: "Draft.md",
        deleted: Math.floor(Date.now() / 1000) - 60 * 60,
        notes: [trashedNote("Draft", "# Draft\n\nDeleted an hour ago.\n")],
        folders: [],
      },
    ],
  },
  {
    id: "work-asw",
    name: "Work — ASW",
    path: "~/work (mock)",
    color: "violet",
    labels: ["work"],
    available: true,
    notes: [
      {
        id: "Budget",
        title: "Budget",
        folder: null,
        tags: ["office"],
        content: "# Budget\n\nOffice costs and compilers. #office\n",
        html: "<h1>Budget</h1><p>Office costs and compilers.</p>",
      },
      {
        id: "Meetings/Retro",
        title: "Retro",
        folder: "Meetings",
        tags: [],
        content: "# Retro\n\nWhat to fix in the process. About Rust too.\n",
        html: "<h1>Retro</h1><p>What to fix in the process.</p>",
      },
    ],
    folders: [],
    trash: [],
  },
  {
    id: "archive-2024",
    name: "Archive 2024",
    path: "/Volumes/Backup/Archive 2024",
    color: "amber",
    labels: [],
    available: false,
    notes: [],
    folders: [],
    trash: [],
  },
];

let activeVault = vaults[0];

/** The MCP server of the app. `?mcp=busy` in the address shows a port in use. */
const mcp = { enabled: true, port: 7377, token: "0123456789abcdef0123456789abcdef" };

function mcpView(): McpStatus {
  const busy = new URLSearchParams(location.search).get("mcp") === "busy";
  const running = mcp.enabled && !busy;
  return {
    ...mcp,
    running,
    error: mcp.enabled && busy ? "Address already in use (os error 48)" : null,
  };
}

/** The same form as `clean_labels` in the core: `#Work` is `work`. */
function cleanLabels(raw: string[]): string[] {
  const labels: string[] = [];
  for (const label of raw) {
    const clean = label.trim().replace(/^#+/, "").split(/\s+/).filter(Boolean).join("-").toLowerCase();
    if (clean && !labels.includes(clean)) labels.push(clean);
  }
  return labels;
}

function vaultView(vault: MockVault): VaultView {
  return {
    id: vault.id,
    name: vault.name,
    path: vault.path,
    color: vault.color,
    labels: [...vault.labels],
    active: vault.id === activeVault.id,
    available: vault.available,
    // The mock keeps one sync setting, the one of the active vault.
    git: vault.id === activeVault.id && remote?.provider.kind === "git",
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
 * Changed keys. They are in `localStorage`, not `sessionStorage`: the settings
 * open in another tab, and the main tab must see them when it gets focus.
 */
function keymapView(): KeymapView {
  let keys: Record<string, string[]> = {};
  try {
    keys = JSON.parse(localStorage.getItem("kknow:mock:keymap") ?? "{}");
  } catch {
    // A broken copy: no changes.
  }
  return { keys, file: "~/Library/Application Support/dev.kknow.desktop/keybindings.json", problems: [] };
}

/**
 * App settings.
 *
 * In the app they live in `settings.json`, shared by both windows. In the
 * browser they only need to survive page changes, so `sessionStorage` is enough.
 */
let appSettings: AppSettings = readSettings();

function keepSettings(settings: AppSettings): void {
  appSettings = settings;
  sessionStorage.setItem("kknow:mock:settings", JSON.stringify(settings));
}

// Each tab has its own copy of the mock; a change in the settings tab must
// reach the copy of the main tab, else its next read returns the old values.
void listen<AppSettings>("settings-changed", ({ payload }) => keepSettings(payload));

function readSettings(): AppSettings {
  const fallback: AppSettings = {
    locale: localStorage.getItem("kknow:locale") ?? "ru",
    theme: "light",
    noteLabel: "title",
    viewMode: "split",
    openLastNote: true,
    lineNumbers: true,
    noteScale: 100,
    autoUpdate: true,
    gitPath: null,
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
    "archive-2024",
    {
      vault: "archive-2024",
      state: "conflict",
      done: 0,
      total: 0,
      at: Date.now() - 42 * 60000,
      message: null,
      conflicts: 2,
      location: "/Volumes/Backup/Archive 2024",
      autoSyncMinutes: 15,
    },
  ],
]);

/** Saves the sync settings of the active vault, with the checks of the core. */
function configureSync(settings: SyncSettingsForm, secret: boolean): SyncSettings {
  const provider = settings.provider;

  if (provider.kind === "s3" && !provider.bucket.trim()) {
    throw coded("noBucket", "sync settings: no bucket is set");
  }
  if (provider.kind === "folder" && !provider.path.trim()) {
    throw coded("noFolder", "sync settings: no folder is set");
  }

  const location =
    provider.kind === "folder"
      ? provider.path
      : provider.kind === "git"
        ? provider.url
        : `${provider.endpoint ?? "s3:/"}/${provider.bucket}/${provider.prefix ?? ""}`;
  const needsSecret =
    provider.kind === "s3" || (provider.kind === "git" && provider.auth === "token");

  statuses.set(activeVault.id, {
    ...statusOf(activeVault.id),
    state: "idle",
    location,
    autoSyncMinutes: settings.autoSyncMinutes,
  });

  // As in the core: the flag stays for the same address and resets for a
  // new one.
  const same = remote?.location === location;
  remote = {
    ...settings,
    location,
    // A folder never has secrets; the core answers the same way.
    needsSecret,
    hasSecret: needsSecret && (secret || (remote?.hasSecret ?? false)),
    encrypted: same && remote?.encrypted === true,
    hasKey: same && remote?.hasKey === true,
  };
  return remote;
}

/**
 * The encryption of the mock storage: its passphrase and its recovery code.
 * A smoke test can start with a storage that another device encrypted.
 */
const MOCK_CODE = "K7QF-2MXA-PL4D-WN6E-ZR3T-YB5H-JC";
let storage: { passphrase: string; code: string } | null =
  sessionStorage.getItem("kknow:mock:encrypted") === "1"
    ? { passphrase: "correct horse", code: MOCK_CODE }
    : null;

/** The passphrase or the code, as `KeyFile::unlock` takes them. */
function opens(secret: string): boolean {
  const plain = (text: string) => text.replace(/[\s-]/g, "").toUpperCase();
  return storage !== null && (secret === storage.passphrase || plain(secret) === plain(storage.code));
}

function checkPassphrase(passphrase: string): void {
  if ([...passphrase].length < 8) {
    throw coded("passphraseTooShort", "the passphrase must have at least 8 characters", {
      value: "8",
    });
  }
}

function syncEncrypt(passphrase: string): EncryptionOn {
  if (!remote) throw coded("syncNotConfigured", "sync is not set up");
  if (storage) throw coded("alreadyEncrypted", "the storage is already encrypted");
  checkPassphrase(passphrase);
  storage = { passphrase, code: MOCK_CODE };
  remote = { ...remote, encrypted: true, hasKey: true };
  return { recoveryCode: MOCK_CODE, settings: remote };
}

function syncUnlock(secret: string): SyncSettings {
  if (!remote) throw coded("syncNotConfigured", "sync is not set up");
  if (!storage) throw coded("notEncrypted", "the storage is not encrypted");
  if (!opens(secret)) throw coded("wrongPassphrase", "wrong passphrase or recovery code");
  remote = { ...remote, encrypted: true, hasKey: true };
  return remote;
}

function syncChangePassphrase(current: string, next: string): void {
  if (!storage) throw coded("notEncrypted", "the storage is not encrypted");
  if (!opens(current)) throw coded("wrongPassphrase", "wrong passphrase or recovery code");
  checkPassphrase(next);
  storage.passphrase = next;
}

/**
 * The git repository of the active vault: a few changes, so the panel has
 * something to show. Staging moves a file between the lists, a commit empties
 * the staged list and adds to "ahead".
 */
const gitState: GitStatus = {
  branch: "main",
  ahead: 0,
  behind: 1,
  changes: [
    { path: "Index.md", from: null, staged: null, unstaged: "modified" },
    { path: "Ideas/New thought.md", from: null, staged: "added", unstaged: null },
    { path: "images/diagram.png", from: null, staged: null, unstaged: "untracked" },
  ],
};

function gitStatus(): GitStatus {
  if (remote?.provider.kind !== "git") {
    throw coded("syncNotConfigured", "sync is not set up");
  }
  return structuredClone(gitState);
}

/** Moves files into the index or out of it, as `git add` and `git reset` do. */
function gitMove(paths: string[], stage: boolean): GitStatus {
  gitStatus();
  for (const change of gitState.changes) {
    if (!paths.includes(change.path)) continue;
    if (stage && change.unstaged) {
      change.staged = change.unstaged === "untracked" ? "added" : change.unstaged;
      change.unstaged = null;
    } else if (!stage && change.staged) {
      change.unstaged = change.staged === "added" ? "untracked" : change.staged;
      change.staged = null;
    }
  }
  return gitStatus();
}

/**
 * Throws away one side of a change, as `git checkout` does: the work tree
 * side goes back to the index, the index side back to HEAD with the work tree.
 */
function gitDiscard(path: string, staged: boolean): GitStatus {
  gitStatus();
  gitState.changes = gitState.changes.filter((change) => {
    if (change.path !== path) return true;
    if (staged) return false;
    change.unstaged = null;
    return change.staged !== null;
  });
  return gitStatus();
}

/** A made-up diff of one file, in the shape of `git diff`. */
function gitDiff(path: string, staged: boolean): string {
  const change = gitStatus().changes.find((entry) => entry.path === path);
  const kind = staged ? change?.staged : change?.unstaged;
  if (!kind) return "";
  if (path.endsWith(".png")) {
    return `diff --git a/${path} b/${path}\nnew file mode 100644\nBinary files /dev/null and b/${path} differ\n`;
  }
  const head = `diff --git a/${path} b/${path}\n`;
  if (kind === "added" || kind === "untracked") {
    return `${head}new file mode 100644\n--- /dev/null\n+++ b/${path}\n@@ -0,0 +1,3 @@\n+# New thought\n+\n+A draft of an idea.\n`;
  }
  return (
    `${head}--- a/${path}\n+++ b/${path}\n@@ -1,4 +1,5 @@\n # Contents\n \n-Old line.\n+New line.\n+One more.\n The end.\n`
  );
}

/** Connects git with the checks of the core: the program, the address, the token. */
function connectGit(args: Record<string, unknown> | undefined): SyncSettings {
  if (!appSettings.gitPath) {
    throw coded("gitPathNotSet", "the path to git is not set: set it in the settings");
  }
  const url = (args?.url as string | null) ?? (remote?.provider.kind === "git" ? remote.provider.url : null);
  if (!url) throw coded("noGitUrl", "sync settings: no repository address is set");
  if (/^https?:\/\/[^/]*@/.test(url)) {
    throw coded("gitUrlCredentials", "the repository address contains a login or a token");
  }
  const auth = args?.auth as GitAuth;
  const token = Boolean(args?.token);
  if (auth === "token" && !token && !remote?.hasSecret) {
    throw coded("noGitToken", "repository token not found");
  }
  return configureSync(
    {
      provider: { kind: "git", url, auth, env: (args?.env as string | null) ?? null },
      policy: "keepBoth",
      autoSyncMinutes: 0,
    },
    token,
  );
}

/**
 * A page can start with sync already set up: the smoke tests put the settings
 * form into `sessionStorage` before the page loads. The settings window is
 * another tab with its own mock, so it cannot set up sync for this one.
 */
const presetSync = sessionStorage.getItem("kknow:mock:sync");
if (presetSync) configureSync(JSON.parse(presetSync) as SyncSettingsForm, false);

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
  if (note.locked && !lockOf().unlocked) {
    return { ...openView(note), content: "", html: "", closed: true, bookmarks: [] };
  }
  return openView(note);
}

function openView(note: MockNote): NoteView {
  const locked = note.locked === true;
  return {
    locked,
    closed: false,
    id: note.id,
    title: note.title,
    content: note.content,
    html: note.html,
    // The same scan as `note_backlinks`, so the toolbar count and the panel agree.
    backlinks: [...new Map(noteBacklinks(note.id).map((row) => [row.id, row])).values()].map(
      (row) => ({ id: row.id, title: row.title, label: null }),
    ),
    outgoing: [],
    tags: note.tags,
    bookmarks: locked ? [] : (note.bookmarks ?? []),
    threads: locked ? [] : threads.filter((thread) => thread.note === note.id),
    proposals: locked
      ? []
      : proposals
          .filter((proposal) => proposal.note === note.id)
          .map((proposal) => recount(proposal, note.content))
          .filter((proposal) => proposal.status === "pending"),
  };
}

/**
 * Rewrites `[[old…]]` links to the new IDs and returns the number of changed
 * notes. The core also keeps short names and Markdown links; the mock needs
 * only enough for the status bar message. The HTML stays as it was.
 */
function relink(vault: MockVault, moves: [string, string][]): number {
  let changed = 0;
  for (const note of vault.notes) {
    let content = note.content;
    for (const [from, to] of moves) {
      content = content.replace(
        new RegExp(`(!?\\[\\[\\s*)${escapeRegExp(from)}(\\s*[\\]|#])`, "g"),
        `$1${to}$2`,
      );
    }
    if (content === note.content) continue;
    note.content = content;
    changed += 1;
  }
  return changed;
}

/**
 * Bookmarks on lines that exist in the text, sorted and one per line, as in
 * the core. Names lose the outer spaces; an empty name is no name.
 */
function bookmarkLines(marks: BookmarkMark[], content: string): BookmarkMark[] {
  const count = content.split("\n").length - (content.endsWith("\n") ? 1 : 0);
  const byLine = new Map<number, BookmarkMark>();
  for (const { line, name } of marks) {
    if (line < 1 || line > count) continue;
    const wanted = name?.trim();
    const known = byLine.get(line);
    if (!known) byLine.set(line, wanted ? { line, name: wanted } : { line });
    else if (!known.name && wanted) known.name = wanted;
  }
  return [...byLine.values()].sort((a, b) => a.line - b.line);
}

/**
 * A link target as the core finds it: the full ID or the file name, then the
 * heading or `^block` line. The mock reads headings and blocks from the text.
 */
function linkTarget(from: string, link: string): LinkTargetView | null {
  const [target, subpath] = link.split("|")[0].split(/#(.*)/s);
  const wanted = target.trim().replace(/\.md$/i, "").toLowerCase();
  const note = wanted
    ? activeVault.notes.find(
        (each) =>
          each.id.toLowerCase() === wanted ||
          (each.id.split("/").pop() ?? "").toLowerCase() === wanted,
      )
    : activeVault.notes.find((each) => each.id === from);
  if (!note) return null;
  const lines = note.content.split("\n");
  const headings = lines.flatMap((text, index) => {
    const found = /^(#{1,6})\s+(.*?)\s*$/.exec(text);
    return found ? [{ level: found[1].length, text: found[2], line: index + 1 }] : [];
  });
  const blocks = lines.flatMap((text, index) => {
    const found = /(?:^|\s)\^([A-Za-z0-9-]+)\s*$/.exec(text);
    return found ? [{ id: found[1], line: index + 1 }] : [];
  });
  const sub = subpath?.trim() ?? "";
  let line: number | null = null;
  if (sub.startsWith("^")) {
    line = blocks.find((each) => each.id.toLowerCase() === sub.slice(1).toLowerCase())?.line ?? null;
  } else if (sub) {
    const last = (sub.split("#").pop() ?? sub).toLowerCase();
    line = headings.find((each) => each.text.toLowerCase() === last)?.line ?? null;
  }
  return { id: note.id, line, headings, blocks: blocks.map((each) => each.id) };
}

/**
 * Links to note `id` with their lines. The mock finds `[[name]]` and
 * `[[path]]` wikilinks by text; the core also knows Markdown links.
 */
function noteBacklinks(id: string): BacklinkLineView[] {
  const names = [id, id.split("/").pop() ?? id].map((each) => each.toLowerCase());
  const lines: BacklinkLineView[] = [];
  for (const note of [...activeVault.notes].sort((a, b) => a.id.localeCompare(b.id))) {
    // A locked note has no links in the index: it is never a source.
    if (note.id === id || note.locked) continue;
    note.content.split("\n").forEach((text, index) => {
      for (const found of text.matchAll(/!?\[\[([^\]|#]+)[^\]]*\]\]/g)) {
        if (!names.includes(found[1].trim().toLowerCase())) continue;
        const at = found.index ?? 0;
        lines.push({
          id: note.id,
          title: note.title,
          line: index + 1,
          before: text.slice(0, at).trimStart(),
          text: found[0],
          after: text.slice(at + found[0].length).trimEnd(),
        });
      }
    });
  }
  return lines;
}

/** Spans of `[[…]]` links: a name inside one is no mention. */
function linkSpans(text: string): [number, number][] {
  return [...text.matchAll(/!?\[\[[^\]]*\]\]/g)].map((found) => [
    found.index ?? 0,
    (found.index ?? 0) + found[0].length,
  ]);
}

/**
 * Places where the file name of note `id` stands as plain text. The mock
 * skips wikilinks and fenced code; the core also skips tags, HTML and
 * inline code.
 */
function unlinkedMentions(id: string): MentionView[] {
  const name = id.split("/").pop() ?? id;
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}_])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\p{L}\\p{N}_])`, "giu");
  const mentions: MentionView[] = [];
  for (const note of [...activeVault.notes].sort((a, b) => a.id.localeCompare(b.id))) {
    if (note.id === id || note.locked) continue;
    const links = linkSpans(note.content);
    let lineStart = 0;
    let code = false;
    note.content.split("\n").forEach((text, index) => {
      if (text.startsWith("```")) code = !code;
      for (const found of code ? [] : text.matchAll(pattern)) {
        const start = lineStart + (found.index ?? 0);
        const end = start + found[0].length;
        if (links.some(([from, to]) => start < to && from < end)) continue;
        mentions.push({
          id: note.id,
          title: note.title,
          line: index + 1,
          before: text.slice(0, found.index).trimStart(),
          text: found[0],
          after: text.slice((found.index ?? 0) + found[0].length).trimEnd(),
          start,
          end,
        });
      }
      lineStart += text.length + 1;
    });
  }
  return mentions;
}

/** Makes a mention a link, as the core does: `[[name]]` or `[[name|text]]`. */
function linkMention(from: string, start: number, end: number, text: string, target: string): LinkRef[] {
  const note = find(from);
  if (note.content.slice(start, end) !== text) {
    const line = note.content.slice(0, start).split("\n").length;
    throw coded("notAMention", `${from} has no mention on line ${line}`, { value: from, line: String(line) });
  }
  const name = target.split("/").pop() ?? target;
  const link = name === text ? `[[${name}]]` : `[[${name}|${text}]]`;
  note.content = note.content.slice(0, start) + link + note.content.slice(end);
  return view(find(target)).backlinks;
}

/**
 * A hover preview as the core gives it. The mock cuts the section by the
 * heading lines of the text and shows it as paragraphs, without Markdown.
 */
function linkPreview(from: string, link: string): LinkPreview | null {
  const target = linkTarget(from, link);
  if (!target) return null;
  const note = find(target.id);
  if (note.locked) {
    return { id: note.id, title: note.title, html: "", line: null, partMissing: false, locked: true };
  }
  const lines = note.content.split("\n");
  const sub = link.split("|")[0].split(/#(.*)/s)[1]?.trim() ?? "";
  const start = target.line;
  let part = lines;
  if (start !== null) {
    const level = /^(#+)\s/.exec(lines[start - 1])?.[1].length ?? 7;
    const end = lines.findIndex(
      (text, index) => index >= start && (/^(#+)\s/.exec(text)?.[1].length ?? 7) <= level,
    );
    part = lines.slice(start - 1, end < 0 ? lines.length : end);
  }
  // The whole note: its HTML without the line marks, as the core gives it.
  const whole = note.html.replace(/<span class="line-mark[^"]*" data-line="\d+"><\/span>/g, "");
  const html = start === null ? whole : part
    .filter((text) => text.trim() && !text.startsWith("<!--"))
    .map((text) => {
      const heading = /^(#+)\s+(.*)$/.exec(text);
      const escaped = (heading?.[2] ?? text).replace(/&/g, "&amp;").replace(/</g, "&lt;");
      return heading ? `<h${heading[1].length}>${escaped}</h${heading[1].length}>` : `<p>${escaped}</p>`;
    })
    .join("");
  return {
    id: note.id,
    title: note.title,
    html,
    line: start,
    partMissing: Boolean(sub) && start === null,
    locked: false,
  };
}

function find(id: string): MockNote {
  const note = activeVault.notes.find((candidate) => candidate.id === id);
  if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
  return note;
}

/** Hits in one vault, with the same query parsing as single-vault search. */
function hits(vault: MockVault, query: string): { note: MockNote; text: string }[] {
  const { field, text } = parseQuery(query);
  if (!text) return [];

  return vault.notes
    .filter((note) => {
      if (field === "title") return note.title.toLowerCase().includes(text);
      // The index has no text and no tags of a locked note: only its name.
      if (note.locked) return note.id.toLowerCase().includes(text);
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
    if (/^\w+:\(/.test(query.trim())) throw coded("searchQuery", "could not parse the query", { detail: query });
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
  /** The pause before each line, so the float shows a run as it goes. */
  lineMs?: number;
})[] = [
  {
    ...blank("confluence", "Send to Confluence"),
    script: 'wiki-publish replace $KKNOW_FM_CONFLUENCE "$KKNOW_PATH"',
    targets: ["note"],
    requires: ["confluence"],
    output: {
      code: 0,
      stdout: "uploading {path}\npage updated: https://wiki.example/pages/12345\n",
      stderr: "",
    },
  },
  {
    ...blank("lint", "Check links"),
    kind: "python",
    script: "import os\nprint(os.environ['KKNOW_PATH'])",
    match: ["Docs/**"],
    output: {
      code: 2,
      stdout: "checked: {target}\n",
      stderr: "two links lead nowhere:\n  [[Ghost]]\n  [[Not written yet]]\n",
    },
  },
  {
    ...blank("backup", "Pack an archive"),
    kind: "fish",
    script: "tar czf ~/backup.tgz $KKNOW_VAULT",
    targets: ["folder", "vault"],
    confirm: true,
    timeoutSeconds: 300,
    // Long on purpose: the float opens by itself, the lines come one by one,
    // and "stop" has time to work.
    lineMs: 700,
    output: {
      code: null,
      stdout:
        "packing the archive…\nJournal/2026-10.md\nJournal/2026-09.md\nProjects/kknow.md\nProjects/plan.md\narchive ready: 4 files\n",
      stderr: "",
    },
  },
  {
    // Broken on purpose: the settings form shows its problem, and the target
    // menu shows that an invalid command does not appear there.
    ...blank("broken", "Typo in the kind"),
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
function commandProblems(): CommandProblem[] {
  const problems: CommandProblem[] = [];

  for (const command of MOCK_COMMANDS) {
    const id = command.id;
    if (!id.trim()) {
      problems.push({ code: "noId" });
      continue;
    }
    if (!command.name.trim()) problems.push({ code: "noName", id });
    if (!command.script.trim()) problems.push({ code: "emptyScript", id });
    if (!MOCK_KINDS.includes(command.kind)) {
      problems.push({ code: "unknownKind", id, kind: command.kind, known: MOCK_KINDS.join(", ") });
    }
    if (command.targets.length === 0) problems.push({ code: "noTargets", id });
  }

  return problems;
}

/** Commands that can run; invalid ones do not reach the menu. */
function healthyCommands(): typeof MOCK_COMMANDS {
  const broken = new Set(commandProblems().map((problem) => ("id" in problem ? problem.id : "")));
  return MOCK_COMMANDS.filter((command) => command.id.trim() && !broken.has(command.id));
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
    "# kknow\n\nA notes app in Rust. See [[Rust]]: in a standalone file the link is broken.\n",
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
  if (content === undefined) throw coded("fileGone", `the file is gone and was removed from the list: ${path}`, { value: path });
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

/**
 * The mock starts with a downloaded version, so the status bar shows the
 * "Restart" button. After the "restart" the page reloads on the new version:
 * the flag in `sessionStorage` survives the reload.
 */
let update: UpdateStatus = sessionStorage.getItem("kknow:mock:updated")
  ? { state: "idle" }
  : { state: "ready", version: "0.2.0" };

/**
 * The mock `attachments/` folder: path → bytes. A repeated name gets a number,
 * as in the core. A dropped file has no bytes in the browser, so `mockImage`
 * draws it.
 */
const attachments = new Map<string, ArrayBuffer | null>();

function mockAttachment(name: string, bytes: ArrayBuffer | null): string {
  if (!isImageName(name)) throw coded("invalidImageName", `invalid image name: ${name}`, { value: name });
  const dot = name.lastIndexOf(".");
  let free = name;
  for (let n = 2; attachments.has(`attachments/${free}`); n++) {
    free = `${name.slice(0, dot)} ${n}${name.slice(dot)}`;
  }
  attachments.set(`attachments/${free}`, bytes);
  return `attachments/${free}`;
}

function isImageName(name: string): boolean {
  return /^[^./\\][^/\\]*\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i.test(name);
}

/**
 * The `Channel` of `@tauri-apps/api/core`: the core sends the messages of one
 * call through it. Here the mock calls `onmessage` itself.
 */
export class Channel<T> {
  onmessage: (message: T) => void = () => {};
}

/** Runs that a "stop" has reached, by their ID. */
const stoppedRuns = new Set<string>();

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function invoke<T>(
  command: string,
  rawArgs?: Record<string, unknown> | Uint8Array,
  options?: { headers?: Record<string, string> },
): Promise<T> {
  // A raw body (`save_attachment`) has no named arguments; they come in headers.
  const args = rawArgs instanceof Uint8Array ? undefined : rawArgs;
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
      keepSettings(args?.settings as AppSettings);
      // As in the core: every window learns about the change, this one too.
      emit("settings-changed", appSettings);
      return appSettings as T;
    }

    case "get_keymap":
      return keymapView() as T;

    case "set_keymap":
      localStorage.setItem("kknow:mock:keymap", JSON.stringify(args?.keys ?? {}));
      return keymapView() as T;

    case "reveal_keymap":
      console.info("reveal_keymap: the browser has no file manager");
      return undefined as T;

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

    case "set_vault_labels": {
      const vault = vaults.find((entry) => entry.id === args?.id);
      if (vault) vault.labels = cleanLabels(args?.labels as string[]);
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
        labels: [],
        available: true,
        notes: [],
        folders: [],
        trash: [],
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
      if (!found) throw coded("unknownVault", `vault not found: ${String(args?.id)}`, { value: String(args?.id) });
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
        tags: note.locked ? [] : note.tags,
        locked: note.locked === true,
      })) as T;
    }

    case "lock_state": {
      const lock = lockOf(vaultOf(args));
      return { password: lock.password !== null, unlocked: lock.unlocked } as T;
    }

    case "set_lock_password": {
      const lock = lockOf(vaultOf(args));
      if (lock.password) throw coded("lockPasswordExists", "the vault already has a password for locked notes");
      checkPassword(args?.password as string);
      lock.password = args?.password as string;
      lock.unlocked = true;
      return lock.code as T;
    }

    case "unlock_notes":
      return unlockMock(vaultOf(args), args?.secret as string) as T;

    case "change_lock_password": {
      const lock = lockOf(vaultOf(args));
      if (!lock.unlocked) throw coded("notesClosed", "locked notes are closed: enter the vault password");
      checkPassword(args?.password as string);
      lock.password = args?.password as string;
      return undefined as T;
    }

    case "lock_all":
      lockOf(vaultOf(args)).unlocked = false;
      return undefined as T;

    case "lock_note":
    case "remove_lock": {
      const vault = vaultOf(args);
      const lock = lockOf(vault);
      if (!lock.password) throw coded("noLockPassword", "the vault has no password for locked notes");
      if (!lock.unlocked) throw coded("notesClosed", "locked notes are closed: enter the vault password");
      const note = vault.notes.find((each) => each.id === id);
      if (!note) throw coded("notFound", `заметка не найдена: ${id}`, { value: id });
      note.locked = command === "lock_note";
      if (note.locked) note.bookmarks = [];
      return view(note) as T;
    }

    case "load_note":
      return view(find(id)) as T;

    case "save_note": {
      const note = find(id);
      if (note.locked && !lockOf().unlocked) {
        throw coded("notesClosed", "locked notes are closed: enter the vault password");
      }
      note.content = args?.content as string;
      const lines = args?.bookmarks as BookmarkMark[] | null;
      if (lines) note.bookmarks = bookmarkLines(lines, note.content);
      return { status: "saved", note: view(note) } as T;
    }

    case "set_bookmarks": {
      const note = find(id);
      note.bookmarks = bookmarkLines(args?.lines as BookmarkMark[], args?.content as string);
      return note.bookmarks as T;
    }

    case "list_bookmarks":
      return activeVault.notes.flatMap((note) =>
        (note.bookmarks ?? []).map<BookmarkView>(({ line, name }) => ({
          note: note.id,
          title: note.title,
          folder: note.folder,
          line,
          text: note.content.split("\n")[line - 1] ?? "",
          ...(name ? { name } : {}),
        })),
      ) as T;

    case "list_tasks":
      return activeVault.notes.flatMap((note) =>
        note.content.split("\n").flatMap<TaskView>((text, index) => {
          const found = taskBox(text);
          if (!found) return [];
          return [
            {
              note: note.id,
              title: note.title,
              folder: note.folder,
              line: index + 1,
              text: text.slice(found.at + 2).trim(),
              done: found.done,
            },
          ];
        }),
      ) as T;

    case "review_proposal": {
      const proposal = proposals.find((each) => each.id === args?.proposal);
      if (!proposal) throw coded("unknownProposal", `proposal ${String(args?.proposal)} not found`, { value: String(args?.proposal) });
      const note = find(proposal.note);
      const review = args?.review as Review;
      let content = note.content;
      for (const index of [...review.accept].sort((a, b) => a - b)) {
        const edit = proposal.edits[index];
        if (content.split(edit.old).length !== 2) {
          throw coded("staleEdit", `in proposal ${proposal.id}, change ${index + 1} no longer finds its text: the note has changed`, { value: proposal.id, line: String(index + 1) });
        }
        content = content.replace(edit.old, edit.new);
      }
      note.content = content;
      for (const index of review.accept) proposal.edits[index].state = "accepted";
      for (const index of review.reject) proposal.edits[index].state = "rejected";
      if (review.feedback) proposal.feedback = review.feedback;
      return view(note) as T;
    }

    case "dismiss_proposal": {
      const index = proposals.findIndex((each) => each.id === args?.proposal);
      if (index >= 0) proposals.splice(index, 1);
      return undefined as T;
    }

    case "reply_thread": {
      const note = find(args?.note as string);
      let thread = threads.find((each) => each.id === args?.thread);
      if (!thread) {
        const id = `t${threads.length + 1}`;
        thread = {
          id,
          note: note.id,
          anchor: "",
          task: String(args?.text ?? ""),
          status: "open",
          messages: [{ from: "me", text: String(args?.text ?? ""), at: Date.now() / 1000 }],
          created: Date.now() / 1000,
          updated: Date.now() / 1000,
        };
        threads.push(thread);
        const lines = note.content.split("\n");
        const number = (args?.line as number) - 1;
        lines[number] = (lines[number] ?? "").replace("<!-- @agent ", `<!-- @agent#${id} `).replace("<!-- @agent:", `<!-- @agent#${id}:`);
        note.content = lines.join("\n");
      }
      thread.messages.push({ from: "me", text: args?.message as string, at: Date.now() / 1000 });
      if (thread.status === "question" || thread.status === "inProgress") thread.status = "answered";
      thread.updated = Date.now() / 1000;
      return view(note) as T;
    }

    case "set_task": {
      const note = find(id);
      const number = args?.line as number;
      const done = args?.done as boolean;
      const lines = note.content.split("\n");
      const changed = setTaskBox(lines[number - 1] ?? "", done);
      if (changed === null) throw coded("notATask", `${note.id} has no task on line ${number}: the note has changed`, { value: note.id, line: String(number) });
      lines[number - 1] = changed;
      note.content = lines.join("\n");
      // The mock HTML is fixed, so only the box of that line changes.
      note.html = note.html.replace(
        new RegExp(`(data-line="${number}"></span>)<input[^>]*/>\\n`),
        `$1${box(done)}`,
      );
      return view(note) as T;
    }

    case "preview":
      return find(id).html as T;

    case "note_image":
    case "file_image": {
      const target = args?.target as string;
      return (attachments.get(target) ?? mockImage(target)) as T;
    }

    case "save_attachment": {
      const name = decodeURIComponent(options?.headers?.["kknow-name"] ?? "");
      const bytes = (rawArgs as Uint8Array).slice().buffer;
      return mockAttachment(name, bytes) as T;
    }

    case "attach_file": {
      const source = args?.source as string;
      return mockAttachment(source.slice(source.lastIndexOf("/") + 1), null) as T;
    }

    case "link_target":
      return linkTarget(args?.from as string, args?.target as string) as T;

    case "link_preview":
      return linkPreview(args?.from as string, args?.target as string) as T;

    case "note_backlinks":
      return noteBacklinks(args?.id as string) as T;

    case "unlinked_mentions":
      return unlinkedMentions(args?.id as string) as T;

    case "link_mention":
      return linkMention(
        args?.from as string,
        args?.start as number,
        args?.end as number,
        args?.text as string,
        args?.target as string,
      ) as T;

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
      const only = args?.vaults as string[] | null | undefined;
      return order
        .filter((vault) => vault.available && (!only || only.includes(vault.id)))
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
      console.info("reveal_note", id ?? "(vault)");
      return undefined as T;

    case "delete_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const index = asked.notes.findIndex((note) => note.id === id);
      if (index < 0) throw coded("notFound", `note not found: ${id}`, { value: id });
      putInTrash(asked, `${id}.md`, asked.notes.splice(index, 1), []);
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
      if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
      if (asked.notes.some((other) => other !== note && other.id === to)) {
        throw coded("noteExists", `note already exists: ${to}`, { value: to });
      }
      note.id = to;
      note.folder = to.includes("/") ? to.slice(0, to.lastIndexOf("/")) : null;
      return { id: to, relinked: relink(asked, [[id as string, to]]) } as T;
    }

    case "duplicate_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const note = asked.notes.find((note) => note.id === id);
      if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
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
      const moves: [string, string][] = [];
      for (const note of asked.notes) {
        if (!note.id.startsWith(prefix)) continue;
        moves.push([note.id, moved(note.id)]);
        note.id = moved(note.id);
        note.folder = note.id.slice(0, note.id.lastIndexOf("/"));
      }
      asked.folders = asked.folders.map((folder) =>
        folder === name ? to : folder.startsWith(prefix) ? moved(folder) : folder,
      );
      return relink(asked, moves) as T;
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
      const inside = (path: string) => path === name || path.startsWith(prefix);
      putInTrash(
        asked,
        name,
        asked.notes.filter((note) => inside(note.id)),
        asked.folders.filter(inside),
      );
      asked.notes = asked.notes.filter((note) => !inside(note.id));
      asked.folders = asked.folders.filter((folder) => !inside(folder));
      return undefined as T;
    }

    case "list_trash": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      return asked.trash
        .map<TrashEntryView>((entry) => ({
          id: entry.id,
          path: entry.path,
          deleted: entry.deleted,
          notes: entry.notes.map((note) => `${note.id}.md`),
        }))
        .reverse() as T;
    }

    case "restore_trash": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const index = asked.trash.findIndex((entry) => entry.id === args?.entry);
      if (index < 0) throw coded("trashEntryNotFound", `trash entry not found: ${String(args?.entry)}`, { value: String(args?.entry) });
      const entry = asked.trash[index];
      // As in the core: a taken file stops the whole restore.
      const taken = entry.notes.find((note) => asked.notes.some((other) => other.id === note.id));
      if (taken) throw coded("exists", `already exists: ${taken.id}.md`, { value: `${taken.id}.md` });
      asked.trash.splice(index, 1);
      asked.notes.push(...entry.notes);
      asked.folders.push(...entry.folders.filter((folder) => !asked.folders.includes(folder)));
      return entry.notes.map((note) => note.id) as T;
    }

    case "trash_note": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const entry = asked.trash.find((each) => each.id === args?.entry);
      const note = entry?.notes.find((each) => `${each.id}.md` === args?.note);
      if (!note) throw coded("trashEntryNotFound", `trash entry not found: ${String(args?.note)}`, { value: String(args?.note) });
      return note.html as T;
    }

    case "delete_trash_entry": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const index = asked.trash.findIndex((entry) => entry.id === args?.entry);
      if (index < 0) throw coded("trashEntryNotFound", `trash entry not found: ${String(args?.entry)}`, { value: String(args?.entry) });
      asked.trash.splice(index, 1);
      return undefined as T;
    }

    case "empty_trash": {
      const asked = vaults.find((entry) => entry.id === args?.vault) ?? activeVault;
      const count = asked.trash.length;
      asked.trash = [];
      return count as T;
    }

    case "list_themes":
      return {
        themes: [
          {
            id: "sepia",
            name: "Sepia",
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
            name: "Gruvbox Light",
            author: "kknow",
            base: "light",
            source: "builtIn",
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
      // A standalone file has no vault and no root: the core gives it only the
      // commands without `vaults` and `match`.
      const file = Boolean(args?.file);
      return {
        commands: healthyCommands()
          .filter((command) => command.targets.includes(kind))
          .filter((command) => !file || (command.vaults.length === 0 && command.match.length === 0))
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
      if (!command) throw coded("unknownCommand", `command not found: ${String(args?.command)}`, { value: String(args?.command) });

      // The browser has no real processes: the mock sends prepared output,
      // line by line, as the core does. Its job is to show the float.
      const run = args?.run as string;
      const lines = args?.lines as Channel<RunLine>;
      const stdout = command.output.stdout
        .replace(
          "{path}",
          (args?.file as string) ?? `/Users/kknow/${(args?.target as string) ?? "(vault)"}`,
        )
        .replace("{target}", (args?.kind as string) ?? "vault");
      const printed: RunLine[] = [
        ...stdout.split("\n").filter(Boolean).map((text) => ({ stream: "stdout" as const, text })),
        ...command.output.stderr
          .split("\n")
          .filter(Boolean)
          .map((text) => ({ stream: "stderr" as const, text })),
      ];

      const started = Date.now();
      const sent: RunLine[] = [];
      for (const line of printed) {
        await pause(command.lineMs ?? 60);
        if (stoppedRuns.has(run)) break;
        lines.onmessage(line);
        sent.push(line);
      }
      const stopped = stoppedRuns.delete(run);
      const text = (stream: RunLine["stream"]) =>
        sent
          .filter((line) => line.stream === stream)
          .map((line) => `${line.text}\n`)
          .join("");

      return {
        command: command.id,
        code: stopped ? null : command.output.code,
        stdout: text("stdout"),
        stderr: text("stderr"),
        timedOut: !stopped && command.output.code === null,
        stopped,
        truncated: false,
        millis: Date.now() - started,
      } satisfies RunOutcome as T;
    }

    case "stop_command":
      stoppedRuns.add(args?.run as string);
      return undefined as T;

    case "edit_commands":
      return manifestView() as T;

    case "save_commands": {
      // There is no file, so a "write" replaces the set in memory. Prepared
      // output stays with the command of the same id: otherwise a label edit
      // would remove its panel report.
      const outputs = new Map(MOCK_COMMANDS.map((command) => [command.id, command.output]));
      const incoming = (args?.commands as CommandSpec[]).map((spec) => ({
        ...spec,
        output: outputs.get(spec.id) ?? { code: 0, stdout: "done\n", stderr: "" },
      }));

      MOCK_COMMANDS.splice(0, MOCK_COMMANDS.length, ...incoming);
      sharedEnv = args?.env as Record<string, string>;
      return manifestView() as T;
    }

    case "share_theme":
      sharedThemes.add(args?.id as string);
      return undefined as T;

    case "mcp_status":
      return mcpView() as T;

    case "set_mcp":
      mcp.enabled = args?.enabled as boolean;
      mcp.port = args?.port as number;
      return mcpView() as T;

    case "reveal_themes":
      console.info("reveal_themes: the browser has no file manager");
      return undefined as T;

    case "reveal_commands":
      console.info("reveal_commands: the browser has no file manager");
      return undefined as T;

    case "sync_status":
      return statusOf(activeVault.id) as T;

    case "sync_statuses":
      return [...vaults.map((vault) => statusOf(vault.id))] as T;

    case "sync_settings":
      return remote as T;

    case "save_sync_settings":
      return configureSync(args?.settings as SyncSettingsForm, Boolean(args?.secret)) as T;

    case "sync_encrypt":
      return syncEncrypt(String(args?.passphrase ?? "")) as T;

    case "sync_unlock":
      return syncUnlock(String(args?.secret ?? "")) as T;

    case "sync_change_passphrase":
      syncChangePassphrase(String(args?.current ?? ""), String(args?.new ?? ""));
      return undefined as T;

    case "forget_sync_settings":
      remote = null;
      statuses.delete(activeVault.id);
      return undefined as T;

    case "git_locate":
      return "/opt/homebrew/bin/git" as T;

    case "git_version": {
      const path = String(args?.path ?? "");
      if (!path.endsWith("git")) throw coded("gitMissing", `git not found: ${path}`, { value: path });
      return "git version 2.47.1" as T;
    }

    case "connect_git":
      return connectGit(args) as T;

    case "git_status":
      return gitStatus() as T;

    case "git_stage":
      return gitMove(args?.paths as string[], true) as T;

    case "git_unstage":
      return gitMove(args?.paths as string[], false) as T;

    case "git_discard":
      return gitDiscard(args?.path as string, args?.staged as boolean) as T;

    case "git_diff":
      return gitDiff(args?.path as string, args?.staged as boolean) as T;

    case "git_commit": {
      gitStatus();
      if (!String(args?.message ?? "").trim()) {
        throw coded("gitEmptyMessage", "empty commit message");
      }
      gitState.changes = gitState.changes.filter((change) => {
        change.staged = null;
        return change.unstaged !== null;
      });
      gitState.ahead += 1;
      return gitStatus() as T;
    }

    case "sync_now": {
      if (!remote) throw coded("syncNotConfigured", "sync is not set up");
      // A device without the key: the error stays in the status, as in the
      // core, so the settings page shows the unlock form.
      if (storage && !(remote.encrypted && remote.hasKey)) {
        const error = coded(
          "storageLocked",
          "the storage is encrypted: enter the passphrase or the recovery code",
        );
        statuses.set(activeVault.id, { ...statusOf(activeVault.id), state: "error", message: error });
        throw error;
      }
      if (remote.provider.kind === "git") {
        const pushed = gitState.ahead;
        const downloaded = gitState.behind;
        gitState.ahead = 0;
        gitState.behind = 0;
        statuses.set(activeVault.id, {
          ...statusOf(activeVault.id),
          state: "idle",
          at: Date.now(),
        });
        return {
          uploaded: 0,
          downloaded,
          removedLocal: 0,
          removedRemote: 0,
          pushed,
          conflicts: [],
        } satisfies SyncReport as T;
      }
      statuses.set(activeVault.id, {
        ...statusOf(activeVault.id),
        state: "conflict",
        done: 2,
        total: 2,
        at: Date.now(),
        conflicts: 1,
        message: null,
      });
      // The mock always reports a conflict: there is no other place to see it.
      return {
        uploaded: 1,
        downloaded: 1,
        removedLocal: 0,
        removedRemote: 0,
        pushed: 0,
        conflicts: [{ id: "Index", copy: "Index (conflict 3f9a1c2b)" }],
      } satisfies SyncReport as T;
    }

    case "list_files":
      return [...looseFiles.keys()].map(fileEntry) as T;

    case "open_file": {
      const path = args?.path as string;
      if (!/\.(md|markdown|mdown)$/i.test(path)) throw coded("notMarkdown", `not a markdown file: ${path}`, { value: path });
      if (!looseFiles.has(path)) looseFiles.set(path, `# ${fileEntry(path).name}\n\nOpened in the mock.\n`);
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

    case "update_status":
      return update as T;

    case "check_update":
      await new Promise((resolve) => setTimeout(resolve, 600));
      return update as T;

    case "restart_to_update":
      sessionStorage.setItem("kknow:mock:updated", "1");
      setTimeout(() => location.reload(), 300);
      return undefined as T;

    case "claim_markdown":
      association = { supported: true, current: "dev.kknow.desktop", ours: true };
      return association as T;

    default:
      throw new Error(`the mock does not know the command ${command}`);
  }
}
