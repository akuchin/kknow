/**
 * Palette: vault search in a separate overlay.
 *
 * Search used to be a field in the sidebar that replaced the folder tree.
 * While the field had text, the tree was hidden, and you had to clear the
 * field to get it back. The palette separates the two: the tree stays in
 * place, and search opens on top of it and closes on Esc without a trace.
 *
 * The overlay is a native `<dialog>`. It gives Esc, the backdrop, the focus
 * trap and focus return for free. A hand-made version is the same amount of
 * code, plus bugs.
 */

import type { BookmarkView, GlobalResult, NoteSummary, TaskView } from "./api";
import { iconMarkup } from "./icons";
import { matches, shortcut, type Action } from "./actions";
import { t, type Key } from "./i18n";

/** Delay before a query: each keystroke would otherwise call the core. */
const SEARCH_DELAY = 150;

/** Delay before the "still searching" message, so a fast answer does not show it. */
const WORKING_DELAY = 250;

/** How many recent notes to show for an empty query. */
const RECENT_LIMIT = 8;

/**
 * The field to search in.
 *
 * In the mockup the chips split hits by type: notes, blocks, tasks. kknow has
 * one type, the note, so there is nothing to split. But the search index has
 * fields, and the chips narrow the query to one of them: `title:(…)`, `tag:(…)`.
 */
type Scope = "all" | "title" | "tag" | "bookmarks" | "tasks";

/**
 * "Bookmarks" and "Tasks" are not index fields but separate lists. The
 * palette gets the whole list from the core and filters it itself: a vault
 * has few of them.
 */
const SCOPES: [Scope, Key][] = [
  ["all", "palette.scope.all"],
  ["title", "palette.scope.title"],
  ["tag", "palette.scope.tag"],
  ["bookmarks", "palette.scope.bookmarks"],
  ["tasks", "palette.scope.tasks"],
];

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** An action at the bottom of the palette: a label and the action with its keys. */
export interface PaletteAction {
  label: string;
  action?: Action;
  run: () => void;
}

/** Everything the palette needs from the app: it has no state of its own. */
export interface PaletteHost {
  /** Searches all vaults at once; each hit knows its vault. */
  search(query: string): Promise<GlobalResult[]>;
  /** Recently opened notes, shown instead of hits when the field is empty. */
  recent(): NoteSummary[];
  /** Note count of the active vault: the right number in the footer. */
  total(): number;
  /** The active vault: its hits get no vault label. */
  activeVault(): string | null;
  /** Opens a note; pass `vault` if it is in another vault. */
  open(id: string, vault?: string): void;
  bookmarks(): Promise<BookmarkView[]>;
  /** Opens the note at the bookmark line. */
  openBookmark(id: string, line: number): void;
  tasks(): Promise<TaskView[]>;
  /** Opens the note at the task line. */
  openTask(id: string, line: number): void;
  create(name: string): void;
  /** Rebuilt on each open: the labels depend on the window state. */
  actions(): PaletteAction[];
}

/** A result row: recent notes have no snippet, search hits do. */
interface Row {
  id: string;
  title: string;
  folder: string | null;
  snippet: string | null;
  /** The vault of the hit; notes of the active vault get no label. */
  vault?: { id: string; name: string; color: string };
  /** Bookmark or task line: the row then opens this place, not just the note. */
  line?: number;
  /** The row is a task, not a bookmark. */
  task?: boolean;
}

/**
 * Whether to offer a new note with this name.
 *
 * The query is also search syntax: `title:(` would give a file name with a
 * colon and a parenthesis. Offer creation only for plain text.
 */
function nameable(query: string): boolean {
  return query.length > 0 && !/[:()"*\\]/.test(query);
}

export class Palette {
  private readonly host: PaletteHost;

  private readonly dialog: HTMLDialogElement;
  private readonly input: HTMLInputElement;
  private readonly chips: HTMLDivElement;
  private readonly label: HTMLParagraphElement;
  private readonly list: HTMLUListElement;
  private readonly message: HTMLParagraphElement;
  private readonly actions: HTMLDivElement;
  private readonly count: HTMLSpanElement;

  private scope: Scope = "all";
  private rows: Row[] = [];
  private selected = 0;
  private timer: number | undefined;
  /**
   * Number of the last query.
   *
   * Answers come in any order: a short query runs faster than a long one and
   * can overtake it. Only the answer to the latest query redraws the list; the
   * others are dropped.
   */
  private issued = 0;
  /** Timer for the "searching" message; a fast answer comes before it fires. */
  private working: number | undefined;

  constructor(host: PaletteHost) {
    this.host = host;

    const dialog = document.createElement("dialog");
    dialog.className = "palette";
    dialog.id = "palette";
    dialog.setAttribute("aria-label", t("palette.title"));
    dialog.innerHTML = `
      <div class="palette-head">
        ${iconMarkup("search")}
        <input id="palette-input" type="text" autocomplete="off" spellcheck="false"
               placeholder="${t("palette.placeholder")}" aria-label="${t("palette.title")}" />
        <div class="palette-chips" role="group" aria-label="${t("palette.title")}">
          ${SCOPES.map(
            ([id, key]) =>
              `<button type="button" data-scope="${id}" aria-pressed="${id === "all"}">` +
              `${t(key)}</button>`,
          ).join("")}
        </div>
      </div>
      <div class="palette-body">
        <p class="palette-section" id="palette-label"></p>
        <ul class="palette-list" id="palette-results"></ul>
        <p class="palette-message" id="palette-message" hidden></p>
        <p class="palette-section">${t("palette.actions")}</p>
        <div class="palette-actions" id="palette-actions"></div>
      </div>
      <footer class="palette-foot">
        <span>${t("palette.hint.move", { keys: shortcut("list.previous") + shortcut("list.next") })}</span>
        <span>${t("palette.hint.open", { key: shortcut("submit") })}</span>
        <span>${t("palette.hint.close", { key: shortcut("cancel") })}</span>
        <span class="spacer"></span>
        <span id="palette-count"></span>
      </footer>
    `;
    // The palette lives outside #app. While it is open, the window behind it
    // is blurred, and a blur on the parent would blur the palette too.
    document.body.append(dialog);

    this.dialog = dialog;
    this.input = dialog.querySelector<HTMLInputElement>("#palette-input")!;
    this.chips = dialog.querySelector<HTMLDivElement>(".palette-chips")!;
    this.label = dialog.querySelector<HTMLParagraphElement>("#palette-label")!;
    this.list = dialog.querySelector<HTMLUListElement>("#palette-results")!;
    this.message = dialog.querySelector<HTMLParagraphElement>("#palette-message")!;
    this.actions = dialog.querySelector<HTMLDivElement>("#palette-actions")!;
    this.count = dialog.querySelector<HTMLSpanElement>("#palette-count")!;

    this.input.oninput = () => this.schedule();
    this.input.onkeydown = (event) => this.onKey(event);

    this.chips.onclick = (event) => {
      const button = (event.target as HTMLElement).closest("button");
      const scope = button?.dataset.scope as Scope | undefined;
      if (!scope || scope === this.scope) return;
      this.setScope(scope);
    };

    // A click outside the content hits the `<dialog>` itself: its area is the
    // backdrop.
    dialog.onclick = (event) => {
      if (event.target === dialog) dialog.close();
    };

    dialog.onclose = () => {
      window.clearTimeout(this.timer);
      window.clearTimeout(this.working);
      delete document.body.dataset.palette;
    };
  }

  /**
   * Opens the palette. `query` is the initial text, for example `tag:rust`;
   * `scope` is the initial chip, for example bookmarks.
   */
  open(query = "", scope: Scope = "all"): void {
    if (this.dialog.open) {
      this.input.value = query;
      this.input.select();
      this.setScope(scope, { silent: true });
      void this.run();
      return;
    }

    this.input.value = query;
    // The chip resets on each open. Otherwise the next search would silently
    // use a field picked long ago. A query like `tag:rust` narrows itself.
    this.setScope(scope, { silent: true });

    document.body.dataset.palette = "open";
    this.dialog.showModal();
    this.input.select();
    this.renderActions();
    void this.run();
  }

  close(): void {
    this.dialog.close();
  }

  private setScope(scope: Scope, options: { silent?: boolean } = {}): void {
    this.scope = scope;
    for (const chip of this.chips.querySelectorAll<HTMLButtonElement>("button")) {
      chip.setAttribute("aria-pressed", String(chip.dataset.scope === scope));
    }
    if (!options.silent) {
      this.input.focus();
      void this.run();
    }
  }

  private schedule(): void {
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.run(), SEARCH_DELAY);
  }

  /** Builds the query for the chosen chip: `word` → `title:(word)`. */
  private query(): string {
    const text = this.input.value.trim();
    if (!text || this.scope === "all") return text;
    return `${this.scope}:(${text})`;
  }

  private async run(): Promise<void> {
    const text = this.input.value.trim();
    const ticket = ++this.issued;

    if (this.scope === "bookmarks") {
      await this.runBookmarks(text, ticket);
      return;
    }
    if (this.scope === "tasks") {
      await this.runTasks(text, ticket);
      return;
    }

    // The first search opens vaults that were not opened yet. Reading and
    // parsing them takes noticeable time. A silent palette looks broken then,
    // so it tells the user what it is doing.
    if (text) this.working = window.setTimeout(() => this.showWorking(text), WORKING_DELAY);

    if (!text) {
      const recent = this.host.recent().slice(0, RECENT_LIMIT);
      this.show(
        recent.map((note) => ({
          id: note.id,
          title: note.title,
          folder: note.folder,
          snippet: null,
        })),
        t("palette.recent"),
        t("palette.emptyPlain"),
        "recent",
      );
      return;
    }

    try {
      const hits = await this.host.search(this.query());
      window.clearTimeout(this.working);
      if (ticket !== this.issued) return;

      this.show(
        hits.map((hit) => ({
          id: hit.id,
          title: hit.title,
          folder: hit.folder,
          snippet: hit.snippet,
          vault: { id: hit.vault, name: hit.vaultName, color: hit.color },
        })),
        t("palette.results", { query: text }),
        nameable(text) ? t("palette.empty", { key: shortcut("submit") }) : t("palette.emptyPlain"),
        "found",
      );
    } catch {
      // An incomplete query like "title:(" is not an error; there is just
      // nothing to search. The window status bar is too far from the eyes.
      window.clearTimeout(this.working);
      if (ticket !== this.issued) return;
      this.show([], t("palette.results", { query: text }), t("palette.broken"), "found");
    }
  }

  /**
   * Bookmarks filtered by the typed text: name, note title, path and line text.
   * A named bookmark shows its name as the title, and the note title moves to
   * the snippet next to the line text.
   */
  private async runBookmarks(text: string, ticket: number): Promise<void> {
    let bookmarks: BookmarkView[];
    try {
      bookmarks = await this.host.bookmarks();
    } catch {
      bookmarks = [];
    }
    if (ticket !== this.issued) return;

    const wanted = text.toLowerCase();
    const rows = bookmarks
      .filter(
        (bookmark) =>
          !wanted ||
          [bookmark.name ?? "", bookmark.title, bookmark.note, bookmark.text].some((field) =>
            field.toLowerCase().includes(wanted),
          ),
      )
      .map<Row>((bookmark) => {
        const text = bookmark.text.trim();
        const snippet = bookmark.name ? [bookmark.title, text].filter(Boolean).join(" — ") : text;
        return {
          id: bookmark.note,
          title: bookmark.name ?? bookmark.title,
          folder: bookmark.folder,
          snippet: escapeHtml(snippet) || null,
          line: bookmark.line,
        };
      });
    this.show(
      rows,
      t("palette.bookmarks"),
      bookmarks.length === 0
        ? t("palette.noBookmarks", { key: shortcut("bookmark.toggle") })
        : t("palette.emptyPlain"),
      "found",
    );
  }

  /**
   * Open tasks filtered by the typed text: task text, note title and path.
   * The task text is the row title, and the note title is the snippet.
   */
  private async runTasks(text: string, ticket: number): Promise<void> {
    let tasks: TaskView[];
    try {
      tasks = (await this.host.tasks()).filter((task) => !task.done);
    } catch {
      tasks = [];
    }
    if (ticket !== this.issued) return;

    const wanted = text.toLowerCase();
    const rows = tasks
      .filter(
        (task) =>
          !wanted ||
          [task.text, task.title, task.note].some((field) => field.toLowerCase().includes(wanted)),
      )
      .map<Row>((task) => ({
        id: task.note,
        title: task.text,
        folder: task.folder,
        snippet: escapeHtml(task.title),
        line: task.line,
        task: true,
      }));
    this.show(
      rows,
      t("palette.tasks"),
      tasks.length === 0 ? t("palette.noTasks") : t("palette.emptyPlain"),
      "found",
    );
  }

  /** Shows that the palette is busy while the core opens vaults. */
  private showWorking(text: string): void {
    this.list.replaceChildren();
    this.message.textContent = t("palette.working");
    this.message.hidden = false;
    this.label.textContent = t("palette.results", { query: text });
    this.count.textContent = "";
  }

  /**
   * The footer number depends on what is shown. For recent notes it is about
   * the vault ("8 of 9"); for hits it counts the hits. Search covers all
   * vaults, so a comparison with the active vault note count would be wrong.
   */
  private show(rows: Row[], label: string, empty: string, kind: "recent" | "found"): void {
    this.rows = rows;
    this.selected = 0;
    this.label.textContent = label;

    this.list.replaceChildren(...rows.map((row, index) => this.rowItem(row, index)));
    this.message.textContent = empty;
    this.message.hidden = rows.length > 0;
    this.count.textContent =
      kind === "recent"
        ? t("palette.count", { shown: rows.length, total: this.host.total() })
        : t("palette.found", { count: rows.length });

    this.markSelected();
  }

  private rowItem(row: Row, index: number): HTMLLIElement {
    const item = document.createElement("li");

    const button = document.createElement("button");
    button.type = "button";
    button.className = "palette-row";
    button.title = row.id;
    button.onclick = () => this.pick(row);
    // Mouse and keyboard move the same highlight. Otherwise Enter could open a
    // row other than the one the user looks at.
    button.onmousemove = () => {
      if (this.selected === index) return;
      this.selected = index;
      this.markSelected();
    };

    const mark = document.createElement("span");
    mark.className = "palette-icon";
    mark.innerHTML = iconMarkup(row.task ? "task" : row.line === undefined ? "note" : "bookmark");

    const text = document.createElement("span");
    text.className = "palette-text";

    const title = document.createElement("span");
    title.className = "palette-row-title";
    title.textContent = row.title;
    text.append(title);

    if (row.snippet) {
      const snippet = document.createElement("span");
      snippet.className = "palette-snippet";
      // Search builds the match highlight; its HTML is already escaped.
      snippet.innerHTML = row.snippet;
      text.append(snippet);
    }

    button.append(mark, text);

    // Only hits from other vaults get a vault label. The active vault is
    // already open, and its name on every row would be noise.
    if (row.vault && row.vault.id !== this.host.activeVault()) {
      const vault = document.createElement("span");
      vault.className = "palette-vault";

      const label = document.createElement("span");
      label.className = "vault-label";
      label.dataset.color = row.vault.color;

      const name = document.createElement("span");
      name.textContent = row.vault.name;

      vault.append(label, name);
      button.append(vault);
    }

    if (row.folder || row.line !== undefined) {
      const folder = document.createElement("span");
      folder.className = "palette-meta";
      const line = row.line === undefined ? "" : t("palette.line", { line: row.line });
      folder.textContent = [row.folder, line].filter(Boolean).join(" · ");
      button.append(folder);
    }

    item.append(button);
    return item;
  }

  private renderActions(): void {
    this.actions.replaceChildren(
      ...this.host.actions().map((action) => {
        const button = document.createElement("button");
        button.type = "button";
        button.className = "palette-action";
        button.onclick = () => {
          this.close();
          action.run();
        };

        const label = document.createElement("span");
        label.textContent = action.label;

        const hint = document.createElement("kbd");
        hint.textContent = action.action ? shortcut(action.action) : "";

        button.append(label, hint);
        return button;
      }),
    );
  }

  private markSelected(): void {
    const buttons = this.list.querySelectorAll<HTMLButtonElement>("button");
    buttons.forEach((button, index) => {
      const active = index === this.selected;
      button.classList.toggle("selected", active);
      if (active) button.scrollIntoView({ block: "nearest" });
    });
  }

  private onKey(event: KeyboardEvent): void {
    if (matches(event, "list.next")) {
      event.preventDefault();
      this.move(1);
    } else if (matches(event, "list.previous")) {
      event.preventDefault();
      this.move(-1);
    } else if (matches(event, "submit")) {
      event.preventDefault();
      this.activate();
    }
  }

  /** Wraps around: arrow down on the last row goes to the first one. */
  private move(delta: number): void {
    if (this.rows.length === 0) return;
    this.selected = (this.selected + delta + this.rows.length) % this.rows.length;
    this.markSelected();
  }

  private activate(): void {
    const row = this.rows[this.selected];
    if (row) {
      this.pick(row);
      return;
    }

    // Empty list and a valid name: Enter creates a note. A search miss usually
    // means the note does not exist yet.
    const name = this.input.value.trim();
    if (this.scope !== "all" || !nameable(name)) return;
    this.close();
    this.host.create(name);
  }

  private pick(row: Row): void {
    this.close();
    if (row.task && row.line !== undefined) {
      this.host.openTask(row.id, row.line);
      return;
    }
    if (row.line !== undefined) {
      this.host.openBookmark(row.id, row.line);
      return;
    }
    this.host.open(row.id, row.vault?.id);
  }
}
