# Bookmarks, tasks, trash

## Bookmarks

A bookmark marks a line in a note.

- <kbd>⌘⇧B</kbd> adds a bookmark to the line with the cursor. Press it again to
  remove the bookmark.
- A click to the left of a line does the same.
- In **Read** mode, ⌘⇧B marks the selection, or the first visible paragraph.

A bookmark can have a name. Right-click the bookmark icon and select
**Add name**.

<kbd>⌘⌥B</kbd> shows all bookmarks of the vault in the palette. Type to filter
them by name, note title, path or line text. Press <kbd>Enter</kbd> to open the
note at that line.

A bookmark moves with its line when you add text above it. kknow keeps
bookmarks in `.kknow/bookmarks.json`, so the terminal sees them too:

```bash
kknow bookmarks                         # note:line  «name»  line text
kknow bookmark plan 12                  # add or remove a bookmark
kknow bookmark plan 12 --name summary   # name it
```

Sync does not copy bookmarks. They stay on one machine.

## Tasks

A task is a list item with a box:

```markdown
- [ ] open task
- [x] done task
```

Nested items, numbered items and items in a quote are tasks too. A `- [ ]` in
a code block is an example, not a task.

- <kbd>⌘⌥T</kbd> shows the open tasks of the vault. Press <kbd>Enter</kbd> to
  open the note at the task line.
- Click a box in the preview to mark the task as done, or open again. kknow
  writes the file at once, and <kbd>⌘Z</kbd> undoes the change.

If the line is no longer a task (someone changed the file), kknow refuses the
click and changes nothing.

```bash
kknow tasks          # open tasks: note:line  text
kknow tasks --all    # also the done ones
```

## Trash

A delete does not remove a note for good. The note or the folder goes to the
trash of the vault, `.kknow/trash`. A note that sync removes also goes there.

- Each vault tree ends with the **Корзина** (Trash) node. The number is the
  number of entries.
- Click a deleted note to read it. **Восстановить** (Restore) in its header
  puts it back.
- The entry menu has **Удалить навсегда** (Delete forever). The trash node has
  **Очистить корзину** (Empty trash).

kknow removes entries older than 30 days on the next delete. A restore never
overwrites a note: if the old path is taken, nothing moves.

```bash
kknow trash                       # entries: ID, path, age, number of notes
kknow trash restore 1761300000    # put an entry back
kknow trash empty                 # remove all entries for good
```

Sync does not copy the trash. Each machine has its own trash.
