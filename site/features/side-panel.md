# Note panel

The note panel is on the right side of the window. It shows three lists for
the open note: the outline, the backlinks and the unlinked mentions. The panel
is the same in all three modes: Edit, Side by side and Read.

<kbd>⌘⌥P</kbd> or the button at the right end of the header hides and shows
the panel. kknow keeps this choice for the window, not for each note.

Each list is a section that you can fold. A click on the section name folds
or opens it, and kknow keeps the open sections for the next start.

## Outline

The outline shows the headings of the note. It follows your edits at once,
also before kknow saves the note.

- The heading of the section at the top of the screen has a color. When you
  scroll, the color moves with you.
- A click on a heading moves the heading to the top of the screen.
- At the end of a long note, the last heading on the screen has the color:
  the last sections cannot scroll to the top.

<kbd>⌘⇧O</kbd> finds a heading by its text in the palette.

## Backlinks

Backlinks are the links from other notes to the open note. The list shows each
source note once, with the line of each link in it. The link in the line is
marked.

- A click on the note name opens the note.
- A click on a line opens the note at that line. In **Read** mode, kknow
  scrolls the preview to the line.
- A note with many links to the open note shows three lines and a "more"
  button.

The button with the link icon in the header shows the number of notes that
link here. A click on it shows the panel with the backlinks open.

## Unlinked mentions

An unlinked mention is a place where the name of the open note is plain text,
not a link. For the note `Release plan`, the line "the release plan is late"
has a mention.

kknow looks for the file name of the note and for its aliases:

```markdown
---
aliases: [Roadmap, "Plan for Q3"]
---
```

A block list (`- Roadmap`) and the older `alias` key also work.

- Case does not matter. The name must be a whole word: `plan` does not find
  "planning".
- Code, links, tags, frontmatter and HTML blocks have no mentions.
- **Link** changes the mention into a link to the open note: `[[Release plan|release plan]]`.
  When the text is the same as the note name, the link has no alias. When the
  name finds a different note from the source note, the link gets the full
  path.

kknow looks for mentions only when the section is open: it must read the
notes that have the name.

A standalone file has only the outline: it has no vault to find links in.
