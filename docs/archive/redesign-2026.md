# kknow redesign and multiple vaults: plan for stages 12–19

> **Archive.** All stages of this plan are complete. This document keeps a
> record of the intent. [ARCHITECTURE.md](../../ARCHITECTURE.md) has the
> resulting design and decisions. [TODO.md](../../TODO.md) has the next tasks.
> References to `PLAN.md` below point to the earlier document. The architecture
> came from that document.

This plan describes the next large step. It has three parts: a new look for the
main window, a separate settings window, and multiple vaults with independent
sync.

The plan starts from the [mockup](../mockup/kknow-final.html) (states `2a`,
`2a·2`, `2b`) and the spec in its last section. We do not copy the exact colors
and sizes from the mockup. The mockup sets the UX (user experience) and the
feature set. Our palette is our own, and it comes from the theme.

The work rules are in [CLAUDE.md](../../CLAUDE.md). This plan covers only the
parts that do not exist yet.

## The main changes

Now the app knows only one folder. The header holds all the controls: path,
sync, reload, theme, language. The sync settings are in a modal dialog. Errors
show as a text status in the header.

The target design:

1. **The header is only for content navigation.** On the left is the vault
   switcher. In the center are the name of the open note and its folder. On
   the right are search (⌘K), new note (⌘N) and the preview toggle (⌘E).
2. **The status bar at the bottom is for state and quick toggles.** It shows
   the active vault and the full file path, the word count and the reading
   time. It also has the sync status, theme, language and the settings button.
   You can click each item. No item opens a modal.
3. **The app has many vaults.** A vault is a folder with its own sync
   settings, a colored label and a ⌘1…⌘9 hotkey. The active vault controls the
   tree, search and the status bar. Sync runs for each vault independently. It
   does not stop editing in the other vaults.
4. **Settings are in a separate window (⌘,).** It has these sections: General,
   Appearance, Vaults, Keys, Editor. The main window keeps only the quick theme
   and language toggles.
5. **Errors and conflicts do not open modals.** The status bar shows them as a
   segment or a chip with an action.

### Where we differ from the mockup and follow the spec

- **The "iCloud" and "Dropbox" providers are a folder**, not two backends. Both
  services sync a usual folder. So the core gets one `FolderProvider`. In the
  UI (user interface), a "provider" is a preset path and name.
- **"Git" from the provider list waits for later.** Git is not a key-value byte
  store. It has commits, branches and its own conflict model. It needs a
  separate algorithm, not an implementation of the current trait. The list has
  no row for Git until an implementation exists. A choice that does not work is
  worse than no choice.
- **The UI calculates the word count and the reading time**, not the core. The
  number must change while you type. A call to Rust on each keystroke costs
  too much. In the terminal, `kknow stats` does the same calculation.
- **The right panel (preview and backlinks) stays.** The mockup does not show
  it because the panel is hidden. ⌘E already hides it, and the app remembers
  the choice.

## Decisions made before the work

This section follows the "Key decisions" section of PLAN.md. It records why we
do things this way. Then we do not discuss them again during the work.

- **The window and the terminal use one vault registry.** The list is in
  `<app config>/vaults.json`. Parsing and validation are in `kknow-app`, not in
  `src-tauri`. If not, `kknow --vault work` in the terminal and "Work" in the
  window come from two different lists. The core still does not know about
  Tauri. The file path comes as an argument. A function in `kknow-app`
  calculates the default folder of the platform.
- **Sync settings stay in the folder.** `.kknow/remote.json` does not move.
  Each vault has its own file. A move into the shared registry brings back the
  old bug: a second vault in the bucket of a different vault. The registry
  holds only the name, path, label and order.
- **Each vault has its own lock.** Now one mutex locks the full `Workspace`,
  and sync holds all of it. The lock moves into the vault slot. Sync divides
  into three parts: plan → transfer (no lock) → apply (lock only for writes).
  Without this change, the spec rule "sync does not block the UI" is not
  possible. A network sync can hold the vault for tens of seconds.
- **The app opens vaults lazily and keeps them open.** Each vault has its own
  in-memory search index. If the app opens all vaults at startup, startup is
  slower and uses more memory. The app opens a vault at first access: a switch,
  a sync, or a global search. After that, the vault stays open. We postpone
  closing on inactivity until the measurements show a need.
- **App settings move from `localStorage` to `settings.json`.** Now there are
  two windows, and they must agree. The second webview has a separate
  `localStorage`, and it does not report changes. Rust keeps the theme, the
  language and the editor behavior. A command changes them, and the
  `settings-changed` event sends them to the windows. `localStorage` keeps only
  the data of the window and the vault, with the vault id as the key:
  - collapsed folders;
  - recent notes;
  - the last open note.
- **Commands get the vault explicitly.** "Active" is a concept of the main
  window UI. The settings window edits and syncs a vault that is not always
  the active one. So commands get a `vault` parameter instead of an implied
  state.
- **The label color comes from a fixed set.** The label is next to UI text. An
  arbitrary `#hex` from the settings can become unreadable on a dark theme. The
  registry keeps the color name. The values are theme variables (`--label-*`),
  so a theme can override them. Extend the `theme::VARIABLES` list and add a
  test for it.
- **The app finds conflicts from files, not from a log.** A copy of the other
  version is a file in the vault (the `KeepBoth` policy). The app calculates the
  "conflict: 2 files" chip from the file names. So the chip stays correct after
  a restart, and no separate state can go out of sync. This follows the rule
  "files are the source of truth".
- **The app does not remove a missing folder from the registry.** For example,
  you disconnect an external drive, or iCloud did not download the folder yet.
  The app marks the vault as unavailable and keeps it in the list. If the app
  silently removes the entry, the user loses the setting for an outside reason.
- **A broken `vaults.json` does not stop the startup.** The app renames the
  file to `vaults.json.broken` and starts with an empty registry. The status bar
  shows a message. A silent empty list looks the same as a lost list.

## Stages

At the end of each stage, the project works. It builds, the tests and clippy
are clean, and you can see the behavior in `pnpm ui:mock`.

### Stage 12: header and status bar (UI only), complete

This stage adds only one Rust command: "reveal in Finder". The goal is to see
the new layout immediately, while the app has one vault. The result is in the
"Stage 12" section of PLAN.md.

What we did:

- `ui/src/styles.css`: the `#app` grid gets a third row for the status bar
  (`grid-template-rows: 40px 1fr 26px`). The layout with the hidden preview
  keeps this row.
- `ui/src/main.ts`: a new header. On the left is the vault switcher (label,
  name and ⌄, with one item for now). In the center are the note title and its
  folder in gray. On the right are "Search ⌘K", "New note" and the preview
  toggle. These items leave the header: `#pick-vault`, `#sync`, `#reload`,
  `#theme`, `#locale`.
- New `ui/src/menu.ts`: an anchored popup menu. A button opens it and sets
  `aria-expanded`. The arrows and Enter select an item. Esc and an outside click
  close it, and the focus goes back to the button. The vault switcher, the
  theme and language menus and the conflict chip use this menu. So we wrote it
  one time for all of them.
- New `ui/src/statusbar.ts`: segments as buttons. The segments are vault, file
  path, words and reading time, sync, theme, language and "Settings". For now,
  the sync segment shows the old state from ⌘⇧S. The text status (`setStatus`)
  also moves here as a separate segment. Its entry point stays the same. Only
  its place on the screen changes.
- The UI counts words in the editor text with a debounce of about 300 ms. The
  reading time uses 200 words per minute. The plural forms come from the
  current `Intl.PluralRules` in `i18n`.
- A click on the path does "reveal in Finder". The `reveal_note` command in
  `commands.rs` calls the system `open -R`. A plugin for one action needs an npm
  package and permissions. The core calculates the path from the `id`. In the
  mock, the click writes to the console.
- `ui/src/i18n.ts`: new keys for the header and the status bar. We removed the
  old header keys. `Dict` comes from the Russian dictionary, so the compiler
  does not let you forget the English one.
- For now, ⌘, opens the old sync dialog. Stage 17 adds the settings window.

How we checked:

- In `pnpm ui:mock`: the layout at 1280 and at 900 (the minimum window width).
  The word counter while you type. A click on each segment. Esc closes the menu
  and returns the focus. The palette works on top of the layout.
- `pnpm --filter kknow-ui build` (it includes `tsc --noEmit`).
- `cargo test --workspace` and clippy.

We also found and fixed an old bug. The closed palette (`display: flex` on a
`<dialog>` without `[open]`) stayed in the layout. It made the window
scrollable. The new status bar at the bottom made this bug visible immediately.

In a narrow window, the file path becomes shorter first (it has a large
`flex-shrink`). The message and the sync state stay readable. The tooltip
shows the full path.

### Stage 13: vault registry in the core, complete

The result is in the "Stage 13" section of PLAN.md. What we did:

- New module `crates/kknow-app/src/vaults.rs`:
  `VaultEntry { id, name, path, color, order }`, `VaultRegistry { entries,
  active }`. The operations are `load`, `save`, `add`, `remove`, `rename`,
  `set_color`, `reorder`, `find(name_or_path)`, `by_hotkey(1..=9)`.
- A function in the same module gives the path of the default settings folder
  (`macOS: ~/Library/Application Support/dev.kknow.desktop`, otherwise
  `XDG_CONFIG_HOME`). Tauri gives its own `app_config_dir()`, and the CLI
  (command-line interface) uses the function. Both point to the same file.
- The `add` operation canonicalizes paths. macOS uses `/private/var` instead of
  `/var`. Without canonical paths, one folder can appear in the list two times.
  The registry rejects duplicates and nested folders. If not, the watcher and
  sync process the same files two times.
- Migration: the app converts `last-vault.txt` into a registry with one vault
  and removes the file.
- CLI: `kknow vaults` (a list with the state), `kknow vaults add <path>
  [--name]`, `kknow vaults rm <name>`. The global `--vault` flag now also
  accepts a name from the registry if the value is not a folder.

How we check: the `kknow-app` tests cover these cases:

- a file round-trip;
- a duplicate by canonical path;
- a nested folder;
- a missing folder (marked, but in the list);
- broken JSON (JavaScript Object Notation): a backup and an empty registry;
- the `last-vault.txt` migration;
- a lookup by name and by path.

### Stage 14: several open vaults and search in all of them, complete

The result is in the "Stage 14" section of PLAN.md. What we did:

- `src-tauri/src/state.rs`: a map of slots replaces `Mutex<Option<Workspace>>`.
  Each slot is `VaultSlot { entry, workspace: Mutex<Workspace>, generation }`.
  The state also holds the registry and the active vault. The generation counter
  moves into the slot. Now each open vault has one watcher, and each watcher
  must stop independently.
- `src-tauri/src/live.rs`: the `vault-changed` event gets a `vault` field. The
  UI must tell an edit in the active vault from an edit in a different vault.
- Commands: `list_vaults`, `add_vault(path)`, `remove_vault(id)`,
  `rename_vault(id, name)`, `set_vault_color(id, color)`, `activate_vault(id)`.
  Note commands get an optional `vault` parameter (the default is the active
  vault).
- `ui/src/main.ts`: the vault switcher in the header starts to work. It shows a
  list with the label and the name, ⌘1…⌘9, "Add a vault…" and "Vault
  settings…". Before a switch, the UI saves a pending edit, as it does for a
  move to a different note.
- For each vault, the UI remembers the collapsed folders, the recent notes and
  the last open note (`kknow:folders:<id>` and others). ⌘2 must bring you back
  to the place where you stopped.
- Themes: the theme choice applies to the full app. The list has the built-in
  themes and the themes of the active vault. If the new vault does not have the
  selected theme, the app keeps the choice until you come back. If not, ⌘2
  silently resets the look.
- **Search in all vaults at the same time.** This moved here from stage 18
  because the customer asked for it. ⌘K searches in all vaults, and each hit
  knows its vault. A hit from a different vault shows that vault's label. When
  you open it, the app first switches the vault. There is no separate scope
  toggle. Nobody asked to search "only here", and one more chip in the palette
  can confuse users.

How we checked: tests on the mock runtime cover these cases:

- separate locks (a busy vault does not stop a different vault);
- lazy opening;
- a removed vault closes its slot;
- search finds results in both folders and opens a vault that nobody opened
  yet;
- ⌘1…⌘9 point to positions in the list.

In `pnpm ui:mock` we used three vaults, and one of them was unavailable. We
checked the switch through the menu and the keys. We checked a hit from a
different vault with its label. We checked the return to the last note of each
vault.

We also fixed a bug. The number in the palette footer showed "6 of 2". It
compared the hits from all vaults with the note count of the active vault.

### Stage 15: sync providers, settings format and folder, complete

The result is in the "Stage 15" section of PLAN.md. What we did:

- `RemoteConfig` moves from the `s3` feature into a shared module. It gets a
  tagged provider for S3 (Amazon Simple Storage Service) or a folder:
  `{"provider":{"kind":"s3",…}}` or `{"kind":"folder","path":"…"}`.
  The app reads the old format (S3 fields at the root) as `s3`. A dedicated
  test covers this, because users already have these files on their machines.
- New `crates/kknow-sync/src/folder.rs`: `FolderProvider`. It maps a key to a
  path. It writes through a temporary file and `rename`. Its `list` is
  recursive and skips its own `.tmp` files. Removing a missing file is a
  success, because the trait requires this.
- "iCloud Drive" and "Dropbox" in the UI are `FolderProvider` with a filled-in
  path. They have no code of their own.
- Only S3 keeps a keychain secret. The form has no key fields for the folder.
- CLI: `kknow sync --folder <path>` next to the current S3 flags.

How we checked: the `kknow-sync` tests cover these cases:

- two devices through `FolderProvider` (`tests/folder_devices.rs`: sync, a
  conflict with a copy, removal);
- reading the old settings format;
- a key with `..` stays in the folder;
- an iCloud placeholder gives a clear error.

In `pnpm ui:mock` we checked the form with a provider choice. The folder needs
three fields instead of seven. "Save" and "disconnect" reach the core.

We also fixed a bug. The settings form took its action from the `close` event
of `<dialog>`. Some webviews do not send this event. Now the action comes from
`submit`.

### Stage 16: sync does not block and reports its state, complete

The result is in the "Stage 16" section of PLAN.md. What we did:

- `kknow-sync`: `SyncEngine` can report progress (`Progress { done, total,
  current }`). It divides the work into plan, transfer and apply.
  `kknow-app::Workspace` takes the lock only for apply.
- New `src-tauri/src/sync_status.rs`: a state for each vault
  (`off | idle | syncing | conflict | error`) and a `sync-status` event
  `{vault, state, done, total, at}`. The state holds the error. No modal
  shows it.
- Auto-sync uses the interval from the vault settings. Each slot has a
  scheduler thread. If a sync already runs, the scheduler skips the tick.
  After a network error, the pause increases, and the app does not repeat the
  message. The app also syncs when you close the window.
- Conflicts: the app calculates the list of copies from the file names in the
  vault. For a different vault, the status bar shows a chip. "Resolve" opens
  the pair "my version ↔ copy" with the actions "accept copy" and "delete copy".
- The status bar and the popover show the provider and the time of the last
  sync. They also show a spinner and "3 of 8", as in the mockup.

How we checked: the tests cover these cases:

- progress increases and stops at `total`;
- the downloaded data goes to the disk only in the third part;
- the app saves a note while a sync is on the "network" (with two threads);
- an unavailable vault sets the state to `error`, and a recount does not clear
  it;
- the app counts conflicts from files;
- the pause rule (`Cadence`): the first tick is immediate, then the interval,
  then an increasing pause with a maximum.

In `pnpm ui:mock` we checked the "kknow · just now" segment with an amber
conflict dot. We also checked the "Archive 2024 · conflict: 2 files" chip with
a "Resolve" menu.

### Stage 17: settings window, complete

The result is in the "Stage 17" section of PLAN.md. What we did:

- Vite builds several pages: `ui/settings.html` and `ui/src/settings/main.ts`.
  Tauri opens a second window with the label `settings`. The
  `open_settings(section)` command, ⌘, and items in the popover open it. Esc
  and ⌘W close it.
- App settings move into the core: `crates/kknow-app/src/settings.rs`
  (`AppSettings { locale, theme, editor… }`) and `settings.json` next to
  `vaults.json`. The commands are `get_settings` and `set_settings`. Both
  windows listen to the `settings-changed` event.
- Sections:
  - **General**: the language, and what to open at startup.
  - **Appearance**: the theme, the themes of the active vault and their
    problems.
  - **Vaults**: a list with `+ / − / ⋯`, the name, the label color, the path
    and "Reveal in Finder". Also the provider, auto-sync, the interval, the
    conflict policy, the last sync and "Sync", and statistics.
  - **Keys**: a table of shortcuts, for reference only for now.
  - **Editor**: the preview by default, and the save and preview delays.
- Vault statistics: a new command on top of `Workspace` that gives notes,
  folders and size. `kknow stats` gives the same data.
- We removed the old sync dialog from the main window.
- A language change sends an event. Before, one window did
  `location.reload()`. Now both windows reload, and the app saves a pending
  edit before the reload.

How we check: the tests cover a settings round-trip. A broken `settings.json`
does not stop the startup. `set_settings` sends the event. `pnpm ui:mock`
opens `/settings.html` with stubs. We do not start the app window for checks.

### Stage 18: the rest of global search, complete

Stage 14 added search in all vaults. This stage adds the parts that matter only
for large folders:

- The palette shows "indexing…" while the app lazily opens a vault that nobody
  opened yet. Before, the first such search was only slower than usual.
- A limit on the number of hits for each vault. Then one large folder does not
  fill the full list.

### Stage 19: polish, accessibility, documentation, complete

This stage added keyboard support and `aria` in the menus and the status bar. It
added `prefers-reduced-motion` for the sync spinner and the settings toggles.
It also rewrote PLAN.md and README.md for the new design. The mockup is now in
`docs/mockup/kknow-final.html`. The link in this document no longer points to
the Downloads folder of a different person.

## Risks and problems

- **Memory and startup time.** Each open vault has an in-memory index. Measure
  with 5000 notes, as we did for one vault. If the numbers are bad, close
  inactive vaults. We postponed this decision, but we did not forget it.
- **Path canonicalization.** PLAN.md already lists it as a watcher problem.
  Now the duplicate check in the registry also depends on it.
- **The keychain with an unsigned build.** The keychain asks for permission for
  each vault address, so now it asks several times. This is normal in dev
  mode.
- **iCloud placeholders.** A file that iCloud did not download is an `*.icloud`
  file with metadata, not a note. `FolderProvider` must return a clear error.
- **Two windows and one state.** A command without an explicit `vault` acts on
  the wrong vault at some time. We need the parameter now, not later.
- **Tauri tests cannot check the second window.** The mock runtime does not
  create windows. Unit tests cover the commands, and `ui:mock` checks the
  window.
- **Core errors still do not use the dictionary.** They come from Rust as a
  string. The status bar shows them without changes, as before.

## What we do not do on purpose

- Git as a sync provider.
- Arbitrary label colors.
- Tabs and editor splits.
- Dragging notes in the tree.
- Encryption for sync.
- A persistent search index.
- CRDT (conflict-free replicated data type) merging.
- Mobile platforms.

The first two items wait because of priority. PLAN.md already lists the other
items.
