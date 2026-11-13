# Next and previous bookmark in a note

## Goal

Let the user jump from one bookmark of the open note to the next or the previous one, from the palette or with a key. (Issue #22; TODO.md, section "Editor".)

## Current behavior

- Bookmarks live in the editor state as text positions (`ui/src/bookmarks.ts`). `bookmarkLines(state)` returns their 1-based lines, sorted and without duplicates.
- To reach a bookmark, the user opens the toolbar ribbon menu or the palette chip "Bookmarks" (⌘⌥B), and picks one by hand. There is no command for "the next one".
- `goToLine(view, line, at)` in `ui/src/bookmarks.ts` puts the cursor at the line start and scrolls the line into view (`Editor.goToLine`).
- Actions and their keys are listed once in `ui/src/actions.ts` (`DEFAULTS`). The window handler (`ui/src/window/keys.ts`), the palette action list (`ui/src/window/palette.ts`), the settings page labels (`ui/src/settings/keys.ts`) and the locales read from it.
- ⌘G / ⌘⇧G (next/previous search match), ⌘⌥←/→ (history), ⌘⌥B, ⌘⇧B, ⌘⌥P and ⌘⌥T are taken.
- `openBookmark` in `ui/src/window/marks.ts` shows how to go to a line in "Read" mode: it switches to "split" with `setMode("split", { remember: false })`.

## Required behavior

Two actions in the open note: "Next bookmark" and "Previous bookmark".

- Next: go to the first bookmark whose line is after the cursor line. If there is none, wrap to the first bookmark of the note.
- Previous: go to the last bookmark whose line is before the cursor line. If there is none, wrap to the last bookmark of the note.
- The cursor moves to the start of the bookmark line, and the line is scrolled into view.
- A note without bookmarks (also a standalone file or a locked note, which have none): nothing happens, no status message, no error.
- A note with one bookmark: both commands go to it (also when the cursor is on it, by wrap-around).
- Both commands are in the command palette and have default shortcuts.

## Requirements

- R1. A pure function `adjacentBookmark(lines: number[], cursorLine: number, direction: 1 | -1): number | null` returns the target line. `lines` is sorted ascending.
- R2. For direction `1` it returns the smallest line greater than `cursorLine`; if none, `lines[0]`.
- R3. For direction `-1` it returns the largest line smaller than `cursorLine`; if none, the last line.
- R4. With an empty `lines` it returns `null`.
- R5. A cursor on a bookmark line counts as "not after, not before" that bookmark: Next from the bookmark goes to the following one; with a single bookmark, wrap-around returns the same line.
- R6. `goToBookmark(view, direction)` (in `ui/src/bookmarks.ts`) reads `bookmarkLines(view.state)` and the line of the main selection head, calls `adjacentBookmark`, and moves the cursor with `goToLine`. With `null` it does not dispatch anything.
- R7. The cursor is at the line start and the line is in the visible area after the command (scroll with `y: "center"`, as `goToLine` does by default).
- R8. Two new actions in `DEFAULTS` (`ui/src/actions.ts`), placed after `history.forward`: `"bookmark.next": ["Mod-Alt-ArrowDown"]` and `"bookmark.previous": ["Mod-Alt-ArrowUp"]`. This is the final pair; the implementer does not choose another one. Checked against the repo and the CodeMirror default keymap: no other default action, no `RESERVED` key and no key in `ui/src` uses `Mod-Alt-ArrowUp/Down` (only `Mod-Alt-ArrowLeft/Right` for history, `Alt-ArrowUp/Down` for `item.up/down` and `Alt-Shift-Arrow*` for tables, which are different chords). CodeMirror binds `Alt-ArrowUp/Down` (move line) and `Shift-Alt-ArrowUp/Down` (copy line), not `Mod-Alt-ArrowUp/Down`. The existing conflict check in `actions.ts` and its test stay green. Only if the conflict test fails on the implementer's machine, report it as a blocker instead of picking another key.
- R9. The window key handler runs both actions (`ui/src/window/keys.ts`), and the editor keymap runs them when the editor has focus. In "Read" mode (no cursor) the command switches to "split" like `openBookmark` does, then uses the editor state. If the note has no bookmarks, "Read" mode does not change.
- R10. Both actions are in the palette command list (`ui/src/window/palette.ts`) with `action: "bookmark.next"` / `"bookmark.previous"`, so the palette shows the key.
- R11. Both actions can be rebound on the settings keys page: labels in `ui/src/settings/keys.ts`.
- R12. UI strings are in both `locales/en.json` and `locales/ru.json`: `palette.action.bookmarkNext`, `palette.action.bookmarkPrevious`, `settings.keys.action.bookmark.next`, `settings.keys.action.bookmark.previous` (the existing i18n test must pass: both files have the same keys).
- R13. The commands do nothing visible when bookmarks are not allowed for the note (standalone file, locked note) or the list is empty: no `setStatus`, no exception.

## Design

- `adjacentBookmark` is a pure function in `ui/src/bookmarks.ts`, so Node tests load it without the DOM (the module already imports only CodeMirror packages).
- `goToBookmark(view, direction)` sits next to `goToLine` in the same file; `Editor` gets a method `goToBookmark(direction)`, like `goToLine`.
- In `ui/src/window/marks.ts` add `goToAdjacentBookmark(direction)`: if the editor has no bookmarks, return; if `state.mode === "read"`, `setMode("split", { remember: false })`; then `editor.goToBookmark(direction)`. If the preview is open (`state.mode !== "edit"`), also `scrollToLine(el.preview, line)` as `goToHeading` does, so the preview follows. For this, `Editor.goToBookmark` returns the target line or `null`.
- Register the actions in `windowActions`, in the palette list, in `settings/keys.ts`, and in the editor keymap built from `actions.ts` (follow how `bookmark.toggle` is done there).
- Docs: `site/reference/shortcuts.md` (add both keys near ⌘⇧B), `README.md` bookmark section and shortcut table, and a short note in the bookmarks text of the site feature page that covers bookmarks (find with `grep -ril bookmark site/features`). Remove the bullet "You cannot go from one bookmark to the next" from `TODO.md`.
- No core (`crates/*`) change: bookmarks are already in the editor.

## Tests

- `ui/tests/bookmarks.test.ts` (Node, `pnpm ui:test`), for `adjacentBookmark`:
  - order: lines `[3, 10, 20]`, cursor on 1, 3, 5, 10, 15 → Next gives 3, 10, 10, 20, 20; Previous from 5, 10, 25 gives 3, 3, 20.
  - wrap-around: Next from 20 or 25 → 3; Previous from 3 or 1 → 20.
  - one bookmark `[7]`: both directions from 1, 7 and 9 → 7.
  - empty list → `null` both ways.
- Same file, with `EditorState`/`EditorView`-free logic only if the view is not available in Node; otherwise test `goToBookmark` through `bookmarkLines` on a state, or leave it to e2e.
- `ui/tests/actions.test.ts`: `bookmark.next` is `Mod-Alt-ArrowDown`, `bookmark.previous` is `Mod-Alt-ArrowUp`, and no other default action or reserved key shares them (extend an existing test or add one).
- `ui/e2e/note.spec.ts`: a smoke test on the mock: a note with two bookmarks; run "Next bookmark" from the palette, and the cursor line changes in order, then wraps; a note with no bookmarks: the command runs, the status bar shows no error.
- Run `pnpm ui:test`, `pnpm ui:e2e`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `pnpm docs:build`.

## Out of scope

- Moving between bookmarks of other notes or of the whole vault (the palette "Bookmarks" chip does this).
- Changes in the core, the CLI, the MCP tools or the format of `bookmarks.json`.
- A status message such as "bookmark 2 of 5".
- Bookmarks of standalone files and locked notes (they have none).
- The other TODO items in the "Editor" section.
