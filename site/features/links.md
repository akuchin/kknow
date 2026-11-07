# Links and Markdown

kknow reads standard CommonMark and adds the wiki syntax that Obsidian users
know.

## What the parser understands

| Syntax | Meaning |
|---|---|
| `[[note]]` | link to a note |
| `[[note\|alias]]` | link with a different text |
| `[[note#Heading]]` | link to a heading |
| `[[note#^block]]` | link to a block |
| `text ^block` | a block ID at the end of a paragraph or a list item |
| `![[note]]` | embed a note or an image |
| `![[note#Heading]]` | embed one section of a note |
| `[text](note.md)` | usual Markdown link |
| `#tag`, `#nested/tag` | tags |
| `---` … `---` at the top | YAML frontmatter |

The parser skips code and math. `` `[[this is not a link]]` `` is not a link,
and `#include` in a code block is not a tag.

## Embeds

`![[note]]` shows the text of the other note in place, in a frame. The preview,
Read mode and live preview show it. In live preview the embed must stand alone
on its line; inside a line of text it stays a link. Put the cursor on the line,
or click the `</>` button of the embed, to see the source. `![[note#Heading]]` shows one section: the heading and
the text up to the next heading of the same or a higher level.
`![[note#^block]]` shows the paragraph of the block.

- An embed is read-only. To change the text, open the note: a click on the
  embed opens it at that section.
- An embed can contain embeds, three levels deep. A deeper embed stays a link.
- A note cannot embed itself, also through other notes. Such an embed stays a
  link with a dashed frame, and its tooltip tells why.
- When the embedded note changes, the embed shows the new text.
- Search does not find the text of an embed in the note that embeds it. The
  text belongs to the other note, and search finds it there.
- `![[picture.png]]` is an image, not a note. See
  [Editor → Images](/features/editor).

## How a short link finds a note

A short link follows the usual wiki rules:

1. kknow tries the full path from the vault root.
2. Then it tries the file name.
3. Then it tries a path suffix.

If many notes have the same name, kknow selects the note that is closest in the
folder tree.

## Write and open links

The editor completes links and tags as you type, and <kbd>⌘</kbd>-click opens
a link. See [Editor → Links](/features/editor#links).

A link to a heading matches the heading text without case. A Markdown link can
also use the slug: `[plan](Plan.md#next-steps)` finds `## Next steps`.

## Backlinks

The note header shows the number of notes that link to the open note. The
[note panel](/features/side-panel) lists them with the line of each link, and
also the places where the note name is plain text. In the terminal:

```bash
kknow backlinks "Project plan"   # each link as note:line and its line
kknow links "Project plan"     # outgoing links
kknow broken                   # links to notes that do not exist
kknow orphans                  # notes with no incoming links
```

## Rename and move

When you rename or move a note or a folder, kknow updates the links to it in
the other notes. The rule is simple: after the move, each link must find the
same note.

- A short link that still finds the note stays as it is. Thus most moves change
  no other file, and git and sync see no noise.
- A link that would break gets the full path.
- The alias, `#Heading`, `#^block` and the `!` of an embed stay.
- Links in code stay too.

The status bar tells how many notes changed.

## Raw HTML

The preview does not show raw HTML from a note. A note can come from another
person's vault or from sync. A `<script>` in it would run with the privileges
of the app.

HTML comments stay in the file. kknow uses two kinds of them:

- `<!-- @agent «words»: task -->` — a comment for an AI agent. See
  [Agents](/features/agents#comments-for-the-agent).
- `<!-- confluence: 987654321 -->` — a parameter for a custom command. See
  [Custom commands](/features/commands#a-per-note-parameter).
