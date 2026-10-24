# kknow — architecture and design decisions

This document is for a new person on the project. It tells what the project is,
how it works, why it works this way, and where the traps are. For the user's
view of the app, read [README.md](README.md). For the next tasks, read
[TODO.md](TODO.md).

## What it is

kknow is a fast, local-first knowledge base for plain Markdown files. The
important logic is in Rust, not in the frontend. This is intentional.

The goals for the MVP (minimum viable product) are: speed; Markdown; sync
plugins (first, S3-compatible storage); full-text search; themes.

## Principles not to break

1. **Files are the source of truth.** A vault is a usual folder of `.md`
   files. Any editor can open them. The link graph, the search index and the
   sync state are a cache that kknow can build again. If you delete the cache,
   you lose no data.
2. **The core knows nothing about the UI (user interface).** No crate in
   `crates/` depends on Tauri. Thus the project has two equal frontends: the
   window and the CLI (command-line interface).
3. **Sync is a plugin behind a trait.** S3 (Amazon Simple Storage Service) is
   the first implementation of `SyncProvider`. It is not a function inside the
   core. The next backend must not need changes to the algorithm.

## Stack

| Task | Tool | Why |
|---|---|---|
| Shell | Tauri 2 | small binary, Rust core, system webview |
| Editor | CodeMirror 6 | mature, extensible, many Markdown editors use it |
| Markdown | `pulldown-cmark` | fast CommonMark, pull API |
| Search | `tantivy` | "Lucene in Rust", queries in milliseconds |
| File watching | `notify` + `notify-debouncer-full` | cross-platform |
| Frontend | TypeScript + Vite, no framework | thin UI, focus on Rust |
| Sync | `opendal` | one API for S3/MinIO/R2 |

The frontend is plain TypeScript on purpose. A reactivity layer would take
attention away from Rust, and Rust is the purpose of the project.

## Layout

```
crates/
  kknow-core    Vault, Note, NoteId — the vault model, reading and writing files
  kknow-index   markdown parsing, wiki links, tags, link graph, HTML, tantivy
  kknow-sync    manifests, sync plan, conflicts, configuration, S3, folder
  kknow-run     commands on targets: manifest, Runner trait, running with a timeout
  kknow-app     Workspace, vault registry, app settings, themes, watcher
  kknow-cli     the kknow binary — the same thing in the terminal
src-tauri/      commands on top of kknow-app, background watchers, keychain
ui/             the interface: TypeScript, Vite, CodeMirror 6; two windows — main and settings
testdata/vault  a small vault for manual checks
docs/mockup/    the redesign mockup the header, status bar and settings were built from
```

Dependencies go in one direction only: `core → {index, sync, run} → app →
{cli, src-tauri}`. `kknow-run` does not depend even on `core`. A module that
runs external programs must know nothing about note parsing. The caller gives
it a target that is ready to use.

A vault keeps its service data in a hidden folder inside the vault. The vault
does not treat this folder as notes. The files are:

- `.kknow/sync.json` — the snapshot of the last sync.
- `.kknow/remote.json` — the storage address and the conflict policy.
- `.kknow/bookmarks.json` — bookmarks.
- `.kknow/themes/*.json` — themes.
- `.kknow/trash/<entry>/…` — deleted notes and folders.

The window and the terminal share data in the app config directory
(`vaults::config_dir`):

- `vaults.json` — the vault registry.
- `settings.json` — app settings.
- `commands.json` — commands.
- `files.json` — standalone files that you opened without a vault.
- `themes/*.json` — shared themes.

### Data flow on edit

1. Editor → `save_note` command → `Workspace::save` → file write.
2. kknow updates the parse, the search index and the graph.
3. kknow replies to the UI with the new HTML and backlinks.

External edit:

1. Watcher → `VaultEvent` → `Workspace::apply`.
2. kknow sends the `vault-changed` event to the window.
3. The UI updates the list and, if necessary, the open note.

## Core: parsing, graph, files

- `NoteId` is a path relative to the root, without the extension. It refuses
  `..` and absolute paths.
- `Vault` walks the folder. It skips hidden folders, for example `.git` and the
  hidden folders of other tools. It reads files.

  It writes atomically through a
  temp file and `rename`. It updates (`refresh`) one file. It moves and copies a
  note to a new identifier, and it moves a folder. All of these operations
  refuse to overwrite a path that a file already uses. A case-insensitive disk
  does not confuse them: `Note` → `note` is the same file, not a different one.
- `Vault::remove` and `Vault::remove_folder` do not delete files. They move the
  item to the trash (`kknow_core::Trash`, `.kknow/trash`):
  - Each delete gets an entry folder. Its name is the delete time in Unix
    seconds, with `-2`, `-3`… for a second delete in the same second.
  - Inside the entry, the item keeps its path from the vault root. A restore
    moves each file back to that path.
  - The move is one `rename`: the trash is on the file system of the vault.
  - Each move to the trash first removes entries older than 30 days. This
    costs one directory read.
  - A restore refuses to overwrite a file. If one file of the entry is taken,
    nothing moves.
  - The watcher sees the old path disappear and reports `Removed`. It skips
    the new path, because the path is hidden. `Workspace::delete` already
    removed the note from the index, so the event changes nothing.
- Parsing: `[[link]]`, `[[link|alias]]`, `[[link#Heading]]`,
  `[[link#^block]]`, `![[embed]]`, Markdown links, tags, frontmatter.
- A link graph with wikilink resolution rules (full path, then file name, and
  so on) and backlinks.
- `Workspace::stats` gives notes, folders and size.

## Search

- `SearchIndex` uses tantivy. It indexes text, title, tags and path. It has a
  Russian stemmer. The syntax has `tag:rust`, `title:kknow`, `-word` and
  phrases in quotes.
- kknow also searches the last word of the query as a word prefix. Thus the
  list becomes shorter while you type. Each result has a snippet with
  highlights.
- The watcher sends edits from outside the app to the parse, the graph and the
  search. You do not need to do anything.
- `search_all` searches all vaults. Each hit knows its folder. A hit from a
  different vault shows the label of that vault. When you open such a hit, the
  window first switches the vault and then opens the note. `search_all` skips
  vaults that are not available. The hit limit (20) applies to each vault, not
  to the full response.

## Sync

- `SyncProvider` is a trait with four operations (`get`, `put`, `delete`,
  `list`). It is **synchronous**: async code never got into the core. Only the
  S3 implementation needs `tokio`, and the `s3` feature hides it.
- Implementations:
  - `MemoryProvider` — for algorithm tests.
  - `S3Provider` through `opendal` — S3, MinIO, R2. It has an endpoint, a
    region, a prefix in the bucket and access keys.
  - `FolderProvider` — a usual folder. It also covers iCloud and Dropbox.
- `Manifest` is a list of notes with content hashes. It has no modification
  times on purpose: computer clocks drift, and S3 sets its own upload time. One
  format describes three states: local, remote, and the snapshot of the last
  successful sync (`.kknow/sync.json`).
- kknow compares these three manifests and makes a `Plan` of actions:
  `Upload`, `Download`, `RemoveLocal`, `RemoveRemote`, `KeepBoth`.
- `ConflictPolicy` has three values:
  - `KeepBoth` — the default. kknow saves the other version as a copy next to
    the note. You lose nothing.
  - `PreferLocal`.
  - `PreferRemote`.

  kknow gets the conflict list from the copy files
  (`kknow_sync::is_conflict_copy`), not from a log.
- `RemoteConfig` is the configuration of the vault. It has a tagged provider
  (`{"provider":{"kind":"s3",…}}` or `{"kind":"folder","path":"…"}`), the
  conflict policy and the auto-sync interval. kknow still reads the old format,
  with the S3 fields at the root of the file. A dedicated test checks this. This
  struct has no field for the secret key, by design. `kknow_sync::connect` makes
  the provider from the configuration.
- `FolderProvider` uses the same keys as the bucket (`manifest.json`,
  `notes/*.md`). It writes through a temp file and `rename`. It skips its own
  `.kknow-tmp` files.

  If you delete a file that does not exist, the operation
  succeeds. A key from a foreign manifest cannot cause a write outside the
  folder. The provider refuses `..`, empty segments and absolute paths. iCloud
  can keep a file in the cloud only (`.name.icloud`). The provider lists such a
  file under its real name, and a read of it gives a clear error.
- The sync has three parts:
  1. `stage` makes the plan and reads the content to send. It holds the vault
     lock.
  2. `transfer` does the network work. It does **not** hold the vault.
  3. `commit_local` writes the files. It holds the lock again. `RemoveLocal`
     goes through `Vault::remove`, so the note goes to the trash.

  `run` joins the three parts for the terminal. Thus a note save does not wait
  until the sync ends.
- Progress is `Progress { done, total }` over the actions of the plan.
- `src-tauri/src/sync_status.rs` keeps the state of each vault
  (`off | idle | syncing | conflict | error`). It also keeps the time of the
  last successful sync, the conflict count and the address. It sends each
  change as a `sync-status` event.
- Auto-sync: the interval is in the vault configuration. Each vault has a
  scheduler thread. The thread reads the interval again each second. It skips a
  tick during a sync.

  After a failure, it makes the pause two times longer, up
  to eight times the interval. The `Cadence` type holds this rule, and a test
  checks it without a wait. Before the window closes, kknow syncs one last time
  each vault with auto-sync. It waits for a maximum of five seconds.
- The window keeps the secret key in the OS (operating system) keychain under
  the storage address. The terminal has no keychain. There the key comes from an
  environment variable (`KKNOW_S3_*`, `KKNOW_FOLDER`).

## Vaults

- `kknow-app::vaults` is the registry of the folders that kknow knows. Each
  entry has a stable identifier, a name, a path, a label colour and a position.
  The sync configuration does not go into the registry. It stays in the folder.
- kknow canonicalizes the path when you add a vault. The registry refuses
  duplicates and folders inside each other (`NestedVault`). If a folder
  disappears, it stays in the list with the mark "unavailable".

  If the file is
  broken, kknow renames it to `vaults.json.broken` and starts a new list. It
  returns the error to the caller, and the status bar shows it. kknow repairs a
  file that a person edited by hand: it drops repeated identifiers. If the
  active vault does not exist, kknow uses the first available vault.
- `AppState` holds the registry and a map of open vaults. Each vault has its
  own lock (`VaultSlot`). Thus a network sync in one vault does not delay a save
  in a different vault. Vaults open lazily: on a switch, a search or a sync.
  After that, they stay open with their parse and index.
- Each vault has one watcher. The `vault-changed` event has the vault
  identifier, and the window ignores the edits of other vaults. When you remove
  a vault from the list, kknow sets its `closed` flag, and its thread stops.
- Commands take an optional `vault`. Without it, they use the active vault.
- The window keeps memory for each vault (`kknow:folders:<id>` and similar
  keys). It keeps collapsed folders, recent notes and the last open note. ⌘2
  returns you
  to the place where you stopped.
- Four actions meet in one function, `adopt` (`src-tauri/src/lib.rs`):
  - `open -a kknow ~/notes`;
  - "Open With" in Finder;
  - a command-line argument;
  - a folder that you drag into the window.

  `adopt` adds the folder to the registry. If the folder is already in the
  list, `adopt` makes it active. macOS sends these actions as a
  `RunEvent::Opened` event. Thus the app starts through
  `build(…).run(|app, event| …)`, not through `run(…)`. The app also declares
  the `public.folder` document type in `src-tauri/Info.plist`
  (`LSHandlerRank = Alternate`: kknow does not take double-clicks on folders).

  kknow reads the registry and the settings **before** it builds the app. Read
  the pitfall about the order below. If kknow refuses a folder, it writes the
  reason to standard error.
- The window gets such a folder through the `vault-opened` event and switches
  to it. If the event comes before the window loads, the window does not get
  it. This is not a problem: on load the window opens the active vault anyway.
- The window listens for drag and drop on the webview
  (`getCurrentWebview().onDragDropEvent`), not through `listen`. The shell sends
  `tauri://drag-*` to one specific webview.

## Standalone files

A standalone file is a Markdown file that you open without a vault. Examples:
a `README.md` from a repository, a draft on the desktop.

- The list is `kknow-app::files::OpenFiles`, in the file `files.json` next to
  the vault registry. kknow canonicalizes the paths. The order is the order in
  which you opened the files. If you open a file again, its position does not
  change. If the file is broken, kknow renames it to `.broken`, as with the
  registry.
- **kknow forgets a missing file**, but it keeps a missing vault. A vault can
  disappear for reasons that we do not control, for example a disconnected
  disk. A loss of its sync configuration would be bad. If a standalone file
  disappears, almost always a person deleted or moved it by hand. A row that
  points to
  nothing only interferes. kknow forgets the file in these cases:
  - when it reads the list at startup (it writes the list again immediately);
  - on each `list_files`;
  - when an open fails;
  - when it compares the file with the disk on window focus.
- A file from a known vault is a note of that vault, not a standalone file
  (`VaultRegistry::locate`). If not, one note would have two rows in the window.
  The second row would have no links and no search. A file in the hidden
  folder of a vault (`.kknow`) is not a note for the vault. It stays a
  standalone file.
- The folder of the file does **not** become a vault. A double-click on
  `README.md` is not a reason to parse, index and sync all of the repository.
  Thus a standalone file has no graph, search or backlinks, because their
  scope is paths from the vault root.
- A standalone file gets commands: it is a note without a vault. It gets the
  commands for notes that have no `vaults` and no `match`. These two fields
  mean "only there", and a file is in no vault and has no root. Read
  "Commands on targets". `[[links]]` in the preview are
  broken. kknow has nothing to resolve them against. A guess from nearby files
  that kknow did not parse would be random.
- A standalone file has no watcher. kknow sees an external edit when it
  compares the file with the disk on window focus. kknow writes with a check
  (`save_if_unchanged`).

  If the file changed, the other version stays on disk.
  kknow saves our version as a copy `README (конфликт <hash>).md`, and the copy
  appears in the list. The write goes through a temporary `.kknow-tmp` and
  `rename`, and it copies the permissions of the old file. Without this, a file
  that only you can read would silently become readable by others.

  Someone can
  delete the file outside kknow while it has unsaved edits. Then the write
  restores
  the file. If the file has no unsaved edits, the window closes and
  forgets it.
- **The window can read and write only files from the list**
  (`AppState::known_file`). The path comes from the frontend. Without this
  check, `save_file` could write to any location. A file gets into the list only
  when a person selects it. Ways to select: a dialog, drag and drop, Finder, an
  argument. `reveal_note` uses the same method: the core computes or checks the
  path.
- In the window, a standalone file uses the same `NoteView`, with `file: true`
  and `id` as an absolute path. `saveNow`, the preview and the disk check use
  this field to select the `*_file` commands. The window keeps the last open
  file in `kknow:file`. An open of a vault note clears this key. Thus at startup
  the window reads it before `useVault`.
- A double-click in Finder comes as the same `RunEvent::Opened` as a folder.
  kknow puts the "open" request into `AppState::pending` and sends the window
  an empty `file-opened` event. The window takes the request with the
  `take_opened_file` command, on the event or on load, whichever comes first.

  On a cold start, the window would not get an event with the path in it.
  For a file, "open the active one" does not help, as it does for a vault. A
  file from a
  different vault makes that vault active in the core. Thus on a cold start the
  window opens that vault.
- Association with `.md`: `Info.plist` declares `net.daringfireball.markdown`
  with `LSHandlerRank = Default`. It also imports the type for older macOS.
  This puts kknow into "Open With". Only a person can make kknow the default
  app. Thus "Settings → General" has a button:
  - `src-tauri/src/association.rs` calls Launch Services
    (`LSSetDefaultRoleHandlerForContentType`) through FFI (foreign function
    interface).
  - This is the second `unsafe` in the project after `killpg`. A `CfString`
    type wraps it and releases the memory itself.
  - Apple marks these functions as deprecated, but they work. When they stop,
    only one file needs a change.
  - Launch Services does not know an app that is not in `/Applications`. It
    returns `-10814`, and kknow changes this code into a clear error.

## Themes

- A theme is a manifest in `.kknow/themes/<id>.json`: name, author, base
  (`light`/`dark`) and values of known variables. It is not CSS: a theme author
  sets colours and does not write styles.
- Parsing and validation are in `kknow-app::theme`. Thus they are the same for
  the window and the terminal. An unknown variable or an incorrect value does
  not break the theme. It becomes a warning. If kknow cannot parse a file, the
  file goes into the list of errors. It does not silently disappear.
- In the window, `data-theme` enables the built-in base, and the variables of
  the manifest go on top of it. The window keeps the selected theme between
  starts. If the theme disappears, the window uses the light theme. If the new
  vault does not have the theme, the window does not forget it. It waits until
  the theme is available again. Without this, ⌘2 would silently reset the look.
- Shared themes are in `<config_dir>/themes/`. You can see them from any vault
  and with no vault. Many folders have no themes of their own, and nobody will
  copy a favourite theme into each folder. A vault theme with the same name has
  priority over the shared one. Someone put it there on purpose, for example
  with changes for that folder. The `shadowsGlobal` flag tells the window that
  a shared copy already exists.
- kknow gives the path to the shared folder at startup (`AppState::use_themes`),
  as with the path to the commands file. The code does not find the path
  itself. Tests must not read the real folder of the person who runs them.
- Built-in themes (`gruvbox`, `gruvbox-light`) are manifests in
  `crates/kknow-app/themes/`, and `include_str!` puts them into the binary. The
  app comes from a release or Homebrew, and a new user has no theme files.
  Files in the app bundle or a copy into the shared folder at first start would
  be worse: the first gives a file that nobody can edit, the second writes into
  the user's folder without asking. A built-in theme goes through the same
  checks as a file. The order is built-in, then shared, then vault, and a later
  theme with the same `id` wins. Thus a user can change a built-in theme: put a
  file with its `id` into the shared folder. `shadowsGlobal` stays true only
  over a shared copy: a built-in theme has no file to update.
- "Make shared" (`theme::share`) copies the file byte for byte. It does not
  build the manifest again. Thus the copy keeps the variables that this version
  does not know yet. The `id` comes from the window. Thus the core refuses any
  value that looks like a path and not like a file name.

## Commands on targets

A command is your own program that you attach to a note, a folder or a vault.
Examples: publish a page to Confluence, run a linter, make an archive.

- A command is an entry in `commands.json`. It has `id`, `name`, `kind` (the
  program that runs it), `script` and an optional scope. The scope has these
  fields:
  - `targets` (`note`, `folder`, `vault`);
  - `vaults` (identifiers from the registry);
  - `match` (path patterns from the vault root);
  - `requires` (frontmatter fields).

  An empty condition does not limit anything. Thus a manifest with four fields
  works everywhere: in all vaults and on standalone files.
- A standalone file is a `note` target, not a fourth kind (`Target::file`). A
  separate kind would hide all existing commands from files until you mark
  each one. The vault ID is empty, the command starts in the folder of the
  file, and the script gets no `KKNOW_VAULT*` and no `KKNOW_NOTE`. With
  `KKNOW_VAULT`, `kknow list` in the script would take the folder as a vault.
  The window sends the path, and the core accepts only a file from the list of
  open files, as for `save_file`. In the terminal: `kknow run <id> --file
  README.md`. It needs no vault.
- There are more fields:
  - `cwd` — `vault` by default, or `target`;
  - `timeoutSeconds` — 60;
  - `confirm` — ask before the run (the window does this);
  - `login` — read the shell profile;
  - `env`;
  - `program` — a program to use instead of the default, for example python
    from a virtual environment.

  The file also has its own `env`. All commands get it, and the `env` of a
  command goes on top of it.
- `kknow_run::Runner` is a trait with three methods: the kind name, the default
  program and the arguments around the script. Implementations: `shell`,
  `bash`, `fish`, `zsh`, `python`. `shell` is `sh`, not the shell of the user
  who
  started kknow. The design is the same as for `SyncProvider`: the manifest
  selects the kind at runtime, not the build.
- **The path comes as an environment variable. kknow does not put it into the
  script text.** There are seven variables:
  - `KKNOW_PATH` — the absolute path of the target;
  - `KKNOW_TARGET` — the kind;
  - `KKNOW_RELATIVE` — the path from the root;
  - `KKNOW_NOTE` — the note identifier, only for notes;
  - `KKNOW_VAULT`;
  - `KKNOW_VAULT_ID`;
  - `KKNOW_VAULT_NAME`.
- `KKNOW_VAULT` is on purpose the same variable that `--vault` reads in the
  terminal. Thus `kknow list` in a script uses the folder for which you ran the
  command.
- **Parameters for each file come from the metadata of the note.** Scalar
  values go into the environment as `KKNOW_FM_<KEY>`. The line
  `confluence: 123456789` gives `$KKNOW_FM_CONFLUENCE`. Thus the command
  `wiki-publish replace $KKNOW_FM_CONFLUENCE "$KKNOW_PATH"` gets the page
  number as a separate argument. kknow changes the key into a valid variable
  name (`confluence-id` → `KKNOW_FM_CONFLUENCE_ID`). It skips a key that it
  cannot change into a valid name.
- **There are two metadata sources: frontmatter and a comment**
  `<!-- confluence: 987654321 -->`. The comment is necessary where people
  outside kknow can see the frontmatter. GitHub shows frontmatter as a block in
  the `readme.md` of a repository. A publish would copy it onto the page.
  `parse` collects both sources and gives them as `ParsedNote::fields`. The
  consumer does not need to know the source of each field.
- **The editor shows `<!-- confluence: … -->` as a chip** with an icon and
  the page number (`ui/src/confluence.ts`). A click on the chip opens a form
  where you change the number or remove the comment. The editor context menu
  has the same form: "Связать с Confluence" adds the comment on the first line
  of the note, after the frontmatter if there is one. The form also takes a
  page link and keeps only the number (`/pages/123/` or `pageId=123`). The
  chip shows only for a one-line comment with this one key, and only in the
  main editor: a table cell is not a file.
- If two values have the same key, one rule applies to both sources: **the
  first value wins**. kknow parses the frontmatter before the body, so the
  frontmatter wins over a comment. Of two comments with the same key, the
  higher one wins.
- `requires: ["confluence"]` makes this a condition: the item appears only on
  notes where the field has a value. Thus the link "command ↔ specific file" is
  in the file itself, not in the shared manifest. You do not need a separate
  "note → page number" registry.
- `take_field` in `kknow-index` does the parsing. It reads flat `key: value`
  lines and removes quotes. It skips nested mappings and lists line by line. It
  is not a YAML library: notes have only flat fields.
- kknow takes comments from raw HTML events and does not scan the text. In a
  code block the parser gives text, not HTML. Thus `<!-- … -->` in an example
  does not become metadata. kknow parses the comments after the walk, from a
  buffer that it collects in order. The parser gives a multi-line comment as
  several events, one for each line. `<!--` and `-->` are in different events.
- **A window that you open from Finder gets `PATH` from launchd, not from the
  shell.** Measurements on this computer:
  - `launchctl getenv PATH` is empty. Thus the system
    `/usr/bin:/bin:/usr/sbin:/sbin` applies, without `/opt/homebrew/bin` and
    `~/.local/bin`.
  - `sh -lc` gets homebrew, but not `~/.local/bin`. `config.fish` sets that
    one.
  - `fish -c` with the same short `PATH` gives the full user environment. fish
    reads its config and universal variables at each start, not only at login.

  This is the reason for `kind: "fish"` in the template and for the `login`
  flag for `sh`, `bash` and `zsh`.
- **kknow searches for the interpreter not only on `PATH`.** Without this,
  `kind: "fish"` would be a trap. `/opt/homebrew/bin/fish` is not on the system
  `PATH`. The advice "use fish" would work in `pnpm dev`, where `PATH` comes
  from
  the terminal. It would fail in the installed app.
  - `locate` checks `PATH`, then `/opt/homebrew/bin`, `/usr/local/bin`,
    `/opt/local/bin`, `/usr/bin`, `/bin`.
  - kknow expands `~/` in `program`. A shell would expand it, but kknow runs the
    program without a shell.
  - If kknow does not find the program, it gives its own error with the program
    and the reason. It does not give a bare "No such file or directory".
- kknow builds the process environment in three layers. First the shared `env`
  of the file, then `KKNOW_*`, then the `env` of the command. The shared block
  sets
  `PATH` and tokens one time for all commands. It cannot overwrite the computed
  `KKNOW_*`. The `env` of a command is an intentional change to one command, and
  it can change anything.
- `kknow_app::commands` finds the target by name. `Workspace` finds a note, in
  the same way as `kknow show`. For a folder, kknow checks for `..`, an absolute
  path, and an exit from the root through the canonicalized path. The frontend
  and the terminal give only the name. The core computes the path.
- `match` patterns know `*`, `?` and `**` (`glob.rs`, our own matcher of about
  fifty lines). kknow compares them with the **file** path, not with the note
  identifier. If not, `**/*.md` would match nothing.
- Separate threads read the output. They keep the first 64 KB of each stream.
  A read after the process ends does not work: the process pipe is small. A
  command that prints more than the pipe holds would wait forever.
- On timeout kknow kills the **process group**, not only the process. `sh`
  almost
  always runs the real work as a separate process. This is why `kknow-run` has
  `libc` and one `unsafe` for `killpg`.
- An incorrect entry in the file becomes an error message. It does not silently
  disappear. Examples: a typo in `kind`, an empty script, a repeated `id`. The
  list of errors goes to the status bar and to `kknow commands`.
- In the window:
  - "⋯" and a right-click on a tree row give the commands for a note and a
    folder.
  - For a vault, the "Commands…" item gives them.
  - The palette (⌘K) also has the commands of the open note.
  - The menu of a standalone file also has its commands.
  - The same menu has file operations above the commands. They are: rename,
    move, duplicate, copy the path and the `[[link]]`, open in the default app,
    reveal in Finder. Delete is the last item.
  - Rename is a field in the tree, as for a vault.
  - Move shows a second list of folders, not a submenu. Our menu cannot show
    submenus.
  - The section order is the same as in file managers: create, rename,
    clipboard and system, custom commands, delete.
  - The custom commands section has the title "Commands", without the name of
    the note or folder. The menu opens from that row or from the header, so
    the name only made the line long.
  - Each frequent item has a key of its own, as in macOS context menus:
    R rename, M move, D duplicate, C copy path, L copy link, O open, S reveal
    in Finder, E edit commands, ⌫ delete. The first nine custom commands get
    digits. The menu matches `event.code`, not `event.key`, so the keys work
    with the Russian layout on too.
  - The result goes to the status bar. The output goes to a small popup next to
    it. The popup has a summary with the exit code and the time, and both
    streams as text.

  In the terminal: `kknow commands` and `kknow run`.
- You edit commands with a form in the settings. "Configure commands…" opens
  the "Commands" section. You do not edit them in a text editor, as you do
  with themes. The reason: the form knows things about a command that a text
  editor does not know. It knows the available run kinds, the possible targets
  and the environment that the command gets. It also shows the errors from the
  core immediately, not after the first failed run.
- The form edits the **manifest** (`kknow_run::Manifest`), not the set
  (`CommandSet`). The set drops incorrect entries. An edit through it would
  remove from the file all entries with core errors. A typo in the run kind is
  the most frequent error. You must fix it where you can see it. Thus an
  unknown kind shows as the first item of the list, not as an empty field.
- The form refuses to edit the file in one case only: a missing bracket in the
  JSON. kknow could not read the file, and a save would erase its content. In
  this case the form shows the error and a Finder button. The file is still
  there after the form appeared. You can read it, copy it with the
  settings to a different computer, or fix it by hand.
- The form writes the file after each edit, fully, without a "save" button,
  like the other settings. It saves an unfinished command (an empty script,
  cleared `targets`) like any other command. Such a command is a reason for an
  error message, not a reason to stop the window from closing.

## Auto-update

- The model is Zed, not Sublime. The app checks and downloads a new version
  without a dialog. The status bar then shows **Restart to update**. The user
  selects the time of the restart, so an update never stops typing. Sublime
  asks in a dialog each time, and the window has a rule: no modals for state.
- The work is `tauri-plugin-updater`, in `src-tauri/src/updater.rs`. All of
  it runs in Rust, as sync does. The windows use three commands
  (`update_status`, `check_update`, `restart_to_update`) and the
  `update-status` event. Thus the mock replaces three commands, and the
  windows need no plugin permissions.
- The setting `autoUpdate` is in the core settings. The core does not know how
  to update: that needs Tauri. The setting controls only the background
  checks. **Check now** and the menu item work also when it is off.
- **A developer build is version 0.0.0.** `Cargo.toml` keeps `0.0.0`, and only
  the release workflow writes the tag version. A check by `debug_assertions`
  alone is not enough. `pnpm install:app` makes a release build, and it would
  see each published version as an update.
- The scheduler is a thread that looks at the wall clock each 10 minutes. A
  sleep of 6 hours would not work: the monotonic clock stops while the laptop
  sleeps. A laptop that sleeps each night would almost never check.
- The status bar shows only a downloaded version. A failed check goes only to
  the settings. A laptop without network would otherwise show an error each 6
  hours.
- The restart comes from the main window. It saves the open note first, then
  the core runs the last sync, as on close. The settings window has no restart
  button, because it cannot save the note of the main window.
- If the user quits without the button, `RunEvent::Exit` installs the
  downloaded version. Windows is the exception: there the update is an
  installer with its own window. A window after "Quit" is a surprise.
- The release is a draft until all platforms are built. The apps read
  `latest.json` from the latest published release. A published release without
  that file gives the apps a 404.
- The update archives are on only in CI, through `tauri.release.conf.json`. A
  local `pnpm build:app` has no private key, and with the archives on, the
  build fails.
- `requireSignedVersion` is off for now. It also checks that the signed
  version is the announced version, and it stops a downgrade attack. Turn it
  on when all releases that users can update from carry the signed version.
- The Homebrew cask has `auto_updates true`, so `brew upgrade` does not fight
  the app over the version. Add it only after a release with the updater is
  out. Before that, `brew upgrade` is the only way for an old app to get the
  updater.

## Window

### Layout

- The window has three strips. The header is for navigation in the content.
  The footer is for the state. The work area is between them (`#app`, the third
  grid row is `status`).
- The header is about the open item.
  - On the left, with the width of the sidebar: the sidebar button (⌘\\) and
    history (⌘⌥← / ⌘⌥→). Thus the note name is above the editor, not above the
    tree.
  - In the centre: the file name (without `.md`) and the location of the note
    (the vault label, vault / folder). The tooltip shows the full path. It is
    the file name, not the title: the title is the first line of the text
    anyway, and "Rename" in the same menu changes the file name.
  - On the right: 💬, the note toolbar, the mode and search (⌘K).
- The note name is a button. Its menu is the same as the menu of a tree row
  (`openCommandMenu`). For a standalone file, it is the menu of the file row.

  The header has no menu of its own on purpose. Two lists of actions for the
  same note would become different with the first new command. "Rename" from
  the header expands a collapsed sidebar, because the field appears in the tree
  row.
- The note toolbar (`#note-tools`, `renderNoteTools`) has bookmarks, run and
  backlinks. A toolbar button gets an action by one of these rules: the action
  is frequent, or it has a state that you must see, or both. Rare and
  dangerous actions (rename, move, delete) stay in the note menu only.
  - A button does not replace a menu item. The run list is `commandItems`, the
    same list as in the note menu, so the two cannot become different.
  - Bookmarks belong to lines, not to the note. Thus the button is not a
    toggle: its menu lists the bookmarks of the note. The filled ribbon shows
    that the note has bookmarks. The toggle in the menu works on the cursor
    line, as ⌘⇧B does.
  - ▶ repeats the last command (`kknow:command:last`), from any menu. Usually
    you run one command many times, for example a publish. If the open note
    does not have that command, ▶ opens the list.
  - Backlinks are also in the panel under the preview. The button is for the
    "Edit" mode, which hides the preview. With zero links the button stays
    disabled and does not disappear: a toolbar that changes its width from
    note to note moves the buttons away from the cursor.
  - A standalone file has no bookmarks and no backlinks, so only ▶ is visible.
- The sidebar header has the tree toolbar: new note, new folder, collapse all
  vaults, add a vault. "New note" moved here from the header: the field for the
  name is in the sidebar. A click on it or ⌘N expands a collapsed sidebar.
- History keeps places, not notes: `{ vault, id }` or the path of a standalone
  file. "Back" can go to a different vault. While the window itself moves
  through history (`travelling`), it does not record opens. If a place does not
  open (someone deleted or renamed the note), the window marks it with
  strikethrough. The
  keys are ⌘⌥←/→, a frequent convention. In CodeMirror ⌘[ and ⌘] change the
  indentation.
- Status bar (`ui/src/statusbar.ts`):
  - On the left are the vault items. First the vault name with a menu, then
    sync, then the message. Sync is a property of the vault, not of the
    window.
  - On the right are the note and window items. They are the command result,
    a chip about a problem in a different vault, words, zoom, the line numbers
    toggle and the gear. Zoom
    shows
    only when it is not 100%, and a click resets it. The gear has theme,
    language and "Settings…" in one menu.
  - The full path is not there now. It was the longest segment, and in a
    usual window the bar cut it until it was useless. "Reveal in Finder" and
    "Copy path" are in the note menu.
  - A click on sync starts a sync now.
  - The line numbers toggle changes the `lineNumbers` setting, not the editor.
    The settings window has the same switch. Both windows get the new value
    from the `settings-changed` event, so the two switches always agree.
  - A different vault with a problem gets a chip, for example
    "Archive 2024 · conflict: 2 files". The chip has a "Resolve" and "Sync"
    menu.
- The window has one menu module (`ui/src/menu.ts`). It serves the vault
  switcher, the gear menu, the note menu, the tree-row menus and the editor
  context menu. Arrows, Home/End, Esc and Tab work in it. Esc and Tab close
  it, and the focus returns to the button. A click outside and a scroll also
  close it. When you open a second menu, the first one closes.
  - The editor context menu (`ui/src/editor-menu.ts`) opens with a right click
    in the text, at the pointer (`openMenuAt`). It has no button, so the focus
    goes back to the text. It has the comment, the bookmark, and cut, copy and
    paste. Each item shows its shortcut, as in the sidebar menus.
  - A right click outside the selection moves the cursor there first, as in
    other editors. The menu is about the place under the pointer.
  - Clipboard items use `navigator.clipboard`, not `execCommand`. The menu
    takes the focus from the text, and `execCommand` needs the focus there.
  - The note editor of a table cell has the same menu, without the bookmark.
    The cell stays open while the menu has the focus. A closed cell is a
    widget: its selection belongs to the browser, so it gets the browser
    menu.
- There are three note modes (`data-mode` on `#app`):
  - "Live preview" — markup renders in the text, no preview pane.
  - "Side by side" — plain Markdown and the preview.
  - "Read" — only the preview at full width, as a column in the centre.

  This is not a pane toggle. The window never shows the same content two times
  side by side. The full source is always available in some mode.

  ⌘E goes
  to the next mode. In "Edit", the window does not compute the preview again.
  That would need a call to the core on each edit. A sync result shows the
  preview again, but the window does not keep this for the note.
- "Read" shows the same `#preview` as "Side by side", not its own container.
  Thus note zoom works there with no extra work: `applyScale` sets it on the
  preview as a percentage. `.preview-pane` holds the base size of "Read", and
  the window multiplies it by the zoom percentage.
- The window keeps the mode for each note (`ui/src/note-mode.ts`, like zoom:
  `localStorage`, key `positionKey`, it moves on rename). The window always
  writes a manual selection, also when it is equal to the default. Thus it does
  not change when the default changes. The window does not write a mode that it
  got on open.

  The default is `viewMode` in the core settings. An old file
  before modes knew only `previewVisible`. On read, a disabled preview becomes
  "Edit".
- The sidebar collapses (⌘\\, `data-sidebar`). The column stays in the grid,
  but `--sidebar` becomes zero. The header uses the same variable for its
  alignment. The window keeps this selection in `localStorage`.
- Command output has its own popup next to the status bar (`#run-float`). It
  is not a panel in the right wing. A command also runs when the preview is
  closed. It is wrong to replace the open note with a `git status` report.

  A
  successful command shows as a line in the status bar and as a separate
  segment with its name. A click on the segment opens the output. Esc and the
  cross close it. A failure opens the popup automatically.
- `reveal_note` in `src-tauri`: the core computes the path from the note
  identifier, and the frontend never has it. Without an `id`, the command
  reveals the folder itself.

### Sidebar — workspace

- The tree roots are vaults, not the folders of one vault. You can see all
  vaults at the same time, like workspace folders in VS Code. The window reads
  the tree of a vault when you expand it (`toggleVault` → `list_notes`). During
  parsing and indexing, the branch shows the row "reading the folder…".
- Inside is a folder tree. `buildTree` in `ui/src/main.ts` builds it from note
  paths. The core needs no separate command, because `NoteId` is the path.
  The indentation of the nested `<ul>` shows the nesting, so the code never
  computes the depth. A note keeps the arrow slot empty. If not, its name would
  be to the left of the name of its folder.
- The window cuts a long name with an ellipsis and does not wrap it. In a tree,
  people read the row order and the indentation. The hover tooltip shows the
  full name. For a note, it has the full path.
- The order uses `Intl.Collator` for the window language: folders before notes,
  in alphabetical order in each group.
- The window keeps the collapsed folders for each vault (`kknow:folders:<id>`)
  and the expanded vaults in `kknow:vaults:open`. By default all folders in the
  tree are open.
- You can open a note from a link, a search or a creation, not from the tree.
  Then `reveal` shows it: the folders above it expand.
- A vault row has two buttons: the row button and "⋯". Its menu (also on a
  right-click) has: make active, rename, reveal in Finder, settings, label
  colour, remove from the list.
  - Rename is a field in the tree. ↵ saves, Esc cancels, and a mouse move away
    saves.
  - The field is not inside the row button, because a field inside a button
    does not get clicks. The window hides the button during the edit.
  - One field (`renameInTree`) serves three kinds of rows. For a folder and a
    note it holds the file name, not the title. The title is in the text.
- For the core, rename and move of a note are one operation: `Vault::rename`
  to a new identifier. The identifier is the path. For a folder, the operation
  is `rename_folder`. It returns "old — new" pairs, and `Workspace` moves them
  in
  the search and the graph.

  kknow does not change links in other notes. The
  resolver finds the note by file name and by path suffix after the move. If it
  does not, the graph shows a broken link. This is more honest than a silent
  edit of a different note. A duplicate goes next to the original: `Name copy`,
  `Name copy 2`, the first free name.
- You can see edits in a different vault. `onExternalChange` reads again the
  tree of the vault that sent the event.
- There are three ways to add a vault, and all of them meet in `adoptVaults`:
  - the folder button in the sidebar header;
  - "Add a vault…" in the status bar menu;
  - a folder that you drag into the window.

  You can drop several folders at the same time. An incorrect one (a file, a
  duplicate, a nested folder) does not cancel the others.

### Search palette

- The search across the vault is a separate window (`ui/src/palette.ts`), not a
  field in the sidebar. The tree stays in its place. The search comes on top
  and closes on Esc with no changes.
- The window is a native `<dialog>` with `showModal`. It gives Esc, the
  backdrop, the focus trap, and the return of the focus to its earlier place.
  The palette is in `document.body`, not in `#app`. While it is open, it
  blurs the window behind it (`body[data-palette="open"] #app`). A blur on a
  parent would also blur the palette.
- Two keys open the palette: ⌘K and ⌘⇧P. Plain ⌘P stays for printing.
- The palette has no state of its own. Notes, actions and the open history are
  in `main.ts`. They go to the palette through `PaletteHost`.
- Chips in the header limit the query to an index field: `title:(…)`,
  `tag:(…)`. The chip resets on close. If not, the next search would silently
  use a field that you selected some time ago.
- Search responses have numbers (`issued`). A short query is faster than a
  long one and can return first. Thus the palette shows only the response to
  the most recent query.
- The mouse and the keyboard share the row highlight. `onmousemove` moves the
  same index as the arrows. If not, ↵ would open a different row from the one
  that you look at.
- An empty query shows recently opened notes (`kknow:recent`). The start of the
  vault fills the rest of the list. "Recent" here means "opened in the window",
  because the core does not give modification times.
- If the search finds nothing, the palette offers to create a note with that
  name. It does this only for plain text: `title:(` would give a file name with
  a colon and a bracket.
- While a vault that you did not visit yet opens lazily, the palette tells what
  it does. The message appears after 250 ms. Thus a fast response does not show
  it.
- The number in the footer depends on the content. For recent notes it is
  about the vault ("8 of 9"). For hits, it is about the hits.

### Settings window

- Settings are a separate window (⌘,), not a panel: `ui/settings.html` +
  `ui/src/settings/`. The Vite build has several pages. The `open_settings`
  command opens it, and the section comes as an anchor in the URL. The command
  does not open a second window: it brings the open window to the front.
- On macOS the app menu (kknow → Settings…) opens the same window
  (`src-tauri/src/menu.rs`). The menu is the system default with one more
  item, so Edit (copy, paste) and Quit stay as they were. The menu takes ⌘,
  before the webview, so the key also works in the settings window. The item
  text follows the UI language: `set_settings` changes it.
- Sections:
  - General — language, whether to open the last note.
  - Appearance — theme, errors in the themes of the active vault, and what
    the note tree shows. `noteLabel` in `settings.json` is `title` (the
    first-level heading, else the file name) or `file` (the file name without
    `.md`). The tree sorts by file name in both cases. The palette and the
    links keep the title, and the header always shows the file name.
  - Vaults.
  - Commands.
  - Keys — reference.
  - Editor — default note mode, line numbers.
- The "Vaults" section has a list on the left with the label, the sync state
  and a dot. On the right are:
  - the name (you can edit it in place);
  - the path and "Reveal";
  - label colour;
  - provider, S3 fields or folder;
  - auto-sync;
  - conflict policy;
  - "Sync" with the time of the last sync;
  - statistics (notes, folders, size, hotkey).
- The "Commands" section has the same structure. The list on the left has a dot
  on the commands with core errors. On the right are:
  - label, identifier, kind and program;
  - the script, with a collapsed reference "What the script gets in its
    environment";
  - scope, run conditions and variables (the variables of the command and the
    shared variables of the file).

  The window computes the reference from the command itself. `$KKNOW_NOTE`
  appears only where the command is for notes. "Required note fields" become
  the related `$KKNOW_FM_…`.
- App-level settings are in `kknow-app::settings` and `settings.json`. The
  commands are `get_settings`/`set_settings`, and the event is
  `settings-changed`. A copy of the language stays in `localStorage`, because
  `i18n` reads it synchronously on load.
- Both windows get the permissions (`capabilities/default.json`). Tauri gives
  permissions by window label. Without this, the settings window would not get
  even the folder picker.

### Styling and labels

- The icons are our own (`ui/src/icons.ts`): about fifteen paths on a 24×24
  grid, `currentColor` and `fill: none`. The module exports two functions:
  - `iconMarkup` — for a template that the code builds as a string;
  - `icon` — for DOM (Document Object Model) that the code builds by hand. For
    example: the table toolbar, the diagram button.
- Header buttons have an icon and a label. Some places have no space for a
  label: the table toolbar, the "+" for a new note, the crosses. There only the
  icon stays, and `aria-label` replaces the label.
- A button has four states, and you can see all of them:
  - hover — background and border;
  - pressed — accent background;
  - keyboard focus — a ring (`:focus-visible` with one rule for all the window);
  - on (`aria-pressed`) — full accent.

  The sync button has two more states. An unconfigured vault has a dimmed icon.
  A sync in progress blinks (`data-busy`). Under `prefers-reduced-motion` the
  window removes the blink and the toggle animation in the settings.
- A strip at the edge marks the open note in the list, not only a background.
- Status bar segments have `aria-label`, because their label is a value, not a
  purpose. The message has `role="status"`, and the chip has `aria-haspopup`.
- All labels are in `locales/<locale>.json` at the repository root: one flat
  file per language, in the i18next JSON v4 format. The code uses only keys.
  The window (`ui/src/i18n.ts`) and the macOS menu (`src-tauri/src/menu.rs`)
  read the same files. We do not use the i18next library: the format is
  enough for translation tools (Weblate, Crowdin, Lokalise, i18n Ally), and
  our `t()` is 20 lines.

  The keys are flat and contain dots. Nesting is not possible:
  `statusbar.vault.empty` and `statusbar.vault.empty.title` are both texts.
  Thus a tool must use the flat key style (in i18n Ally,
  `"i18n-ally.keystyle": "flat"`).

  Substitution is `{{name}}`, as in i18next. Plurals are keys with a suffix
  from `Intl.PluralRules`: `statusbar.words_one`, `_few`, `_other`. The code
  asks for `statusbar.words` with `count`. A missing form falls back to
  `_other`, and a missing key falls back to the Russian file.

  The Russian file is the reference. The `Key` type comes from it, so a typo
  in a key is a build error. `ui/tests/i18n.test.ts` compares the other files
  with it: the same keys, the same placeholders, and only the plural forms
  that the language has. The labels of the CodeMirror search panel come from
  the same files.
- There are two locales: `ru` and `en`. You select the language in the status
  bar and in the settings. Each file has its own name in its own language
  (`locale.name`), so "Русский" stays "Русский" in the English list.
- To add a language, add `locales/<code>.json` and one line in `LOCALES` in
  `src-tauri/src/menu.rs`. The window finds the file itself
  (`import.meta.glob`). A Rust test fails if a file is not in `LOCALES`.

## Editor

### Live preview

- `ui/src/live-preview.ts` draws the markup directly in CodeMirror:
  - a heading without hashes;
  - `**bold**` without asterisks;
  - `` `code` `` with a background;
  - `[[link]]` and `[text](url)` without brackets;
  - a ```` ```mermaid ```` block as a diagram;
  - other code blocks as the same `<pre>` as in the preview;
  - a table as a real `<table>`.
- Parsing in the editor uses GFM (GitHub Flavored Markdown,
  `markdown({ base: markdownLanguage })`), not CommonMark. The core renders
  tables, so the editor must show them too.
- The editor highlights code in blocks by the language from the fence. The
  parser gets the language descriptions from `@codemirror/language-data`, and
  it loads a grammar the first time it needs it. The editor, the block widget
  and the preview share the highlight style (`ui/src/highlight.ts`). In the
  preview, the window colours a `<pre><code class="language-…">` block with the
  same style. Token colours are the `code-*` theme variables, so a theme also
  colours code.
- There are two rules:
  - Inline markup — `**bold**`, `` `code` ``, `[link]()`, `[[wiki]]` — shows
    **for each element** (`touched`). The asterisks are visible only while the
    cursor or the selection touches the element itself, with its boundaries.
    The other elements in the paragraph stay rendered.
  - Headings and blocks show **by line** (`revealed`). The line must touch the
    cursor or a selection in one line. For a heading, the line is the element.
    A block does not need a separate toggle: put the cursor in it. The `</>`
    button on a diagram does exactly that.
- A selection across several lines does not reveal a block. Such a selection is
  not an edit. It is an action on a part of the text: copy, delete, replace.
  Before, it revealed the block, and ⌘A changed diagrams and code blocks into
  raw text. You saw hashes and pipes instead of the note. Inline markup under ⌘A
  shows, so you can see exactly what you copy. There is one exception: a
  selection fully inside a code block (`heldOpen`). A selection of several lines
  of code is an edit. Without the exception, the block collapsed into a widget
  during the drag and lost the selection.
- During the drag of such a selection, the editor disables the browser
  selection in the widgets (`editor.ts`, `blockSelection` →
  `[data-selection="block"]`). If not, a second highlight over the table cells
  goes on top of the CodeMirror band. You get boxes with gaps instead of one
  band. In one line the browser selection stays: you use it to read and copy a
  table.
- Esc clears the cell selection. A selection in the document also clears it
  (`cellsFollowSelection`). Before, ⌘A drew a band across all of the note, and
  the rectangle in the table stayed highlighted. That gave two highlights about
  different things at the same time. This rule does not apply to the cursor.
  Without a selection the rectangle stays, because you use it to read.
- A theme change sends `themeChanged` to the editor. The editor increments the
  diagram generation number. If not, CodeMirror would keep the old SVG, because
  the source did not change.
- From the outside, a code block is a widget with one scrollbar. Inside, it is
  usual lines with the block background. Thus a wide ASCII diagram stays
  correct when you read it, and long lines wrap when you edit them.
- One click opens a code block widget for editing. A drag in it stays with the
  browser: you select and copy text as everywhere else.
  - The click at the end of a drag does not open editing. If it did, it would
    remove the new selection (`click` also comes after a drag).
  - The editor finds a drag by the mouse movement from the press point
    (`draggedFrom`), not by the selection. The browser does not reset an old
    selection on a press in an island that you cannot edit. After the first
    selection, every click would look like the end of a drag.
  - The cursor goes to the place of the click (`caretRangeFromPoint`). The
    offset in the widget text is the same as the offset in the block body.
  - For a diagram, the `</>` button and a double-click open the source. One
    click does nothing on purpose. A diagram is a picture, and an accidental
    click must not change it into text.
- The editor catches ⌘C and ⌘X on the document when a selection is inside a
  widget. CodeMirror does not see such a selection: for it, the selection is
  empty. It would put the line under the cursor into the clipboard, and ⌘X
  would also delete that line. The handler stops the event before it gets to
  the editor. The browser itself copies the highlighted text.
- The `liveEnabled` message disables live preview. When the field is off,
  `build` returns an empty set of decorations. A `Compartment` did not work. A
  state field that you add through it at runtime does not give decorations. A
  new state for all of the editor is not possible, because the edit history
  would disappear.
- A widget (`ImageWidget`) draws an image `![caption](file)` and
  `![[file.png]]`. While the cursor touches the markup, you see the markup and
  the image below it. Thus you can see what you edit. A click opens the image in
  the full window. The `</>` button opens the source, as for a diagram. The
  parser reads `![[file.png]]` as an `Image` without a URL. Thus `addWikilinks`
  draws it, not the tree walk.
- This is not a full live preview yet: lists stay as text.

### Tables

- A table works as in Obsidian Live Preview (`ui/src/table-widget.ts`). It is
  always drawn as a table. A click opens the cell in its own small CodeMirror
  editor with the cell markup, at the place of the click. The other cells
  stay drawn.
- Each key in the cell goes to the note at once. The cell editor replaces the
  full cell range (`forward`): the note keeps the cell trimmed, so offsets in
  the cell and in the note do not match at the edges.
- While a cell is open, the widget keeps its DOM (`updateDOM`). Each key
  changes the note, and a new DOM would lose the cell editor and its cursor.
  If the table shape changes (an undo brings back a row), the widget is built
  again, and the cell opens again from `active`.
- The note history is the only history. ⌘Z in a cell runs undo on the note,
  and the cell editor takes the new text (`syncEditor`).
- Keys in a cell:
  - Tab / ⇧Tab — the next / previous cell in reading order. Tab in the last
    cell adds a row.
  - Enter — the same column in the next row; after the last row, a new row.
    Enter in an empty last row removes it and leaves the table, as a list
    ends.
  - ⇧Enter — `<br>`: a real line break would end the table.
  - ⌘Enter — leaves the table below it. Esc — closes the cell.
  - Arrows at the edge of the cell text go to the next cell; up from the
    header and down from the last row leave the table.
- A move to another cell formats the whole table, as Advanced Tables does:
  it pads the columns and aligns the text as the separator says. The padding
  is not an undo step (`addToHistory: false`): ⌘Z undoes the text, not the
  spaces. A new or removed row is a step.
- A pipe typed or pasted into a cell becomes `\|`, a line break a space
  (`cellInput`). The drawn cell shows `\|` as `|`.
- A click outside the table closes the cell (blur). A click in another table
  first closes the open cell. Its formatting can move this table, so the
  position goes through the change map.
- For the note cursor, a table is atomic (`atomicRanges`): the arrows step
  over it.
- The table and the cell editor are inside the note DOM, so the note editor
  sees their browser selection and reads it as its own cursor. This is never
  a note cursor: a click in a cell moved it to the table edge, and a selection
  in the cell editor (another view) mapped to a random position. So the note
  drops a `select` from the DOM inside `.cm-table` (`ignoreTableSelection`).
  Also, a move to another cell rebuilds the table and removes the cell DOM with
  the selection in it. WebKit then moves the selection to some text below the
  table, and the cursor jumped there. So `closeCell` clears the selection
  first. The mock in Chromium does not show this: check it in WebKit.
- The theme of the note editor reaches the cell editor too: its selectors are
  "descendant of the editor". So `styles.css` takes back the padding, the
  height and the scroll for `.cm-cell-editor`. Its class comes through
  `editorAttributes`: CodeMirror rewrites the class of its DOM itself.
- The `</>` button shows the markup of the full table (`sourceTable`), and so
  does a ⌘F match in it. There the Advanced Tables keys work
  (`ui/src/table-editing.ts`), and they also work in the "side by side" mode:
  - Tab, ⇧Tab, Enter as in a cell, but after the last column Tab goes past
    the closing pipe; text there becomes a new column on the next Tab. After a
    run of Tab, Enter goes back to the column where the run started.
  - ⌘Enter leaves the table; ⌥⇧ and an arrow move the row or the column.
  - You start a table with `| Name` and Tab. The CodeMirror parser does not
    see a table without a separator, so the module also finds a table by its
    text: lines that start with a pipe.
  - The markup lines use a monospace font, so that the padding lines up the
    columns.
- Leaving a table always gives an empty line between the table and the
  cursor. By the GFM rules a text line directly below a table is one more row.
- The toolbar has the commands of Advanced Tables: add and remove rows and
  columns, alignment, moves, sort, transpose. Above the drawn table it acts on
  the cell selection or on the open cell; after an action the selection and
  the open cell follow the moved cells. Above the markup it acts on the
  cursor cell.
- `ui/src/table.ts` has no imports. It parses a table into a model, does the
  operations (`runCommand`) and builds the text again. Node runs its tests
  without a build: `pnpm ui:test` (`ui/tests`).
- Width: wide characters (CJK, emoji) count as two, combining marks as zero.
  Without this, a table with such text does not line up.
- Sort: numbers compare as numbers (`2` before `10`, a comma is a decimal
  point), text by `Intl.Collator("ru")`. Empty cells go to the end in both
  directions. The header stays first.
- You cannot delete the header and the last column. The header does not move,
  because a body row in its place would change the meaning of the columns.
- A selection in the drawn table is a rectangle of cells. Drag the mouse from
  one corner to the other. ⇧-click extends the current selection. People
  select cells for an action, but they also select while they read a part of
  the table. The rectangle has an outline.
- The table edges have row and column handles.
  - A press selects the full strip. A drag or ⇧ selects several strips in a
    row. The corner handle clears the selection.
  - A full row or column is the same rectangle at full width or full height.
  - A handle lights when the selection touches its strip, and it is brighter
    when the selection covers all of the strip.
- In one cell, a drag selects text, as everywhere else. When the mouse goes
  into a different cell, the widget takes the selection and clears the browser
  selection. It continues to clear it on `selectionchange` until the drag ends.
  `user-select: none` does not stop every selection that is already in
  progress. A press in a cell immediately clears the previous rectangle. The
  window always has only one highlight.
- ⌘C with a rectangle puts columns with tabs between them and rows with
  newlines between them into the clipboard. A table in a different app expects
  this. The handler is on the document in the `capture` phase: the copy event
  goes to the editor, and CodeMirror would copy the markup. Esc clears the
  selection.
- The window has one drag state, not one for each widget. The `data-from`
  marker keeps the table where the drag started.
- A separate function (`inlineDom`) parses the cells. Decorations do not get
  into a widget, so the code must build the markup of a cell in the DOM by
  hand. It inserts text only as text nodes, because the text comes from a
  different person's vault.
- Width, in the editor and in the preview: a cell has a minimum of 8
  characters, code in a cell does not wrap, and a wide table has its own
  scrollbar. Without this, the browser makes narrow columns as small as
  possible and breaks `C-1` at the hyphen. In the preview, the table also made
  the pane wider instead of a scroll.

### Mermaid diagrams

- The renderer gives a ```` ```mermaid ```` block as `<pre class="mermaid">`
  with the escaped source inside (`render.rs`, `mermaid_blocks`). The language
  is the first word on the fence line, case-insensitive.
- The frontend draws the diagram (`ui/src/diagram.ts`). The code loads mermaid
  with a dynamic `import` on the first diagram. Thus its size does not slow
  the window start. The same `diagram.ts` draws the diagram in the editor. The
  widget builds `<pre class="mermaid">`, the same markup that the core gives.
- The window theme draws the diagrams again. mermaid has its own palette, and
  its own setting selects `light`/`dark`, not CSS variables.
- If mermaid cannot read a diagram, the diagram stays as text. The window shows
  the parser message below it through `data-error`. The message shows the line
  with the typo.

### Comments (notes for the agent)

- You can give an instruction to an agent that will read the file later. The
  UI calls it a "comment" ("заметка"). The marker in the file stays `@agent`,
  because agents look for it. Select words, then use "Add comment" in the
  context menu or ⌘⇧M. kknow puts an HTML comment into the file directly
  after the words: `words<!-- @agent «words»: instruction -->`. The file stays
  clean Markdown. Other viewers do not render the comment, and the agent sees
  it as it is.
- A comment can have no words. With the cursor in a paragraph and no
  selection, kknow puts `<!-- @agent: instruction -->` at the end of the
  paragraph (or of an ATX heading). The comment is about the whole paragraph.
  - The format did not change: the parsers already read a note without `«…»`
    as a note without an anchor.
  - The end of the paragraph keeps the comment inline. A comment on its own
    line would be an HTML block, and an HTML block has no words before it.
  - A setext heading ends with its `===` line, and a table or code has no
    place for an inline comment. There "Add comment" is off.
  - The editor and the preview show an icon at the end of the paragraph. The
    💬 list shows "on the paragraph" instead of the words.
  - A cursor anywhere in the paragraph finds this comment (`noteUnder`). Then
    the menu offers "Edit comment", not a second comment.
- The comment repeats the words inside `«…»` on purpose. The agent does not
  need to guess what the instruction is about. The editor and the preview do not
  need to guess how much text to highlight. kknow highlights the words directly
  before the comment if they match the recorded words. If they are different
  (someone edited the text by hand), a badge appears instead of the highlight.
  The instruction stays.
- Parsing occurs in two places:
  - `kknow-index/src/agent_note.rs` for the preview. The renderer gives
    `<mark class="agent-note" title="…">` with escaped text.
  - `ui/src/agent-notes.ts` for the editor. The editor draws it itself, without
    a call to the core.

  When you change the format, update both.
- The preview still removes raw HTML. The note does not put HTML there; only
  our `<mark>` goes there. The highlight searches for the words at the end of
  one
  text event (`TextMergeStream` joins the parts). Words that start inside
  `**…**` or a link get a badge, not broken markup.
- Metadata parsing from comments skips agent notes. `«words»: text` also has a
  colon. Without this, it would become a field.
- In the editor, the comment stays hidden until the selection goes into it. A
  cursor at the boundary is the end of the highlighted words. A long comment
  that appears there would interfere with typing.

  You see the instruction in the tooltip of the highlight. You edit it in a
  form above the words: "Edit comment" in the context menu or ⌘⇧M opens it
  when the cursor is in the words. The form closes on an edit or a cursor
  move. The form also removes the comment: the gutter icon, ⌘⇧M and the
  comments panel open the form, not the context menu, and without a
  "Remove" button there the user had no way to remove a comment. A comment
  saved empty is removed too. Before, a popup opened by itself over each selection; it covered the
  text and got in the way. An instruction has one line. An empty line in the comment
  would break the paragraph. kknow replaces `-->` inside with `->`, because it
  would close the comment too early.
- All notes of the open text are in a popup under the 💬 button in the header.
  The button with a counter shows only when there are notes. It is a popup, not
  a section of the right wing. The wing hides with the preview, and you need
  the list most while you edit.
  - An item has the selected words on one line and the instruction on two
    lines. CSS cuts a long instruction, and the tooltip shows all of it.
  - A click puts the cursor at the start of the words. It also scrolls them to
    the middle of the screen.
  - The editor gives the list (`onNotes`) on each edit and on the steps of the
    background parse. The editor finds notes through the tree. For long text,
    the tree is not complete immediately.
- In "Read" and "Side by side" you can also add a note from the preview
  (`ui/src/preview-notes.ts`). Select words, then use the "Comment" button
  above the selection or ⌘⇧M. The preview keeps this popup: it is not
  editable, and it has no context menu of its own yet.
  - The preview is HTML from the core. It does not know where its text is in
    the Markdown. Thus `previewAnchor` finds the place in the source by the
    words.
  - Matches in comments, link URLs, raw HTML, code and inside `[[…]]` do not
    count. kknow selects the correct match by its index among the same words in
    the visible text of the preview.
  - The index is correct only if the number of matches is the same in both. If
    not, only a unique match is acceptable.
  - kknow does not guess among several matches, because the note would move to
    different words. The form stays open, and the status asks you to select more
    text.
  - kknow does not move the cursor of the hidden editor.
  - In "Read", a click in the 💬 list scrolls the preview to the highlight.
    kknow cannot find it by order, because the core does not highlight words
    inside markup. Thus it finds the highlight by the instruction text.
- A line with a note has an icon in the gutter (see "Gutter"). The highlight
  stays: it shows the words, and the icon shows that the line has a note even
  when the line is long. A click on the icon opens the note popup.
- You can also write a standalone `<!-- @agent … -->` line by hand. It has
  nothing to highlight. Such a comment stays as text in the editor, and the
  preview does not show it.

### Font size

- `ui/src/note-scale.ts` keeps a multiplier for each file. The key is the same
  as for the position memory (`vault/note` or `file:path`). It is in
  `localStorage`, not in the core settings. It is a window convenience, not a
  property of the note, and there is no reason to sync it. The window does not
  store the default size, so a file at the default size follows a new default.
- The default size is `noteScale` in `settings.json`, in percents (100 by
  default). It is a core setting, not `localStorage`: the settings window
  changes it, and the main window learns about it from the settings event. The
  core clamps a hand-edited value to 70–200 and snaps it to a 10% step.
- Steps are whole percents. If not, steps of 0.1 add a small error, and ⌘0
  after some steps would not give exactly one.
- In the editor, it is the CSS variable `--note-scale` on `.cm-editor`
  (`calc(15px * …)` in the theme). The editor does not make the theme again. The
  theme is in the state, and a new theme would reset the edit history. After a
  change, the editor calls `requestMeasure()`. If not, CodeMirror computes line
  heights with the old font. The preview gets `font-size` in percent, and
  headings in `em` grow with it.
- The control is in the status bar: −, a slider, + and the value (a click
  resets to the default size; the value is accent-colored away from it). The
  "Editor" section of the settings has the same control for the default size.
  Its slider writes on release: each write renders the page again, and a new
  slider under the pointer would stop the drag. It is visible while a file is open, also at 100%. Before,
  the size was a section of the note menu, and the status bar showed only a
  changed value. That section made each context menu longer for a rare
  action. A control that appears only after the first change is hard to find.
  The slider sends a difference in steps to `changeCurrentScale`, so ⌘=/⌘−,
  the palette and the slider use one path.
- A note rename moves its size (`moveScale`). A folder move does not. It is a
  small thing, and it would need a loop over all keys.
- ⌘0 is free, because vaults use ⌘1…⌘9. Tauri does not bind window zoom to
  ⌘=/⌘− (`zoomHotkeysEnabled` is off by default), so the keys are ours.

### Search in the open note

- ⌘F opens the `@codemirror/search` panel above the editor
  (`search({ top: true })`). It highlights all matches. ⌘G and ⌘⇧G step through
  them. You can replace one match or all of them. Esc closes the panel. If you
  selected text, the query starts with that text.
- There are two searches, and different keys open them: ⌘F in the note, ⌘K
  across the vault.
- The panel colours come from `EditorView.theme` in `editor.ts`. CodeMirror has
  its own pair of light and dark palettes for the panel. Its own setting
  selects one, not our variables. The highlight colour comes from `--accent`
  through `color-mix`, so the themes need no new variables.
- Live preview reveals the block that has a match (`found` in
  `live-preview.ts`). It folds the block again when the panel closes. The
  decoration field also computes again when the query changes.

### Gutter

- The editor has a gutter on the left (`ui/src/gutter.ts`). It has a narrow
  marker column and, if the user wants them, line numbers.
- The marker column is always there, also on a note without markers. If it
  came with the first bookmark, all text would jump sideways. Its width comes
  out of the left text padding, so without line numbers the text starts where
  it did before the gutter.
- The column shows two kinds of markers. Other code finds them by class:
  - `cm-bookmark-gutter` — the bookmark ribbon (`bookmarks.ts`);
  - `cm-comment-gutter` — the agent note icon (`agent-notes.ts`). One icon
    stands for all notes of the line, and its tooltip lists them.

  On a line with both, the ribbon is on the left.
- A left click in the column toggles the bookmark of the line. Thus a click on
  an empty cell sets a bookmark, and a click on the ribbon removes it. A click
  on the note icon opens the note popup: the cursor goes to the start of the
  note words, and the popup follows the cursor. The right button and
  Control+click are left for a context menu.
- On hover, an empty cell shows a faint ribbon. A standalone file has no
  bookmarks, so there the hint does not show (`data-bookmarks` on the editor).
- The gutter asks for its markers on each update. Thus both kinds of markers
  are in state fields and are built again only when the bookmark positions,
  the text or the syntax tree change.
- Line numbers are on by default. The setting is `lineNumbers` in
  `settings.json`. They are a window setting, not part of the document, so they
  are in a compartment. The editor turns them on and off without a new state,
  and the undo history stays.
- The gutter has no background and no border. Its colors are the theme
  variables (`--text-muted`, `--accent`, `--warning`), so it fits each theme.

### Bookmarks

- The model and the file are in `kknow-app/src/bookmarks.rs`, not in
  `kknow-core`. The core would need serde for one file. Bookmarks are in
  `.kknow/bookmarks.json` in the vault. They are a property of the folder, and
  the CLI sees them as the window does. `Workspace` writes the file immediately
  after each change, and only if something changed (`edit_bookmarks`). Thus a
  save of a note without bookmarks does not touch the file.
- A bookmark is a note, a **line number**, the **line text** and an optional
  **name**.
  - The `name` field is optional in the file. Old files without names load as
    before, so the format version stays 1. A bookmark without a name has no
    field. An empty name or a name of spaces becomes no name
    (`bookmarks::clean_name`).
  - It is a number, not an offset. CodeMirror counts offsets in UTF-16 units,
    Rust counts them in bytes. On Cyrillic text they are different.
  - The text helps to find the line again after an edit outside the window
    moved it. `Bookmark::locate` searches for the nearest line with the same
    text.
    If two lines are at the same distance, it selects the higher one.
  - If it finds nothing, the bookmark stays at the old number (in the text).
    Maybe someone cut the line and will paste it again.
- While the note is open, the editor has the best data (`ui/src/bookmarks.ts`).
  - Its state holds positions, not numbers. Each edit maps them with
    `assoc = 1`. Enter at the start of a bookmarked line moves the bookmark with
    the text. It does not stay on the new empty line.
  - Each position carries the bookmark name. The user edits names only in
    the editor, so the editor sends them with the lines.
  - On save, the editor computes numbers from positions and sends them with
    the names to `save_note` (`bookmarks`, a list of `Mark`).
    `Bookmarks::place` puts the bookmarks exactly on those lines and takes the
    names. If an edit merges two bookmarks into one line, the named one wins.
  - The match uses the line order, so a bookmark keeps its place in the full
    list.
- You set a bookmark only through the editor (`toggleBookmarkAt`). This is
  true for all sources of the command: ⌘⇧B, the context menu, the preview, a
  click in the gutter. The
  edit has the `changed` effect. On it, `Editor` calls
  `onBookmark`, and the window immediately calls `set_bookmarks` with the
  current text. It does not wait for a save, because maybe there are no edits.
- A right click on the bookmark marker opens its menu: add a name or rename,
  remove the name, remove the bookmark (`ui/src/bookmark-menu.ts`). A hover
  on the marker shows the name as the title of the editor root.
  - The handlers use event delegation on the editor root. They find the
    marker by the target (`.cm-bookmark-gutter`). The line comes from the
    pointer height (`lineBlockAtHeight`).
  - The right click listener captures the event on the root. Thus it runs
    before other context menu handlers of the editor.
  - `menu.ts` opens a menu at a button. A right click has no button, so an
    empty element at the pointer (`.point-anchor`) is the anchor. When the
    menu closes, it gives focus to the anchor, and the anchor gives it to the
    editor.
  - A name change also has the `changed` effect. The window sends
    the bookmarks to the core at once (`set_bookmarks`).
- The preview knows the source lines from markers that the core renderer puts
  (`LINE_MARK_CLASS` in `kknow-index/src/render.rs`).
  - An empty `<span class="line-mark" data-line="N">` is at the start of a
    paragraph, a heading and a list item.
  - Before a code block, a table and a rule, the same marker has `is-block` and
    zero height. The renderer cannot put it inside `<pre>` and `<table>`.
  - The renderer counts numbers from the source, not from the text that went to
    the parser. Preprocessing changes links and drops frontmatter. `LineMap`
    converts the offsets to source offsets.
- The marker goes through the event filters as `Event::Html` with a leading NUL
  (the zero character). It becomes a `<span>` only at the end. If not, the raw
  HTML filter would remove it. If the filter passed such `<span>`s, note
  text could fake them. A note never has a NUL, because the parser replaces it
  with U+FFFD.
- `ui/src/preview-lines.ts` draws badges from the markers. A badge goes at the
  marker of a bookmarked line. Inside a paragraph or code block, it goes at the
  nearest marker above.

  The module also finds the line of a place in the
  preview. It takes the marker before the place and adds the newlines in the
  paragraph or code text. A
  Markdown soft break stays in the HTML as a newline. In a table the module
  does not refine the line, because the separator row `|---|` is not in the
  preview.
  - ⌘⇧B in "Read" takes the line of the selection. Without a selection, it takes
    the first block visible from the top (`topLine`).
  - The "Bookmark" button in the preview popup takes the line where the
    selection starts.
- In the editor the marker is a ribbon in the gutter (see "Gutter"). Before,
  it was a `::before` on the line in the left text padding, and a click on it
  did nothing.
- A rename and a folder move take the bookmarks with them. `Bookmarks::rename`
  gets the same pairs that `Vault::rename_folder` returned. A note delete
  removes
  its bookmarks. If a note disappears outside the window, the bookmarks do not
  forget it. They only do not show it, because git can remove a file for a
  short time.
- The list is the fourth chip of the palette. It is not a search index field.
  There are few bookmarks, and the palette filters them itself by name, title,
  path and line text. A named bookmark shows the name as the row title and
  the note title in the snippet.

  A number from the list can be old. Thus the jump goes to the
  bookmark of the open note that is nearest to it. In "Read" you cannot see the
  line. Thus the jump switches to "Side by side", but the window does not keep
  that mode for the note.

### Hotkeys

- ⌘N — new note.
- ⌘O — open a standalone file.
- ⌘S — save now.
- ⌘K and ⌘⇧P — search palette. Esc in it closes it.
- ⌘F — search in the open note.
- ⌘E — next note mode.
- ⌘\\ — sidebar.
- ⌘⌥← / ⌘⌥→ — back and forward.
- ⌘⇧B — bookmark on the cursor line (⌘B in the editor is bold).
- ⌘⌥B — palette with bookmarks.
- ⌘= / ⌘− / ⌘0 — note font larger, smaller, normal.
- ⌘⇧S — sync.
- ⌘, — settings.
- ⌘1…⌘9 — vault by number.

The keys above are macOS keys. On Windows and Linux ⌘ is Ctrl.

Each key belongs to an action: a named thing the user can do (`note.new`,
`history.back`, `table.rowUp`). The list of actions and their keys is
`ui/src/actions.ts`. The keys use the CodeMirror format (`Mod-Shift-b`), where
`Mod` is ⌘ on macOS and Ctrl elsewhere. Everything reads this list:

- the window handler (`windowActions` in `main.ts`) and the CodeMirror
  keymaps (`bindings(action, run)`);
- every hint: tooltips (`withShortcut`), menu items and palette actions (the
  `action` field), the keys page of the settings.

A hint shows the key as the system writes it: `⇧⌘B` on macOS (Apple order
⌃⌥⇧⌘), `Ctrl+Shift+B` on Windows and Linux. The first key of an action is
the one that hints show; the others are aliases (⌘⇧P for the palette).

The locales have no keys: a label names the action, and the code adds the
key. A key inside a sentence is a placeholder (`{{key}}`, `{{submit}}`). A
test in `ui/tests/i18n.test.ts` fails if a locale has ⌘, Ctrl, Esc or a
similar key name.

The window reads keys by `event.code`, so they do not depend on the keyboard
layout.

#### User keys

The user changes keys on the keys page of the settings or in
`keybindings.json` next to `settings.json`. The file holds only the changed
actions: action ID → the full new list of keys; an empty list removes the
keys. The defaults stay in `actions.ts`, so a new action or a new default key
reaches the user without a migration.

- `kknow_app::keymap` reads and writes the file. A bad entry is skipped and
  reported, the rest works. Unknown action IDs stay in the file: a newer
  version may know them. A file that does not parse is not moved aside on
  read (the window reads it on each focus, and the user may be in the middle
  of an edit); a save from the settings keeps it as `.json.broken`.
- `get_keymap` reads the file from disk each time. So the windows ask on focus
  and see a hand edit without a watcher. `set_keymap` writes and sends
  `keymap-changed` to both windows.
- `useKeymap` in `actions.ts` lays the changes over the defaults, and
  `keysOf` gives the result to everything else. The window reads keys
  synchronously while it builds (CodeMirror keymaps, tooltips), so the changes
  have a copy in `localStorage`, like the language. If the file differs from
  the copy, the main window reloads, as for a language change.
- Some actions do not change (`FIXED`): copy, paste, list arrows, Enter, Esc.
  The system or a text field handles them.

The settings page records a key from `keydown` in the capture phase, before
the window keys. `conflictOf` checks it: a key of another action can move
(`bind` takes it from the old action), the keys of the system (`RESERVED`),
⌘1…⌘9 and the editor's own commands cannot. A key of an editor command would
work everywhere except the note, where CodeMirror handles it first. The
default keys of the actions already win over the editor (⌘I is italic, not
"select the parent node"), so they are not conflicts. A key needs ⌘, Ctrl or
⌥, or it is F1…F24: a plain key types text.

## CLI

Commands:

- `list`, `stats`, `links`, `backlinks`, `tags`, `broken`, `orphans`;
- `show`, `render`, `new`, `search`, `watch`, `themes`, `sync`;
- `vaults` / `vaults add` / `vaults rm` / `vaults use`;
- `commands`, `run`, `bookmarks`, `bookmark <note> <line> [--name <name>]`;
- `trash` / `trash restore <entry>` / `trash empty`.
  - `bookmarks` prints `note:line  «name»  text`. Without a name, there is no
    `«»`.
  - `bookmark … --name` names the bookmark and sets it first if it is not
    there. An empty name removes the name.

- `sync` takes `--dry-run`, `--on-conflict both|local|remote`, and
  `--bucket …` or `--folder …`.
- `commands` takes `--example`.

The global `--vault` accepts a name from the registry, not only a path.
`kknow sync` without an address uses the vault configuration: the same storage
as in the window.

`kknow commands` without a target prints all of the file with the scope of each
command. With a target, it prints only the commands for that target.
`kknow run <command> [target]` returns the exit code of the command itself. On
timeout the code is 124, as with `timeout(1)`. It does not mix the streams:
output goes to stdout, and errors go to stderr. Thus `kknow run … | jq` works.

## Key decisions and their reasons

- **kknow finds wikilinks in the raw text, not through parser events.** `[[…]]`
  is not part of CommonMark. pulldown-cmark can split such a sequence into
  several text events. Instead, the parser gives the code and math ranges, and
  the scanner skips them.
- **kknow changes links in the Markdown before rendering.** `render.rs`
  replaces wikilink and tag spans with usual links with the `kknow://` scheme.
  The UI catches clicks on them. Thus kknow does not need to change the event
  stream of the renderer.
- **kknow removes raw HTML from notes.** A note can come from a different
  person's vault or from sync. A `<script>` in it would run with the privileges
  of the app.
- **A regular expression does the prefix search.** `word*` in tantivy does not
  give a prefix search: the tokenizer drops the asterisk.
- **A delete moves the note to a trash in the vault.** The principle "you
  lose no data" covers more than the cache. A wrong click, or a delete on a
  different device that sync brings here, must not lose a note.
  - The trash is in `.kknow`: the scanner and sync already skip it, and a
    `rename` inside one file system is atomic.
  - The code is in `kknow-core`, because `Vault` owns all file writes. Thus
    the window, the CLI and sync get the trash from one place.
  - The entry stores no metadata file. The path inside the entry is enough
    for a restore, and the entry name gives the delete time.
  - The period is fixed at 30 days. A setting would add a field that nobody
    asked for.
- **kknow finds its own writes by the content hash.** If not, the watcher would
  report the write as an external change. The UI would then load the note again
  while you type.
- **The window saves with a check, not blindly.**
  - `save_note` gets `base`, the text at the start of the edit.
    `Workspace::save_if_unchanged` writes only if the file did not change.
  - Something can change the note outside the window in the meantime: an agent,
    a different editor, git. Then kknow does not touch the disk. It saves the
    text of the window as a copy `Note (конфликт <hash>)`, as sync does.
  - Saves go one at a time (a promise chain). If not, the second save would see
    the first one as an edit by someone else.
  - On window focus, kknow compares the open note with the disk again, because
    the watcher can miss an event.
- **Autosave has a fixed delay and is not a setting.** The window writes 1 s
  after the last key press. It also writes at once when the window loses
  focus, when it closes, and before it leaves the note. Nobody tuned the delay,
  so we removed it from the settings; an old `autosaveMs` in `settings.json`
  is ignored.
  - We did not save only on focus loss. Then a crash would lose all text since
    the last switch, and auto-sync, git and the CLI would see an old file.
    A long unsaved edit also makes a conflict copy more likely.
  - A save with the same text as the disk writes nothing, so a focus change
    without edits does not touch the file.
  - A close waits for the save: Tauri keeps the window open while the JS
    `onCloseRequested` handler runs. This needs `core:window:allow-destroy`.
- **The search index is in memory.** For 5000 notes (20 MB), an open with
  indexing takes about 0.25 s. A persistent index would need schema versions
  and a cleanup of old documents. We postponed this on purpose.
- **kknow builds the full graph again on each save.** A link to a note that did
  not exist can become valid. Thus an incremental update of one note is not
  possible. At the current sizes this takes a few milliseconds.
- **The sync configuration belongs to the vault, not to the app.** One
  configuration for the app would silently send a second vault into the bucket
  of the first one. After that, nobody could tell which notes belong to which
  vault.
- **The type system keeps the secret apart from the configuration.**
  `RemoteConfig` has no field for the secret key. The key comes as a separate
  argument to `to_s3`. Thus you cannot write the key into a file next to the
  notes by accident, only on purpose.
- **kknow validates the values of theme variables.** A theme comes from a
  different person's folder with the notes. A `url(…)` in a colour would get an
  image from a different person's server. Thus a value can use only colour
  functions from an allowlist.
- **The label colour comes from a fixed set.** The label is next to the
  interface text. A random `#hex` from the settings would look bad on a dark
  theme. The registry keeps the colour name, and the values are theme
  variables.
- **Notes from the cloud go to the window through the same event as external
  edits.** `sync_now` sends `vault-changed` itself. The watcher will not report
  these files. The parse already knows them, and they are not a new change. Thus
  the interface has one update path.
- **The configuration selects the provider, not the build.** `SyncEngine` is
  generic over the provider type, but kknow selects the storage at runtime.
  Thus `connect` returns `Box<dyn SyncProvider>`, and `Box` itself implements
  the trait. The engine stays generic, and the selection occurs in one place.
- **One folder provider instead of separate "iCloud" and "Dropbox".** Both
  services sync a usual folder. Separate types in the code would be entities
  without a reason. The interface suggests a path, and the core has one
  `FolderProvider`.
- **A cloud file that is still only in the cloud is an error, not an
  empty value.** iCloud replaces the file with a `.name.icloud` stub. If kknow
  counted
  this as a missing key, sync would decide that a different device deleted the
  note. Then sync would erase it here.
- **kknow reads the notes to send into memory before the network part.** The
  sync has parts so that the network part runs without the vault lock. Thus
  kknow must read the content before the network part starts. The cost: the
  notes to upload stay in memory. For notes of this size, this is cheaper than
  a vault lock for the full sync.
- **Progress counts actions, not bytes.** "3 of 8" tells how much work remains
  better than megabytes. Note files are small, and one transfer never takes a
  long time.
- **The sync state is separate from the vault state.** `AppState` knows about
  vaults, and `SyncStatuses` knows about syncs. They do not need to know about
  each other. The window needs both lists at the same time, because the chip
  shows a problem in a different vault.
- **kknow gets the conflict list from files, not from a log.** A copy of the
  other version is a file in the vault. Thus the count stays correct after a
  restart, without a separate state that can become incorrect. This is the
  same principle as "files are the source of truth".
- **The pause after a failure grows.** If the network is down, it is not useful
  to try again each minute. The pause becomes two times longer, up to eight
  times the interval. A successful sync resets the counter. The rule is in
  `Cadence`, not in the thread. Thus a test checks it, and you do not wait in
  real time.
- **The last sync on close waits for a maximum of five seconds.** The sync is
  idempotent: the next start will finish any remaining work. It is not
  acceptable to keep the window on the screen for this.
- **The window and the terminal share the vault registry.** The list is not in
  the app state. It is in a file, and both sides get its path from one function
  (`vaults::config_dir`). Two similar lists would become different on the first
  day. "Work" in the window and `kknow --vault work` in the shell would go to
  different folders.
- **kknow does not remove a missing folder from the registry**, and a broken
  `vaults.json` does not stop the startup. Someone disconnected an external
  disk, or iCloud did not download the folder. These reasons are external. A
  silent "the list is empty" looks the same as "kknow lost the list".
- **kknow does not permit nested vaults.** In a VS Code workspace, folders can
  be
  inside each other. Here, `VaultRegistry::add` refuses them. The reason is not
  the interface. Each vault has its own watcher and its own sync. Two syncs of
  the same files would damage the notes.
- **There is one lock for each vault, not one for the app.** Sync writes notes,
  and it locks its vault while it runs. With a shared lock, it would delay
  edits in a different vault.
- **Vaults open lazily and stay open.** Each vault has its own index in memory.
  If kknow opened all vaults at startup, it would use start time and memory. If
  it closed a vault after each visit, it would parse the folder again. We
  postponed a close after inactivity on purpose, until the numbers show a need.
- **The state itself gives a watcher to an open vault.** `AppHandle` has the
  runtime as a type parameter. If `AppState` held it, every command would need
  that generic parameter. Instead, at startup kknow puts a closure into the
  state, and the closure can start the thread.
- **The search covers all vaults, but it does not mix the hits.** The tantivy
  score applies to its own index only, and you cannot compare it across vaults.
  Thus the list sets the order, with the active vault first, not the score.
  Only a hit from a different vault gets the vault label, because your own
  vault is already open.
- **An expand of a vault in the sidebar does not make it active.** Themes, the
  sync configuration, the status bar and ⌘N use the active vault. A switch reads
  all of that again. People want to see a different folder more often
  than they want to move to it. Thus an expand is only an expand.
- **The tree knows the vault of each row.** A note button has two attributes,
  `data-vault` and `data-id`. The mark of the open note checks both. Each folder
  can have an `Index`, and with the name only, several rows would light at the
  same time.
- **Search hits are a flat list, not a tree.** The palette sorts them by match
  quality. A layout by folder would lose that order: the best hit would go to
  the middle of the list. The palette shows the folder of a hit as a caption
  under the title.
- **A diagram goes to the UI as source, not as a picture.** The core cannot
  draw mermaid, because mermaid is JavaScript. A Rust version of it would be a
  separate project. Thus the core only marks the block with a class, and
  `kknow render` prints the source. Code-block markup would not work for the
  frontend: mermaid reads the content of the element as text. A nested `<code>`
  would go into the diagram.
- **An image goes to the window as bytes, not as a URL.**
  - The core gives `<img class="note-image" data-src="path">` without `src`.
    The window asks for the bytes with the `note_image` command (`file_image`
    for a standalone file). It inserts a `data:` URL (`ui/src/images.ts`).
  - The first version used `blob:`. In the mock, in a usual browser, it worked.
    In the window, a broken-image icon showed instead of the picture.
  - The possible causes are WebKit with the `tauri://` scheme and the fallback
    IPC (inter-process communication) path. That path returns bytes as an array
    of numbers. `data:` and a change of the response to `Uint8Array` cover both
    causes.
  - The core finds the path (`kknow_core::image`). It looks relative to the
    note, then relative to the root. For a bare name, it looks across all of the
    vault and uses the file nearest to the root.
  - The Tauri `asset:` protocol would need a list of permitted folders in
    advance, but you add vaults at runtime. Our own command also checks that the
    file is an image and is inside the vault. A `../../.ssh/id_rsa` path from a
    different person's note opens nothing.
  - The response is raw bytes (`ipc::Response`). A JSON array of numbers for a
    screenshot of two megabytes would take longer to parse than the image
    itself.
- **kknow does not load images from the web.** The CSP (Content Security
  Policy) permits only `data:`. Only the window sets `src` on images: the core
  does not give it, and kknow removes raw HTML from notes. A `![](https://…)`
  in a note from a different person's vault would tell a foreign server that
  you opened the note. This is the same reason why a theme cannot use `url(…)`.
- **CSS limits the image size, not the core.** An image is not wider than the
  column. It is not taller than 480 px in the preview and 360 px in the editor,
  with `object-fit: contain`. A size from the caption (`|300`, `|300x200`)
  becomes the style of the element. It also does not go past the column. A
  click shows the full image.
- **mermaid runs in strict mode.** Node labels come from the note, that is,
  from a different person's vault. In loose mode, markup in a label would run
  with the privileges of the app. kknow removes raw HTML from the preview for
  the same reason.
- **Inline markup shows for each element under the cursor, not for each
  line.** The first version worked by line. It was simpler, and you saw all the
  markup of the text that you edited. It was not good.

  In Markdown, a paragraph
  is one line. While you read, the cursor or the mouse touched it. Then all the
  markup of the paragraph appeared, and the text moved under your hand.

  The
  cursor does not stop on hidden characters. The boundaries of the element
  count as "inside". When the cursor comes near `**`, it has already revealed
  them.
- **A cell opens in its own CodeMirror editor, as in Obsidian.** There were
  two attempts before:
  - A `<textarea>` in each cell. The search and the undo of CodeMirror did
    not work in a cell, and Tab and Enter did different things than in the
    editor.
  - The full table as Markdown text while the cursor is in it (Advanced
    Tables in the source mode). A wide table with long cells wraps, and its
    rows become an unreadable wall of pipes.

  A nested editor gives the editor keys and one history with the note, and
  the table stays a table.
- **A code block in read mode is a widget, not lines with
  `white-space: pre`.** The first attempt used lines, each with its own scroll.
  The goal was that a wide line would not make the full editor wider. That did
  not work.

  CodeMirror computes the caret position from the content, and it does not know
  about a scroll in a line. Thus the caret moved to the wrong place. A widget
  gives the scroll to the browser. It also gives one scrollbar for each block,
  not one for each line.
- **Live preview decorations come from a state field, not from a view
  plugin.** CodeMirror needs this for anything that changes the block layout.
  If not, it cannot compute heights before it draws. The cost: kknow computes
  over the full document, not only the visible part. For notes of this size,
  you do not see the difference.

  One detail: parsing is lazy. When a note
  opens, the tree can be incomplete. The field must also compute again on the
  steps of the background parse (it compares `syntaxTree` by reference). If not,
  the end of a long note has no widgets until the first click.
- **CodeMirror does the search in the note, not the core.** The core knows
  nothing about the cursor, the selection and the scroll. Search in the open
  note is about exactly these things. The search across the vault stays in the
  core. It needs an index, and the editor has none.
- **A match in a block reveals the block.** Lines in a widget get no
  decorations. Thus you would not see a match in a code block, a table or the
  source of a diagram. A search that silently hides some of its hits is worse
  than no search.
- **The window does not catch shortcuts that the editor already handled.** The
  handler in `main.ts` returns on `event.defaultPrevented`. If not, ⌘F in the
  editor would open the search panel two times. Ctrl-K ("delete to end of line"
  in CodeMirror) would also move the focus to the sidebar.
- **The window as a whole does not scroll** (`body { overflow: hidden }`). Some
  elements are in `body` next to `#app`, for example the palette and menus. If
  the window scrolled, they would move the header and the status bar.
- **Window messages go to the status bar, not to modal dialogs.** There is one
  entry point (`setStatus`). A sync error stays visible in the sync segment
  until the next successful attempt. A modal dialog would show it one time and
  then disappear.
- **Settings are a separate window, not a section of the main window.** You see
  them next to the item that you configure. When you close them, nothing moves
  in your work. The cost: Tauri tests cannot check the second window, because
  the mock runtime does not create windows. Unit tests cover the commands, and
  `ui:mock` checks the window itself.
- **Settings are in a file, and `localStorage` holds a copy.** There are two
  windows. The second webview has its own `localStorage`, and it does not
  report changes.

  The source of truth is now `settings.json`. A command changes
  it, and an event tells the windows about the change. The copy stays for the
  language. `i18n` needs it synchronously, at the first moment of the load. If
  not, the window would build the labels in one language and then draw them
  again in a different one.
- **User keys are a separate file with only the changes.** In `settings.json`
  one bad key would make the whole file broken and reset all settings. A full
  copy of the keys would freeze the defaults: a new action would get no key.
  The file is global, not per vault: keys are a habit of the person, and a
  vault from someone else must not move ⌘S.
- **The S3 fields are in the settings with the secret.** A form in the main
  window would be a second place for the same data. The secret goes to the
  keychain, and the window never reads it again. An empty field means "keep the
  previous
  value".
- **The settings form reads the action from the button, not from
  `returnValue`.** The `close` event of a `<dialog>` does not get to the
  listeners in every webview. In the test window it does not come at all, and
  the window silently did not save the configuration. `submit` is a usual event,
  and
  `event.submitter` tells which button you pressed.
- **The window counts words and reading time, not the core.** The number
  changes while you type. A call to Rust on each key press is not worth it. The
  word count is a regular expression over the editor text, with a debounce. In
  the terminal, `kknow stats` prints the same data, and there the core counts.
- **Reveal in Finder is our own command, not a plugin.** `tauri-plugin-opener`
  would add an npm package and permissions for one action. Instead,
  `reveal_note` calls the system `open -R` (and `explorer`/`xdg-open` on other
  platforms). The core computes the path, and the frontend sends only the `id`.
  Thus this command cannot open a random file.
- **Icons are our own file of paths, not a library.** There are about fifteen
  icons, all in one style. A package would add a dependency tree to the build
  with no reason. Also, `currentColor` gives theme support at no cost.
- **The interface language does not come from the system.** The app was first
  in Russian. On an English macOS, `navigator.language` would silently switch
  the window to a translation that nobody asked for. The selection is explicit,
  and Russian is the default. Automatic detection is one line of code. We will
  add it when there are more than two complete translations.
- **A language change loads the windows again.** The code puts labels into the
  header when it builds it as a string. Labels are also in the `placeholder`
  and `title` of fields, and in the CodeMirror configuration.
  `EditorState.phrases` gets its value when the editor starts.

  A rebuild of all
  of this would be a separate redraw layer for an action that you do one time.
  A reload is more honest and cannot become different from the markup. The
  window saves a pending edit before the reload.
- **Error messages do not go through the dictionary.** They come from the core
  as strings, and the CLI uses the same strings. A translation would need error
  codes and a dictionary in Rust too. We postponed this on purpose. The window
  shows such a string as it is.
- **Commands are in the app settings, not in the notes folder.** This is the
  only exception to "the configuration is next to the item that it applies to".
  The reason is security.
  - A theme from a different person's folder can colour the window, and even
    then kknow validates its values.
  - A command from a different person's folder would run with the privileges
    of the app. The folder comes through sync, from a different computer or from
    a shared folder.
  - An open of an unknown folder must not mean "run the text in it".
  - A command itself tells that it belongs to a specific vault, with the
    `vaults` field.
- **The target path comes as an environment variable, not as text in the
  script.** Text substitution would permit command injection. A file name can
  contain `"; rm -rf ~; #`, and file names come from a different person's
  folder and from sync. A variable goes to the process without command-line
  parsing. Whatever it holds stays a value. A test checks this.
- **kknow checks the scope also on run, not only when it shows the menu.** If
  not, `run_command` with a foreign name in its arguments would bypass
  `targets`, `vaults` and `match`. These fields are the only limits on a
  command.
- **A run does not hold the vault lock.** kknow computes only the path under the
  lock. The external program runs as long as it needs to. An upload to the
  cloud that takes one minute must not delay the autosave of a note.

  The sync
  has the parts "plan — network — write" for the same reason. In Tauri this
  needs `#[tauri::command(async)]`. Without it, the command would run on the
  main thread, and the window would stop responding.
- **On timeout kknow kills the process group, not the process.**
  `sh -c 'work'` almost always runs the work as a separate process. If kknow
  killed only the shell, the work would stay alive and keep the pipe open. The
  wait for output would then continue for exactly the time that we tried to
  limit. The cost was the `libc` crate and one `unsafe` for `killpg`. The
  standard library has no process groups.
- **A broken commands file stays in its place.** kknow renames a vault registry
  with content that it cannot read to `.broken`. The app writes that file, and
  you lose nothing there. A person writes the commands file. It would be rude
  to remove their edit because of a missing bracket. kknow returns an error,
  and the file stays where it was.
- **Our own pattern matcher instead of `globset`.** The language is small (`*`,
  `?`, `**`), and we want to see the behaviour in tests next to the code. This
  is
  the same reason as for our own icons.
- **A command parameter is in the note, not in the manifest.** For
  `wiki-publish replace <id> <file>`, each file has a different page number.
  - In `commands.json`, this would need one command for each page.
  - A separate list would be a second registry. It would become different from
    the notes after the first rename.
  - The metadata of a note is the correct place for such data. It moves with
    the file, and you can see it in the editor.

  Thus the parameter comes as variables, and `requires` also decides where the
  menu item shows.
- **kknow reads metadata also from a comment, not only from frontmatter.**
  Frontmatter has a cost: everyone who opens the file outside kknow sees it. A
  `readme.md` in a repository would show the block on GitHub. A publish to
  Confluence would copy it onto the page.

  Thus a service line would appear
  where nobody wants it. Nobody sees `<!-- confluence: … -->`. kknow removes
  raw HTML from the preview, and other Markdown viewers do not render comments
  by definition.
- **Our own code parses the metadata, not a YAML library.** Notes have only flat
  `key: value` pairs. Nested mappings cannot go into an environment variable
  anyway. As before, we postponed full YAML parsing on purpose.
- **kknow reads the shell profile only on request.** It takes time. It also
  prints its own output into the same stream that kknow shows as the command
  output. During tests, `fish` complained about a missing `.cargo/env.fish`
  directly in the report. Thus `login` is off by default, and the template uses
  `fish`, which does not need it.

## Commands

Run everything **from the repository root**. `tauri` searches for
`src-tauri/tauri.conf.json` relative to the current directory.

```bash
pnpm install          # once; ui is included as a pnpm-workspace package
pnpm dev              # the app in development mode
pnpm ui:mock          # the interface in a browser without Tauri, on stubs
pnpm build:app        # a ready kknow.app
pnpm install:app      # the same, installed into /Applications
```

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p kknow-cli -- --vault testdata/vault stats
cargo run -p kknow-cli -- --vault testdata/vault themes
cargo run -p kknow-cli -- --vault testdata/vault sync --dry-run --bucket <name>
cargo run -p kknow-cli -- --vault testdata/vault commands --example
cargo run -p kknow-cli -- --vault testdata/vault run <command> <note>
```

The build puts the app into `target/release/bundle/macos/kknow.app`. The app
has no signature. Thus, at the first start from Finder, macOS asks you to
confirm (right-click → "Open").

`pnpm install:app` builds the app and installs it into `/Applications`
(`scripts/install-app.sh`). Without the installation, `open -a kknow ~/notes`,
"Open With" and Spotlight do not work. LaunchServices knows only an installed
app, not a bundle in `target`.

The script stops the previous version, but only
the one that runs from the install directory. A development build is in a
different location, and the script does not touch it. You can give the
directory as an argument. The tests of the script use this, so they do not
touch the real `/Applications`.

## Environment pitfalls

- **The Tauri `setup` hook runs inside the event loop.** Tauri calls it on
  `RunEvent::Ready`, that is, after the app starts to get events. "Open folder
  in app" on a cold start comes before `Ready`. The handler then finds a state
  that `setup` did not fill yet. This bug was difficult to find:
  1. The folder went into an empty temporary registry.
  2. The write to disk silently did not occur, because the registry had no
     file path yet.
  3. Then `setup` overwrote everything with the data that it read from disk.

  There was no error and no result. Thus kknow prepares everything that does
  not need Tauri (the registry, settings, argument parsing) **before**
  `tauri::Builder`. `setup` keeps only the work that needs the window.
- **The `.dmg` builds only from your own terminal.** The `bundle_dmg.sh` script
  styles the image window through Finder and AppleScript. Without permission to
  control Finder, it fails. Thus `pnpm build:app` builds only the `.app`. Run
  the full `pnpm build` (with `.dmg`) by hand.
- **macOS canonicalizes paths.** `/var/folders` is a symlink to
  `/private/var`, and notify returns resolved paths. You must canonicalize the
  root of the watcher. If not, no path will match. The duplicate check in the
  vault registry also depends on this.
- **The FSEvents subscription is not instant.** Tests need a pause between the
  watcher start and the first file write. If not, you see a race, not the
  behaviour of the code.
- **pnpm 11 does not read `pnpm.onlyBuiltDependencies` from package.json.** The
  settings are now in `pnpm-workspace.yaml`. The current key is `allowBuilds`.
- **The Russian stemmer does not restore the fleeting vowel.** «заметок» and
  «заметки» are different words for the search. Lemmatization is a separate
  task.
- **The keychain asks for permission from a build without a signature.** macOS
  gives the right to an entry to the binary that created it. Each build makes a
  new binary. In dev mode, the window asks for access to its own key. "Always
  Allow" applies until the next build. macOS asks for each storage address, so
  now it asks several times.
- **In tests, secrets are in memory.** Under `cfg(test)`, kknow uses a
  different `secrets::imp`. If not, `cargo test` would ask the person who runs
  it for permission and leave entries in their keychain.
- **Tests give the commands file from outside.** kknow puts the path to
  `commands.json` into `AppState` at startup. The command does not search for it
  in the settings. If not, `cargo test` would read the file of the person who
  runs it. The result would then depend on their scripts. This is the same
  reason as for the keychain.
- **A killed shell does not stop its children.** `child.kill()` sends a signal
  to one process. A `sleep` that this shell started stays alive and keeps the
  pipe open. We found this because a test with a one-second timeout ran for
  thirty seconds. That is exactly the time that the child slept.
- **Tauri tests cannot check the second window.** The mock runtime does not
  create windows. Unit tests cover the commands, and `ui:mock` checks the
  window itself.

## How to verify your work

The project has about 230 tests, and they cover all layers:

- parsing and the graph;
- Tauri commands on the mock runtime;
- an end-to-end test "file edit → watcher → search and graph";
- sync of two devices through the in-memory provider and through a folder;
- parsing and validation of themes;
- the vault registry and separate locks;
- commands with their variables, timeout and output truncation.

Tests do not use the network or the keychain. They use an in-memory provider
instead of S3, secrets in process memory, and the commands file in a temp
directory. Thus `cargo test` asks for nothing and leaves nothing. It does start
processes, but only `sh` with its own scripts in temp folders.

Tests against real S3 have the `#[ignore]` mark, so `cargo test` needs nothing.
To run them:

```bash
docker compose up -d
cargo test -p kknow-sync --test minio -- --ignored
```

After changes, run `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings`. Check the UI behaviour
in `pnpm ui:mock`. There the interface runs on stubs and does not need a built
app.
