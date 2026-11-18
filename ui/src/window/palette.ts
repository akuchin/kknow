// The search palette: its sources and actions. The links of the preview live
// here too: a tag in the preview opens the palette with its query. So do the
// links of the editor text: the data for `[[` completion and the jump on
// ⌘-click. The editor finds the link; the core resolves it in the active
// vault. A standalone file has no vault: no completion, no jump.
//
// The module does not touch the DOM on import: `initPalette` creates the
// palette where the old top-level code created it.

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

/**
 * Search palette.
 *
 * It has no state of its own: notes, actions and open history live in the
 * zones of the window. The palette only shows them and calls back.
 */
export let palette: Palette;

/**
 * Wires the links of the preview and of the editor, then creates the palette
 * and its button.
 */
export function initPalette(): void {
  editor.setLinks(editorLinks);

  // Links in the preview navigate inside the vault, not to the outside.
  el.preview.addEventListener("click", (event) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (anchor) {
      if (followAnchor(anchor)) event.preventDefault();
      return;
    }
    // A click on an embed opens its note at its section. A click that ends
    // a selection selects text instead.
    const embed = (event.target as HTMLElement).closest<HTMLElement>(".embed:not(.embed-stopped)");
    if (!embed?.dataset.note || !document.getSelection()?.isCollapsed) return;
    const { note, sub } = embed.dataset;
    void follow(sub ? `${note}#${sub}` : note);
  });
  initHover();

  palette = new Palette({
    // Search all vaults at once: the user wants the note, the vault is secondary.
    // The active vault comes first; results from others carry their vault label.
    // The label filter of the sidebar narrows the search too: the user picked
    // a group of vaults to work in.
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
    // The editor has the text of a standalone file too, so it gets the outline.
    headings: () => editor.getHeadings(),
    goToHeading,
    actions: () => [
      // Commands of the open note come first: the other actions are about the
      // window, and a command is about what is open in it.
      ...openNoteCommands().map((command) => ({
        label: command.name,
        // Commands have no keys: each user has a different set, and a key for
        // them would take one from the window.
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
      // Only while locked notes are open: there is nothing to close otherwise.
      ...(canLockAll() ? [{ label: t("palette.action.lockAll"), run: () => void lockAll() }] : []),
      { label: t("menu.scale.larger"), action: "scale.larger", run: () => changeCurrentScale(1) },
      { label: t("menu.scale.smaller"), action: "scale.smaller", run: () => changeCurrentScale(-1) },
      { label: t("menu.scale.reset"), action: "scale.reset", run: () => changeCurrentScale(0) },
    ],
  });
  el.openPalette.onclick = () => palette.open();
}

/**
 * "Resolve": switch to the vault and show the conflict copies.
 *
 * A copy is a file with a marker in its name, so search is the way to find
 * it: a query for the marker lists everything to resolve.
 */
export async function resolveConflicts(id: string): Promise<void> {
  await switchVault(id);
  palette.open(CONFLICT_QUERY);
}

/**
 * The word in the names of conflict copies (`Note (conflict 3f9a1c2b).md`).
 * It is part of the file name, not UI text, so it is the same in every language.
 */
const CONFLICT_QUERY = "conflict";

/** The open vault note, or `null` for a standalone file and an empty window. */
function vaultNote(): string | null {
  return state.vault && state.note && !state.note.file ? state.note.id : null;
}

async function anchors(target: string): Promise<LinkAnchors | null> {
  const from = vaultNote();
  if (!from) return null;
  try {
    return await api.linkTarget(from, target);
  } catch {
    // Completion is a hint: an error must not show up while the user types.
    return null;
  }
}

/**
 * Follows a link of the preview or of a hover popup. `false`: it is no link
 * of ours, and the browser can have the click.
 */
function followAnchor(anchor: HTMLAnchorElement): boolean {
  const link = parseLink(anchor.getAttribute("href") ?? "");
  if (!link) return false;

  if (link.kind === "note" && !link.sub) {
    void openNote(link.value);
  } else if (link.kind === "note" || link.kind === "heading") {
    // The heading or the block of the link: the core finds its line.
    void follow(linkText(link) ?? link.value);
  } else if (link.kind === "missing") {
    void createNote(link.value);
  } else if (link.kind === "tag") {
    // A tag in the preview is a ready query: the palette opens with it.
    palette.open(`tag:${link.value}`);
  }
  return true;
}

/** The note and the line of a hover preview. */
async function openPreview(preview: LinkPreview): Promise<void> {
  if (preview.id !== state.note?.id) await openNote(preview.id);
  if (!preview.line || state.note?.id !== preview.id) return;
  editor.goToLine(preview.line, "start");
  if (state.mode !== "edit") scrollToLine(el.preview, preview.line);
}

/** ⌘ in the window: the key that follows a link and shows its preview. */
function followKey(event: MouseEvent | KeyboardEvent): boolean {
  return MAC ? event.metaKey : event.ctrlKey;
}

let hover: HoverPreview;

/**
 * ⌘ and the pointer on a link of the editor or of the preview show the
 * linked note in a popup. A standalone file has no vault, so no preview.
 */
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

  // The preview: the anchor under the pointer, also when ⌘ goes down later.
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

  // Embeds of live preview get the same answers as the popup. A note that
  // embeds itself whole would show itself inside itself: a loop, as in the
  // preview.
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

  // Another note: its links resolve from a different place. The editor gets
  // the new text after this event, so its embeds load fresh.
  let shown: string | null = null;
  state.on("note", (note) => {
    if ((note?.id ?? null) === shown) return;
    shown = note?.id ?? null;
    hover.hide();
    hover.clear();
    clearEmbeds();
  });
  // A note changed on disk: a cached preview or embed can be old.
  void onVaultChanged(() => {
    hover.clear();
    editor.redrawEmbeds();
  });
}

/**
 * Opens the note of a link, at its heading or block. A link to a note that
 * does not exist creates it, as a click in the preview does.
 */
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
    if (found.id !== state.note?.id) await openNote(found.id);
    if (found.line && state.note?.id === found.id) {
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
