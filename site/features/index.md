# Features

kknow is a Markdown editor, a search engine and a sync client in one small
app. This page lists all features. Each section links to a page with the
details.

## Write

- **Live preview.** Headings, lists, tables, images and code look finished
  while you type. → [Editor](/features/editor)
- **Three modes per note:** Edit, Side by side and Read. kknow keeps the mode
  for each note.
- **Tables as in a spreadsheet.** Tab between cells, sort, move rows and
  columns, select a rectangle and copy it.
- **Images.** Paste or drop an image, and kknow saves it into `attachments/`.
- **Mermaid diagrams** in the preview, in the colors of your theme.
- **Font size per note**, from 70% to 200%.

## Connect

- **Wiki links:** `[[note]]`, `[[note|alias]]`, `[[note#Heading]]`, embeds and
  tags. → [Links and Markdown](/features/links)
- **Backlinks** in the header of each note.
- **Safe renames.** A rename or a move updates the links in the other notes.

## Organize

- **Many vaults in one sidebar**, as folders in a VS Code workspace.
  → [Vaults](/features/vaults)
- **Labels** group vaults, for example `#work`, and filter the tree and search.
- **Bookmarks** on lines, **tasks** from `- [ ]` items, and a **trash** with a
  30-day restore. → [Bookmarks, tasks, trash](/features/organize)
- **Standalone files.** Open any `.md` file without a vault.
  → [Standalone files](/features/standalone-files)

## Find

- **⌘K search in all vaults at once**, with tantivy and a Russian stemmer.
  → [Search](/features/search)
- **⌘F** in the open note, with replace and regular expressions.
- **⌘⇧O** to go to a heading.

## Share and automate

- **Sync** with S3-compatible storage, any folder (iCloud Drive, Dropbox) or
  a git repository with a changes view and a diff. Conflicts keep both versions.
  → [Sync](/features/sync)
- **MCP server for AI agents.** Claude Code, Cursor and other clients read your
  vaults, do tasks and resolve your comments. → [Agents](/features/agents)
- **Custom commands.** Run your own script on a note, a folder or a vault.
  → [Custom commands](/features/commands)
- **CLI.** All data is also available in the terminal.
  → [CLI reference](/reference/cli)

## Make it yours

- **Themes:** Gruvbox dark and light, and your own JSON themes.
  → [Themes and language](/features/themes)
- **Shortcuts:** change any key in the settings.
  → [Keyboard shortcuts](/reference/shortcuts)
- **Languages:** Russian (the default) and English.
