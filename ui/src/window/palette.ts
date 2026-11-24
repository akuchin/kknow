import { MAC } from "../actions";
import { api, linkText, onVaultChanged, parseLink, type LinkPreview } from "../api";
import { renderDiagrams } from "../diagram";
import type { EditorLinks } from "../editor";
import { addCopyButtons } from "../code-copy";
import { paintCodeBlocks } from "../highlight";
import { HoverPreview } from "../hover-preview";
import { clearEmbeds, setEmbedSource } from "../live-embeds";
import { paintImages } from "../images";
import type { LinkAnchors } from "../link-complete";
import { scrollToLine } from "../preview-lines";
import { t } from "../i18n";
import { MODES } from "../note-mode";
import { Palette } from "../palette";
import { currentTarget, openNoteCommands, runCommand } from "./commands";
import { el } from "./dom";
import { createNote } from "./edits";
import { editor } from "./editor";
import { pickFile } from "./files";
import { canLockAll, lockAll } from "./lock-view";
import { goToAdjacentBookmark, goToHeading, openBookmark, openTask, recentNotes, toggleCurrentBookmark } from "./marks";
import { changeCurrentScale, openFound, openNote, setMode, travel } from "./note";
import { state } from "./state";
import { fail } from "./status";
import { showGit } from "./git";
import { isGit, runSync } from "./sync";
import { activeNotes, filteredVaults, pickedLabelsText, toggleSidebar } from "./tree";
import { openSettings } from "./util";
import { reloadVault, switchVault } from "./vaults";

export let palette: Palette;

export function initPalette(): void {
  editor.setLinks(editorLinks);

  el.preview.addEventListener("click", (event) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (anchor) {
      if (followAnchor(anchor)) event.preventDefault();
      return;
    }
    const embed = (event.target as HTMLElement).closest<HTMLElement>(".embed:not(.embed-stopped)");
    if (!embed?.dataset.note || !document.getSelection()?.isCollapsed) return;
    const { note, sub } = embed.dataset;
    void follow(sub ? `${note}#${sub}` : note);
  });
  initHover();

  palette = new Palette({
    search: (query) => api.searchAll(query, undefined, filteredVaults() ?? undefined),
    scope: () => pickedLabelsText(),
    bookmarks: () => (state.vault ? api.listBookmarks() : Promise.resolve([])),
    openBookmark: (id, line) => void openBookmark(id, line),
    tasks: () => (state.vault ? api.listTasks() : Promise.resolve([])),
    openTask: (id, line) => void openTask(id, line),
    recent: () => recentNotes(activeNotes()),
    total: () => activeNotes().length,
    activeVault: () => state.vault?.id ?? null,
    open: (id, from) => void openFound(id, from),
    create: (name) => void createNote(name),
    headings: () => editor.getHeadings(),
    goToHeading,
    actions: () => [
      ...openNoteCommands().map((command) => ({
        label: command.name,
        run: () => {
          const target = currentTarget();
          if (target) void runCommand(command, target);
        },
      })),
      { label: t("palette.action.newNote"), action: "note.new", run: () => el.newNote.click() },
      { label: t("palette.action.openFile"), action: "file.open", run: () => void pickFile() },
      ...MODES.map((each) => ({
        label: t("palette.action.mode", { mode: t(`mode.${each}`) }),
        run: () => setMode(each),
      })),
      { label: t("palette.action.bookmark"), action: "bookmark.toggle", run: () => toggleCurrentBookmark() },
      { label: t("palette.action.bookmarks"), action: "bookmark.list", run: () => palette.open("", "bookmarks") },
      { label: t("palette.action.bookmarkNext"), action: "bookmark.next", run: () => goToAdjacentBookmark(1) },
      {
        label: t("palette.action.bookmarkPrevious"),
        action: "bookmark.previous",
        run: () => goToAdjacentBookmark(-1),
      },
      { label: t("palette.action.outline"), action: "note.outline", run: () => palette.openOutline() },
      { label: t("palette.action.tasks"), action: "task.list", run: () => palette.open("", "tasks") },
      { label: t("palette.action.sidebar"), action: "sidebar.toggle", run: () => toggleSidebar() },
      { label: t("palette.action.back"), action: "history.back", run: () => void travel(-1) },
      { label: t("palette.action.forward"), action: "history.forward", run: () => void travel(1) },
      { label: t("palette.action.sync"), action: "vault.sync", run: () => void runSync() },
      {
        label: t("palette.action.gitChanges"),
        run: () => (isGit() ? void showGit() : void openSettings("vaults")),
      },
      {
        label: t("palette.action.syncSettings"),
        action: "settings.open",
        run: () => void openSettings("vaults"),
      },
      { label: t("palette.action.reload"), run: () => void reloadVault() },
      ...(canLockAll() ? [{ label: t("palette.action.lockAll"), run: () => void lockAll() }] : []),
      { label: t("menu.scale.larger"), action: "scale.larger", run: () => changeCurrentScale(1) },
      { label: t("menu.scale.smaller"), action: "scale.smaller", run: () => changeCurrentScale(-1) },
      { label: t("menu.scale.reset"), action: "scale.reset", run: () => changeCurrentScale(0) },
    ],
  });
  el.openPalette.onclick = () => palette.open();
}

export async function resolveConflicts(id: string): Promise<void> {
  await switchVault(id);
  palette.open(CONFLICT_QUERY);
}

const CONFLICT_QUERY = "conflict";

function vaultNote(): string | null {
  return state.vault && state.note && !state.note.file ? state.note.id : null;
}

async function anchors(target: string): Promise<LinkAnchors | null> {
  const from = vaultNote();
  if (!from) return null;
  try {
    return await api.linkTarget(from, target);
  } catch {
    return null;
  }
}

function followAnchor(anchor: HTMLAnchorElement): boolean {
  const link = parseLink(anchor.getAttribute("href") ?? "");
  if (!link) return false;

  if (link.kind === "note" && !link.sub) {
    void openNote(link.value);
  } else if (link.kind === "note" || link.kind === "heading") {
    void follow(linkText(link) ?? link.value);
  } else if (link.kind === "missing") {
    void createNote(link.value);
  } else if (link.kind === "tag") {
    palette.open(`tag:${link.value}`);
  }
  return true;
}

async function openPreview(preview: LinkPreview): Promise<void> {
  const vaultId = state.vault?.id;
  if (!state.isOpen(vaultId, preview.id)) await openNote(preview.id);
  if (!preview.line || !state.isOpen(vaultId, preview.id)) return;
  editor.goToLine(preview.line, "start");
  if (state.mode !== "edit") scrollToLine(el.preview, preview.line);
}

function followKey(event: MouseEvent | KeyboardEvent): boolean {
  return MAC ? event.metaKey : event.ctrlKey;
}

let hover: HoverPreview;

function initHover(): void {
  hover = new HoverPreview({
    load: (target) => {
      const from = vaultNote();
      return from ? api.linkPreview(from, target) : Promise.resolve(null);
    },
    paint: (root) => {
      paintCodeBlocks(root);
      addCopyButtons(root);
      paintImages(root);
      void renderDiagrams(root);
    },
    open: (preview) => void openPreview(preview),
    follow: (anchor) => void followAnchor(anchor),
  });

  let pointer: { x: number; y: number } | null = null;
  const peek = (on: boolean) => {
    if (!on || !pointer || !vaultNote()) return hover.point(null, 0, 0);
    const anchor = document.elementFromPoint(pointer.x, pointer.y)?.closest("a");
    const link =
      anchor && el.preview.contains(anchor) ? parseLink(anchor.getAttribute("href") ?? "") : null;
    hover.point(link ? linkText(link) : null, pointer.x, pointer.y);
  };
  el.preview.addEventListener("mousemove", (event) => {
    pointer = { x: event.clientX, y: event.clientY };
    peek(followKey(event));
  });
  el.preview.addEventListener("mouseleave", () => {
    pointer = null;
    peek(false);
  });
  for (const type of ["keydown", "keyup"] as const) {
    document.addEventListener(type, (event) => {
      if (pointer && (event.key === "Meta" || event.key === "Control")) peek(followKey(event));
    });
  }

  setEmbedSource({
    load: async (target) => {
      const from = vaultNote();
      const preview = from ? await api.linkPreview(from, target) : null;
      if (preview && preview.id === from && preview.line === null) {
        const loop = document.createElement("p");
        loop.className = "muted";
        loop.textContent = t("embed.loop");
        return { ...preview, html: loop.outerHTML };
      }
      return preview;
    },
    paint: (root, done) => {
      paintCodeBlocks(root);
      paintImages(root, done);
      void renderDiagrams(root).then(done);
    },
    open: (target) => void follow(target),
  });

  let shown: string | null = null;
  state.on("note", (note) => {
    if ((note?.id ?? null) === shown) return;
    shown = note?.id ?? null;
    hover.hide();
    hover.clear();
    clearEmbeds();
  });
  void onVaultChanged(() => {
    hover.clear();
    editor.redrawEmbeds();
  });
}

async function follow(target: string): Promise<void> {
  const from = vaultNote();
  if (!from) return;
  try {
    const found = await api.linkTarget(from, target);
    if (!found) {
      const name = target.split("#")[0].trim().replace(/\.md$/i, "");
      if (name) await createNote(name);
      return;
    }
    const vaultId = state.vault?.id;
    if (!state.isOpen(vaultId, found.id)) await openNote(found.id);
    if (found.line && state.isOpen(vaultId, found.id)) {
      editor.goToLine(found.line);
      if (state.mode !== "edit") scrollToLine(el.preview, found.line);
    }
  } catch (error) {
    fail(error);
  }
}

const editorLinks: EditorLinks = {
  notes: () => (vaultNote() ? activeNotes() : []),
  anchors,
  follow: (target) => void follow(target),
  peek: (target, x, y) => hover.point(vaultNote() ? target : null, x, y),
};
