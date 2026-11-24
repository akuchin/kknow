import type { BookmarkView, GlobalResult, NoteSummary, TaskView } from "./api";
import { iconMarkup } from "./icons";
import { matches, shortcut, type Action } from "./actions";
import { t, type Key } from "./i18n";
import type { Heading } from "./outline";

const SEARCH_DELAY = 150;

const WORKING_DELAY = 250;

const RECENT_LIMIT = 8;

type Scope = "all" | "title" | "tag" | "bookmarks" | "tasks";

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

export interface PaletteAction {
  label: string;
  action?: Action;
  run: () => void;
}

export interface PaletteHost {
  search(query: string): Promise<GlobalResult[]>;
  scope(): string;
  recent(): NoteSummary[];
  total(): number;
  activeVault(): string | null;
  open(id: string, vault?: string): void;
  bookmarks(): Promise<BookmarkView[]>;
  openBookmark(id: string, line: number): void;
  tasks(): Promise<TaskView[]>;
  openTask(id: string, line: number): void;
  create(name: string): void;
  actions(): PaletteAction[];
  headings(): Heading[];
  goToHeading(heading: Heading): void;
}

interface Row {
  id: string;
  title: string;
  folder: string | null;
  snippet: string | null;
  vault?: { id: string; name: string; color: string };
  line?: number;
  heading?: Heading;
  task?: boolean;
}

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
  private outline = false;
  private rows: Row[] = [];
  private selected = 0;
  private timer: number | undefined;
  private issued = 0;
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
        <p class="palette-section" id="palette-actions-label">${t("palette.actions")}</p>
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

    dialog.onclick = (event) => {
      if (event.target === dialog) dialog.close();
    };

    dialog.onclose = () => {
      window.clearTimeout(this.timer);
      window.clearTimeout(this.working);
      delete document.body.dataset.palette;
    };
  }

  open(query = "", scope: Scope = "all"): void {
    this.setOutline(false);
    if (this.dialog.open) {
      this.renderActions();
      this.input.value = query;
      this.input.select();
      this.setScope(scope, { silent: true });
      void this.run();
      return;
    }

    this.input.value = query;
    this.setScope(scope, { silent: true });

    document.body.dataset.palette = "open";
    this.dialog.showModal();
    this.input.select();
    this.renderActions();
    void this.run();
  }

  openOutline(): void {
    this.setOutline(true);
    this.input.value = "";
    if (!this.dialog.open) {
      document.body.dataset.palette = "open";
      this.dialog.showModal();
    }
    void this.run();
  }

  close(): void {
    this.dialog.close();
  }

  private setOutline(on: boolean): void {
    this.outline = on;
    this.dialog.dataset.mode = on ? "outline" : "search";
    this.input.placeholder = t(on ? "palette.outlinePlaceholder" : "palette.placeholder");
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

  private query(): string {
    const text = this.input.value.trim();
    if (!text || this.scope === "all") return text;
    return `${this.scope}:(${text})`;
  }

  private async run(): Promise<void> {
    const text = this.input.value.trim();
    const ticket = ++this.issued;

    if (this.outline) {
      this.runOutline(text);
      return;
    }

    if (this.scope === "bookmarks") {
      await this.runBookmarks(text, ticket);
      return;
    }
    if (this.scope === "tasks") {
      await this.runTasks(text, ticket);
      return;
    }

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
        this.resultsTitle(text),
        nameable(text) ? t("palette.empty", { key: shortcut("submit") }) : t("palette.emptyPlain"),
        "found",
      );
    } catch {
      window.clearTimeout(this.working);
      if (ticket !== this.issued) return;
      this.show([], this.resultsTitle(text), t("palette.broken"), "found");
    }
  }

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

  private runOutline(text: string): void {
    const headings = this.host.headings();
    const wanted = text.toLowerCase();
    const rows = headings
      .filter((heading) => heading.text.toLowerCase().includes(wanted))
      .map<Row>((heading) => ({
        id: heading.text,
        title: heading.text,
        folder: null,
        snippet: null,
        line: heading.line,
        heading,
      }));
    this.show(
      rows,
      t("palette.outline"),
      headings.length === 0 ? t("palette.noHeadings") : t("palette.emptyPlain"),
      "outline",
      headings.length,
    );
  }

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

  private resultsTitle(text: string): string {
    const labels = this.host.scope();
    return labels
      ? t("palette.results.labels", { query: text, labels })
      : t("palette.results", { query: text });
  }

  private showWorking(text: string): void {
    this.list.replaceChildren();
    this.message.textContent = t("palette.working");
    this.message.hidden = false;
    this.label.textContent = this.resultsTitle(text);
    this.count.textContent = "";
  }

  private show(
    rows: Row[],
    label: string,
    empty: string,
    kind: "recent" | "found" | "outline",
    total = this.host.total(),
  ): void {
    this.rows = rows;
    this.selected = 0;
    this.label.textContent = label;

    this.list.replaceChildren(...rows.map((row, index) => this.rowItem(row, index)));
    this.message.textContent = empty;
    this.message.hidden = rows.length > 0;
    this.count.textContent =
      kind === "found"
        ? t("palette.found", { count: rows.length })
        : t("palette.count", { shown: rows.length, total });

    this.markSelected();
  }

  private rowItem(row: Row, index: number): HTMLLIElement {
    const item = document.createElement("li");

    const button = document.createElement("button");
    button.type = "button";
    button.className = "palette-row";
    button.title = row.id;
    button.onclick = () => this.pick(row);
    button.onmousemove = () => {
      if (this.selected === index) return;
      this.selected = index;
      this.markSelected();
    };

    const mark = document.createElement("span");
    mark.className = "palette-icon";
    mark.innerHTML = iconMarkup(
      row.heading ? "heading" : row.task ? "task" : row.line === undefined ? "note" : "bookmark",
    );
    if (row.heading) button.style.setProperty("--level", String(row.heading.level - 1));

    const text = document.createElement("span");
    text.className = "palette-text";

    const title = document.createElement("span");
    title.className = "palette-row-title";
    title.textContent = row.title;
    text.append(title);

    if (row.snippet) {
      const snippet = document.createElement("span");
      snippet.className = "palette-snippet";
      snippet.innerHTML = row.snippet;
      text.append(snippet);
    }

    button.append(mark, text);

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

    const name = this.input.value.trim();
    if (this.outline || this.scope !== "all" || !nameable(name)) return;
    this.close();
    this.host.create(name);
  }

  private pick(row: Row): void {
    this.close();
    if (row.heading) {
      this.host.goToHeading(row.heading);
      return;
    }
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
