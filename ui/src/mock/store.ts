import type {
  AppSettings,
  BookmarkMark,
  GitStatus,
  MarkdownAssociation,
  SyncSettings,
  VaultInfo,
  VaultView,
  ProposalView,
  Thread,
} from "../api";
import type { ErrorView } from "../error-view";

export function coded(code: string, message: string, params: Record<string, string> = {}): ErrorView {
  return { code, params, message };
}

export function line(number: number, block = false): string {
  return `<span class="line-mark${block ? " is-block" : ""}" data-line="${number}"></span>`;
}

export function box(done: boolean): string {
  return `<input disabled="" type="checkbox"${done ? ' checked=""' : ""}/>\n`;
}

export interface MockNote {
  id: string;
  title: string;
  folder: string | null;
  tags: string[];
  content: string;
  html: string;
  bookmarks?: BookmarkMark[];
  locked?: boolean;
}

export const threads: Thread[] = [
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

export const proposals: ProposalView[] = [
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

export function recount(proposal: ProposalView, content: string): ProposalView {
  const pending = proposal.edits.filter((edit) => edit.state === "pending").length;
  const accepted = proposal.edits.filter((edit) => edit.state === "accepted").length;
  const status =
    pending > 0 ? "pending" : accepted === proposal.edits.length ? "accepted" : accepted === 0 ? "rejected" : "partial";
  const stale = proposal.edits.map(
    (edit) => edit.state === "pending" && content.split(edit.old).length !== 2,
  );
  return { ...proposal, status, stale };
}

export const notes: MockNote[] = [
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
    html:
      `<h1>kknow</h1><p>A project in <a href="kknow://note/Rust">Rust</a>.</p><h2>Plans</h2><ul><li>stage 3: search</li></ul>` +
      '<pre><code class="language-rust">// entry point\nfn main() {\n    let name = &quot;kknow&quot;;\n    println!(&quot;{name}&quot;);\n}\n</code></pre>',
  },
  {
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
    id: "Diagram",
    title: "Diagram",
    folder: null,
    tags: [],
    content:
      "# Diagram\n\n```mermaid\nflowchart LR\n  Note --> Parse --> Graph\n```\n\n```mermaid\nflowchart LR\n  A -->\n```\n",
    html: `<h1>${line(1)}Diagram</h1>\n${line(3, true)}<pre class="mermaid">flowchart LR\n  Note --&gt; Parse --&gt; Graph\n</pre>\n${line(8, true)}<pre class="mermaid">flowchart LR\n  A --&gt;\n</pre>\n`,
  },
  {
    id: "Images",
    title: "Images",
    folder: null,
    tags: [],
    content:
      "# Images\n\nWide:\n\n![panorama](img/wide.svg)\n\nTall, by file name:\n\n![[tall.svg]]\n\nWith a size: ![[wide.svg|200]] and more text.\n\nMissing: ![none](img/missing.png)\n\nFrom the web: ![](https://example.com/a.png)\n",
    html: `<h1>Images</h1>\n<p>Wide:</p>\n<p><img class="note-image" data-src="img/wide.svg" alt="panorama"></p>\n<p>Tall, by file name:</p>\n<p><img class="note-image" data-src="tall.svg" alt=""></p>\n<p>With a size: <img class="note-image" data-src="wide.svg" alt="" style="width:200px"> and more text.</p>\n<p>Missing: <img class="note-image" data-src="img/missing.png" alt="none"></p>\n<p>From the web: <img class="note-image" data-src="https://example.com/a.png" alt=""></p>\n`,
  },
  {
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

export function mockImage(target: string): ArrayBuffer {
  if (target.includes("missing")) throw coded("imageNotFound", `image not found: ${target}`, { value: target });
  const [width, height] = target.includes("tall") ? [600, 1800] : [2400, 900];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="100%" height="100%" fill="#7aa2f7"/><circle cx="50%" cy="50%" r="${height / 4}" fill="#e0af68"/><text x="24" y="80" font-size="64" fill="#1a1b26">${target}</text></svg>`;
  return new TextEncoder().encode(svg).buffer as ArrayBuffer;
}

export interface MockLock {
  password: string | null;
  code: string;
  unlocked: boolean;
}

export const locks = new Map<string, MockLock>();

export function lockOf(vault: MockVault = session.activeVault): MockLock {
  let lock = locks.get(vault.id);
  if (!lock) {
    lock = { password: null, code: "K7QF-2M9X-ABCD-EFGH-JKLM-NPQR-ST", unlocked: false };
    locks.set(vault.id, lock);
  }
  return lock;
}

export function vaultOf(args: Record<string, unknown> | undefined): MockVault {
  return vaults.find((entry) => entry.id === args?.vault) ?? session.activeVault;
}

export function unlockMock(vault: MockVault, secret: string): "password" | "recoveryCode" {
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

export function checkPassword(password: string): void {
  if ([...password].length < 8) {
    throw coded("passphraseTooShort", "the passphrase must have at least 8 characters", { value: "8" });
  }
}

export interface MockVault {
  id: string;
  name: string;
  path: string;
  color: string;
  labels: string[];
  available: boolean;
  notes: MockNote[];
  folders: string[];
  trash: MockTrashEntry[];
}

export interface MockTrashEntry {
  id: string;
  path: string;
  deleted: number;
  notes: MockNote[];
  folders: string[];
}

export function trashedNote(id: string, content: string): MockNote {
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

export function putInTrash(vault: MockVault, path: string, notes: MockNote[], folders: string[]): void {
  const now = Math.floor(Date.now() / 1000);
  const taken = new Set(vault.trash.map((entry) => entry.id));
  let id = String(now);
  for (let attempt = 2; taken.has(id); attempt++) id = `${now}-${attempt}`;
  vault.trash.push({ id, path, deleted: now, notes, folders });
}

export const vaults: MockVault[] = [
  {
    id: "notes",
    name: "notes (mock)",
    path: "~/notes (mock)",
    color: "blue",
    labels: ["work", "ai"],
    available: true,
    notes,
    folders: ["Drafts"],
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

export function cleanLabels(raw: string[]): string[] {
  const labels: string[] = [];
  for (const label of raw) {
    const clean = label.trim().replace(/^#+/, "").split(/\s+/).filter(Boolean).join("-").toLowerCase();
    if (clean && !labels.includes(clean)) labels.push(clean);
  }
  return labels;
}

export function vaultView(vault: MockVault): VaultView {
  return {
    id: vault.id,
    name: vault.name,
    path: vault.path,
    color: vault.color,
    labels: [...vault.labels],
    active: vault.id === session.activeVault.id,
    available: vault.available,
    git: vault.id === session.activeVault.id && session.remote?.provider.kind === "git",
  };
}

export function vaultInfo(vault: MockVault): VaultInfo {
  return {
    id: vault.id,
    name: vault.name,
    root: vault.path,
    color: vault.color,
    noteCount: vault.notes.length,
  };
}

export function keepSettings(settings: AppSettings): void {
  session.appSettings = settings;
  sessionStorage.setItem("kknow:mock:settings", JSON.stringify(settings));
}

export function readSettings(): AppSettings {
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
export const MOCK_CODE = "K7QF-2MXA-PL4D-WN6E-ZR3T-YB5H-JC";
export function relink(vault: MockVault, moves: [string, string][]): number {
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

export function bookmarkLines(marks: BookmarkMark[], content: string): BookmarkMark[] {
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

export function find(id: string): MockNote {
  const note = session.activeVault.notes.find((candidate) => candidate.id === id);
  if (!note) throw coded("notFound", `note not found: ${id}`, { value: id });
  return note;
}


export const session = {
  activeVault: vaults[0],
  remote: null as SyncSettings | null,
  appSettings: readSettings(),
  storage: (sessionStorage.getItem("kknow:mock:encrypted") === "1"
    ? { passphrase: "correct horse", code: MOCK_CODE }
    : null) as { passphrase: string; code: string } | null,
  sharedEnv: { CONFLUENCE_SERVER: "https://wiki.example" } as Record<string, string>,
  association: {
    supported: true,
    current: "com.microsoft.VSCode",
    ours: false,
  } as MarkdownAssociation,
};
export const gitState: GitStatus = {
  branch: "main",
  ahead: 0,
  behind: 1,
  changes: [
    { path: "Index.md", from: null, staged: null, unstaged: "modified" },
    { path: "Ideas/New thought.md", from: null, staged: "added", unstaged: null },
    { path: "images/diagram.png", from: null, staged: null, unstaged: "untracked" },
  ],
};


export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
