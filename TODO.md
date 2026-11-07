# What's next

This file lists the work that is still open. To learn why the project works the
way it does, read [ARCHITECTURE.md](ARCHITECTURE.md).

These parts are complete:

- the core and the CLI (command-line interface);
- the window, search, sync, themes and live preview;
- the UI (user interface) redesign;
- multiple vaults: the sidebar as a workspace, and opening a folder from Finder
  and from the command line;
- commands on a note, a folder and a vault, with a form for them in the
  settings.

The plan for the redesign is closed. Its archive copy is
[docs/archive/redesign-2026.md](docs/archive/redesign-2026.md).

## Next tasks

- **The app never closes a vault.** Each open vault has its own in-memory
  index. Measured on 2026-10-24 (12 generated vaults of 500 notes, 3 MB on
  disk each, release build): about 10 MB per open vault, 130 MB for all 12,
  after the writer went down to one thread. A 5000-note vault opens in about
  0.55 s. This is not a problem yet, so vaults still do not close.
  Revisit if real vaults are much larger: memory grows with the note count.
  A vault with auto-sync must stay open for its scheduler, so closing needs
  more than a timer.
- **Conflict resolution is only a list of files.** "Resolve" opens the copies
  in the palette. Add a side-by-side comparison of the two versions. Put the
  "accept" and "delete copy" buttons next to it.
- **Tables do not have all of Advanced Tables.** There are no formulas and
  no CSV export. You cannot paste a rectangle from the clipboard into cells.
  You can copy a selection, but you cannot paste one.
- **The app keeps only the last output.** The popup near the status bar shows
  the output of the last command. There is no run history. A restart clears
  the output.
- **You cannot bind a command to a key.** Each user has a different set of
  commands, so a list in the code does not work. Add a field to the manifest.
  Check that the window does not already use the shortcut. The form has a place
  for this field: the "How it runs" section.
- **The command form does not show where a command appears.** The form asks for
  the scope (`match`, `requires`, vaults). To check a pattern, you must go to
  the tree. Add a line such as "matches N notes now". The core can calculate
  this already: `CommandSet::for_target` does the same calculation.

- **You cannot go from one bookmark to the next.** The editor has no command
  for the next and the previous bookmark in a note. (Bookmarks sync now, with
  agent threads and proposals.) The line
  markers in the preview (`data-line`) have one more use. They can scroll
  the preview together with the editor in "Side by side".

- **Standalone files do not see the files near them.** `[[links]]` in the
  preview are broken. The file has no watcher of its own. If a different
  program changes the file, the window shows the change only when you return to
  it. Two changes are necessary:
  - a non-recursive watcher on the folder of the open file;
  - link resolution against the other `.md` files in that folder.

  Also add a "Make it a vault" button on the file row, for a folder that needs
  it.

- **Git: a better diff in the changes view.** The view shows the plain text
  of `git diff` now. A real diff view (side by side or inline, with word
  changes) must replace it.

- **Agent threads and proposals: what is still open.**
  - The editor does not color the highlight of a comment by the state of its
    thread. Only the 💬 list shows the state.
  - There is no inbox for all vaults: the questions of agents and the
    proposals show only for the open note. `kknow proposals` lists one vault.
  - The window does not see a thread or a proposal that another process
    writes (for example `kknow mcp` in a terminal) until the note changes:
    the watcher skips `.kknow`. The HTTP server of the window has no such
    problem.
  - A proposal shows edits one by one, not inside the editor text.

## Reading: the next features

These follow the link completion and the list keys. Do them in this order.

5. **Hover preview, then real embeds.** The plan is in
   [docs/reading-2026.md](docs/reading-2026.md).
   - ⌘ plus hover on a link shows a popup with the note, or with the section
     of `#Heading`. The core needs a render of one section:
     `Workspace::render_section(id, subpath)`, by the heading line from
     `link_target`.
   - Then `![[note]]` and `![[note#Heading]]` show the content in place, in
     the preview and in live preview. Limit the depth (3) and stop on a
     loop. An embed is read-only; a click opens the note.

## Larger tasks

These are the next large features, in this order:

1. **E2E (end-to-end) encryption for sync.** The notes must not be on S3 as
   plain text. The design has a place for it: a wrapper around the `get`/`put`
   of the provider.
2. **Import CSS themes from other Markdown editors** (for example, Obsidian).
   Such a theme is a CSS (Cascading Style Sheets) file. The app reads only the
   variable values from it and does not execute the other rules.

## Postponed on purpose

- General-purpose plugins through WASM (WebAssembly): `extism`/`wasmtime`.
- CRDT (conflict-free replicated data type) merging: `automerge`.
- Mobile platforms.
- A persistent search index.
- Full YAML (YAML Ain't Markup Language) parsing in frontmatter. The app reads
  scalar fields, and commands use them. It does not read nested mappings and
  lists.
- Real content embedding for `![[…]]`. Now the app shows it as a link.
- Arbitrary label colors.
- Tabs and editor splits.
- Dragging notes in the tree.
