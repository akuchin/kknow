# What's next

This file lists the work in the code that is still open. To learn why the
project works the way it does, read [ARCHITECTURE.md](ARCHITECTURE.md). The
work for the public launch that is not code (signing, screenshots, posts) is
in [docs/launch-checklist.md](docs/launch-checklist.md).

These parts are complete:

- the core and the CLI (command-line interface);
- clear errors for a first start: refused S3 keys, no bucket, no answer, no
  access to a folder, a missing folder, git sign-in and address problems;
- the window, search, sync, themes and live preview;
- the UI (user interface) redesign;
- multiple vaults: the sidebar as a workspace, and opening a folder from Finder
  and from the command line;
- commands on a note, a folder and a vault, with a form for them in the
  settings;
- reading: the note panel (outline, backlinks, unlinked mentions), the hover
  preview and embeds. The plan is in
  [docs/reading-2026.md](docs/reading-2026.md);
- E2E (end-to-end) encryption of S3 and folder sync: names and contents, a
  passphrase and a recovery code. The plan is in
  [docs/encryption-2026.md](docs/encryption-2026.md).

The plan for the redesign is closed. Its archive copy is
[docs/archive/redesign-2026.md](docs/archive/redesign-2026.md).

## Before the launch

Do these tasks in this order. They come from Phase 1 of the launch
checklist: a new user must open the app with no errors.

1. **A real Obsidian vault as it is.** Open one and check links, embeds,
   attachments and the `.obsidian` folder. Write down what does not work, and
   fix what a new user meets first.
2. **A diff view: one component for two places.**
   - Conflicts: "Resolve" opens the copies in the palette now. Show the two
     versions side by side, with the "accept" and "delete copy" buttons next
     to them.
   - Git: the changes view shows the plain text of `git diff` now. Show the
     same comparison (side by side or inline, with word changes).

## Next, by area

There is no order inside this section. Take a task when its area is the
next one to work on.

### Sync

- **Encryption: what is still open.**
  - Rollback is not detected: the storage owner can give back an older
    manifest. A counter in the encrypted manifest and the last seen value in
    `.kknow/sync.json` would give a warning.
  - Encryption cannot be turned off, and there are no device keys: a lost
    device keeps the data key until encryption is turned on again in a new
    storage.
  - The status bar shows no lock for an encrypted vault.
  - `kknow sync --dry-run` does not show plain files that the next sync will
    encrypt (`prepare` writes, so the plan does not run it).

### Editor

- **Tables do not have all of Advanced Tables.** There are no formulas and
  no CSV export. You can copy a selection of cells, but you cannot paste a
  rectangle from the clipboard into cells.
- **The preview does not scroll with the editor** in "Side by side". The line
  markers in the preview (`data-line`) can do this.

### Commands

- **You cannot bind a command to a key.** Each user has a different set of
  commands, so a list in the code does not work. Add a field to the manifest.
  Check that the window does not already use the shortcut. The form has a place
  for this field: the "How it runs" section.
- **The command form does not show where a command appears.** The form asks for
  the scope (`match`, `requires`, vaults). To check a pattern, you must go to
  the tree. Add a line such as "matches N notes now". The core can calculate
  this already: `CommandSet::for_target` does the same calculation.
- **The app keeps only the last output.** The popup near the status bar shows
  the output of the last command. There is no run history. A restart clears
  the output.

### Agents

- The editor does not color the highlight of a comment by the state of its
  thread. Only the 💬 list shows the state.
- There is no inbox for all vaults: the questions of agents and the
  proposals show only for the open note. `kknow proposals` lists one vault.
- The window does not see a thread or a proposal that another process
  writes (for example `kknow mcp` in a terminal) until the note changes:
  the watcher skips `.kknow`. The HTTP server of the window has no such
  problem.
- A proposal shows edits one by one, not inside the editor text.

### Standalone files

- **Standalone files do not see the files near them.** `[[links]]` in the
  preview are broken. The file has no watcher of its own. If a different
  program changes the file, the window shows the change only when you return
  to it. Two changes are necessary:
  - a non-recursive watcher on the folder of the open file;
  - link resolution against the other `.md` files in that folder.
- A "Make it a vault" button on the file row, for a folder that needs it.

## Watch, do not do yet

- **The app never closes a vault.** Each open vault has its own in-memory
  index. Measured on 2026-10-24 (12 generated vaults of 500 notes, 3 MB on
  disk each, release build): about 10 MB per open vault, 130 MB for all 12,
  after the writer went down to one thread. A 5000-note vault opens in about
  0.55 s. This is not a problem yet, so vaults still do not close.
  Revisit if real vaults are much larger: memory grows with the note count.
  A vault with auto-sync must stay open for its scheduler, so closing needs
  more than a timer.

## Larger tasks

- **Import CSS themes from other Markdown editors** (for example, Obsidian).
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
- Arbitrary label colors.
- Tabs and editor splits.
- Dragging notes in the tree.
