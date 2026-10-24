// The note toolbar in the header: the note menu and the bookmarks, run and
// backlinks menus. The counts on its buttons belong to the note zone
// (`renderNoteTools`) and the commands zone (`renderRunButton`).
//
// The module does not touch the DOM on import.

import { t } from "../i18n";
import { openMenu, type MenuItem } from "../menu";
import {
  commandItems,
  currentTarget,
  editCommandsSection,
  lastCommand,
  runCommand,
} from "./commands";
import { el } from "./dom";
import { openCommandMenu } from "./edits";
import { editor } from "./editor";
import { openFileMenu } from "./files";
import { openBookmark, toggleCurrentBookmark } from "./marks";
import { openNote } from "./note";
import { palette } from "./palette";
import { state } from "./state";

/** Wires the buttons of the header. */
export function initToolbar(): void {
  el.noteMenu.onclick = () => openNoteMenu();
  el.noteBookmarks.onclick = () => openBookmarksMenu();
  el.runLast.onclick = () => {
    const last = lastCommand();
    const target = currentTarget();
    // Nothing to repeat yet: the menu shows what can run.
    if (last && target) void runCommand(last, target);
    else void openRunMenu();
  };
  el.runMenu.onclick = () => void openRunMenu();
  el.noteBacklinks.onclick = () => openBacklinksMenu();
}

/**
 * Menu of the open note. It is the same as its tree row menu: commands,
 * rename, clipboard, Finder. One menu for two places keeps them in
 * sync. A separate file uses its row menu from the file list.
 */
function openNoteMenu(): void {
  if (!state.note) return;
  if (state.note.file) {
    const path = state.note.id;
    const entry = state.files.find((candidate) => candidate.path === path);
    if (entry) void openFileMenu(el.noteMenu, entry);
    return;
  }
  if (!state.vault) return;
  void openCommandMenu(el.noteMenu, { kind: "note", name: state.note.id, vault: state.vault.id });
}

/**
 * Bookmarks of the open note: a jump to each, then the toggle and the list of
 * all bookmarks. The toolbar has no line of its own, so the toggle works on
 * the cursor line, as ⌘⇧B does.
 */
function openBookmarksMenu(): void {
  if (!state.note || state.note.file) return;
  const id = state.note.id;
  const marks = editor.getBookmarkMarks();
  const text = editor.getText().split("\n");

  const jumps: MenuItem[] =
    marks.length > 0
      ? marks.map(({ line, name }, index) => ({
          label: name ?? bookmarkLabel(text[line - 1] ?? ""),
          detail: t("topbar.bookmarks.line", { line }),
          glyph: "bookmark",
          key: index < 9 ? String(index + 1) : undefined,
          run: () => void openBookmark(id, line),
        }))
      : [{ label: t("topbar.bookmarks.none"), disabled: true, run: () => {} }];

  openMenu(el.noteBookmarks, [
    { title: t("topbar.bookmarks.here"), items: jumps },
    {
      items: [
        {
          label: t("palette.action.bookmark"),
          glyph: "bookmark",
          action: "bookmark.toggle",
          run: () => toggleCurrentBookmark(),
        },
        {
          label: t("topbar.bookmarks.all"),
          glyph: "search",
          action: "bookmark.list",
          run: () => palette.open("", "bookmarks"),
        },
      ],
    },
  ]);
}

/**
 * A bookmark line as a menu label: the text without the markdown marker and
 * without agent notes, which are HTML comments in the text.
 */
function bookmarkLabel(line: string): string {
  const text = line
    .replace(/<!--.*?(-->|$)/g, "")
    .replace(/^\s*(#{1,6}\s+|[-*+]\s+(\[.\]\s+)?|\d+[.)]\s+|>\s*)/, "")
    .trim();
  return text || t("topbar.bookmarks.blank");
}

/** Commands of the open note, the same list as in its menu. */
async function openRunMenu(): Promise<void> {
  const target = currentTarget();
  if (!target) return;
  const items = await commandItems(target);
  if (!items) return;
  openMenu(el.runMenu, [{ title: t("commands.title"), items }, editCommandsSection()]);
}

/** Notes that link to the open one. The panel under the preview lists them too. */
function openBacklinksMenu(): void {
  if (!state.note) return;
  openMenu(el.noteBacklinks, [
    {
      title: t("panel.backlinks.title"),
      items: state.note.backlinks.map((link, index) => ({
        label: link.title,
        detail: link.label ? `«${link.label}»` : undefined,
        glyph: "note",
        key: index < 9 ? String(index + 1) : undefined,
        run: () => void openNote(link.id),
      })),
    },
  ]);
}
