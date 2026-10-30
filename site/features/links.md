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
| `![[note]]` | embed a note or an image |
| `[text](note.md)` | usual Markdown link |
| `#tag`, `#nested/tag` | tags |
| `---` … `---` at the top | YAML frontmatter |

The parser skips code and math. `` `[[this is not a link]]` `` is not a link,
and `#include` in a code block is not a tag.

## How a short link finds a note

A short link follows the usual wiki rules:

1. kknow tries the full path from the vault root.
2. Then it tries the file name.
3. Then it tries a path suffix.

If many notes have the same name, kknow selects the note that is closest in the
folder tree.

## Backlinks

The note header shows the number of notes that link to the open note. Its menu
opens each of them. In the terminal:

```bash
kknow backlinks "Project plan"
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
