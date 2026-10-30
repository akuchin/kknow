# What is kknow

kknow is a fast, local-first knowledge base for plain Markdown files. You write
notes in a desktop window, and the files stay on your disk as ordinary `.md`
files.

## Three ideas

### Files are the source of truth

A **vault** is an ordinary folder of `.md` files. You can open the same folder
in any other editor, keep it in git or put it in Dropbox. kknow does not add a
database next to your notes.

kknow keeps other data, but all of it is a cache:

- the link graph and the backlinks;
- the full-text search index;
- the state of the last sync.

kknow can build the cache again at any time. If you remove it, you lose no
notes.

### One core, many frontends

The logic is in Rust: parsing, links, search, sync and file watching. The same
core serves three frontends:

- the **window** — an editor with live preview, built on Tauri;
- the **CLI** (command-line interface) — the `kknow` program in the terminal;
- the **MCP server** — the same vaults for an AI agent, through the Model
  Context Protocol.

A note that you save in the window is visible at once in `kknow search` and to
the agent.

### Local first

kknow works without a network and without an account. Sync is optional, and
you select the storage: an S3 bucket, a MinIO server, iCloud Drive or a plain
folder. kknow never loads images or themes from the web, so a note from another
person cannot tell a third-party server that you opened it.

## Who it is for

kknow is a good fit if you:

- want an Obsidian-like editor, but on plain files that other tools can use;
- keep several separate folders of notes, for example work, study and personal;
- use the terminal or AI agents with your notes;
- want to own the sync storage.

## What kknow is not

- It is not a cloud service. You do not get hosting or a web version.
- It does not have plugins in JavaScript. You extend it with
  [custom commands](/features/commands) in any language.
- It does not sync through git yet. You can still keep a vault in a git
  repository: kknow shows the changes from `git pull` as any outside edit.

## Next steps

- [Install kknow](/guide/quick-start) and open your first vault.
- Read the [feature overview](/features/).
- Read how it works inside: [Architecture](/architecture).
