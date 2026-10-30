# Search

kknow has two searches: <kbd>⌘K</kbd> in all vaults, and <kbd>⌘F</kbd> in the
open note. This page is about ⌘K. For ⌘F, see
[Editor](/features/editor#find-in-the-note).

## The palette

<kbd>⌘K</kbd> (or <kbd>⌘⇧P</kbd>) opens the palette above the window.

- Type the query. The list updates while you type.
- Use the arrow keys to select a result, and <kbd>Enter</kbd> to open it.
- If the search finds nothing, <kbd>Enter</kbd> makes a note with the typed
  name.
- <kbd>Esc</kbd> closes the palette.

While the field is empty, the palette shows the recent notes.

## Query syntax

The search uses [tantivy](https://github.com/quickwit-oss/tantivy), a search
engine in Rust.

| Query | Finds |
|---|---|
| `sync conflict` | notes with both words |
| `"exact phrase"` | the phrase |
| `-draft` | notes without the word |
| `tag:rust` | notes with the tag |
| `title:kknow` | notes with the word in the title |

The last word also matches as a prefix: `synch` finds "synchronization".

The chips next to the field limit the search to titles or tags. Click a tag in
the preview to open the palette with a `tag:…` query.

## All vaults at once

The query goes to all vaults at the same time. Often you remember a text but
not the folder with it.

- The results of the active vault come first.
- A result from another vault has the label of that vault.
- When you open it, kknow switches the vault and then opens the note.
- The label filter of the sidebar also limits the search.

## Russian words

A stemmer brings Russian word endings to one form. Thus «синхронизации» finds
«синхронизацию».

## Speed

Each open vault has one search index in memory. kknow builds it when it opens
the vault. For 5,000 notes (20 MB), this takes about 0.25 seconds, with the
parsing and the link graph. The files are the source of truth, so kknow can
always build the index again.

## Other palettes

The same palette has more lists:

| Keys | List |
|---|---|
| <kbd>⌘⇧O</kbd> | headings of the open note |
| <kbd>⌘⌥B</kbd> | bookmarks of the vault |
| <kbd>⌘⌥T</kbd> | open tasks of the vault |

## In the terminal

```bash
kknow search "tag:rust -draft" --limit 20
```
