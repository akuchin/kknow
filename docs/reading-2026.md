# Plan: reading features (TODO items 4 and 5)

This plan covers two items from [TODO.md](../TODO.md), "Reading: the next
features": the side panel of a note, and then hover previews and real embeds.
Do the steps in order. Each step is one commit. After each step, run
`cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings`. After a UI step, also
run `pnpm ui:test` and `pnpm ui:e2e`.

## What exists and what is missing

| Need | Exists now | Missing |
|---|---|---|
| Outline | `headingsOf` (`ui/src/outline.ts`) follows unsaved edits; the preview has `data-line` marks | The section on the screen is not known |
| Backlinks with the line | `NoteView.backlinks: LinkRef { id, title, label }` | `Edge` has no position of the link, so there is no line and no line text |
| Unlinked mentions | — | `Frontmatter::fields` takes only scalars: `aliases: [a, b]` is one string, a block list `- a` is skipped |
| Render of one section | `link_target` finds the heading line | No section range, no render of a part of a note |
| Embeds `![[note]]` | `EdgeKind::Embed` in the graph; the render shows a link (`render.rs`) | No content source in the render, no loop guard, no live preview widget |

## Item 4: the side panel

### Step 1. Core: link positions and aliases

- `Edge` gets `span`: the range of the link in the source note.
- `Workspace::backlink_lines(id)` gives each link to the note with its line
  and the line text around the link (`before`, `link`, `after`). It reads each
  source note one time. The list is in note order, then in text order.
- The window loads these lines on request (`note_backlinks`), not in
  `NoteView`. `NoteView` comes back after each save, and a note with many
  backlinks would read many files on each save.
- CLI `kknow backlinks` and the MCP tool `backlinks` show `note:line`.
- `Frontmatter::list(key)`: an inline list `[a, "b"]` or a block list
  `- a`, only for one key. This is not a YAML parser.
  `ParsedNote.aliases` holds the `aliases` (and `alias`) values.

### Step 2. Core: unlinked mentions

- `Workspace::unlinked_mentions(id)` finds lines where the note name or an
  alias stands as plain text. Tantivy gives the candidate notes with a phrase
  query. A text check then finds the exact places: word boundaries, any case.
- Not a mention: the note itself, frontmatter, code, existing `[[…]]` and
  `[…](…)` links.
- `Workspace::link_mention(from, span, target)` changes the text into
  `[[target|text]]`, or into `[[text]]` when the text is the target. The write
  goes through the usual save with a `base` check, so it does not erase an
  external edit.
- Tauri commands and stubs in `ui/src/mock`.

### Step 3. UI: the panel

- A new module `ui/src/window/aside.ts` (layer 4, next to `marks.ts`).
- The grid of `#app` gets a fourth column `--aside` in all three modes. A
  hidden panel sets it to zero, as `--sidebar` does.
- Three sections: outline, backlinks, unlinked mentions. Mentions load only
  when their section is open.
- The section on the screen:
  - "Live preview" and "Side by side": the top visible line of the editor;
  - "Read": the `data-line` marks of the preview (`topLine`), once a frame
    on scroll.

  A click on a heading scrolls to it in the same view.
- The action `aside.toggle` in the keymap, default `Mod-Alt-p`. (`Mod-Alt-\`
  was the first idea, but CodeMirror uses it for "indent the selection".) The
  state is in `localStorage`, one key
  for the window, not one for each note.
- A standalone file shows only the outline.
- Remove the backlinks section under the preview and `renderBacklinks` in
  `note.ts`. The toolbar button stays: it shows the count and opens the panel
  at the backlinks.
- UI strings in `locales/ru.json` and `locales/en.json`.
- Docs: `site/features/side-panel.md` (sidebar and index),
  `site/reference/` shortcuts, `site/architecture.md`, ARCHITECTURE.md
  ("Layout", "Window code"), README. Close "Backlinks are not visible in
  Edit" in TODO.md.
- E2E: the panel in all three modes, a click on a heading, "Link" on a
  mention.

## Item 5: hover preview, then real embeds

### Step 4. Section render and hover preview

- `Workspace::render_section(id, subpath)`. A section starts at the heading
  line from `link_target` and ends before the next heading of the same or a
  higher level. `#^block` takes the paragraph or the list item of the block.
  No subpath: the whole note without frontmatter.
- The render uses the saved text, without `data-line`, bookmarks and comment
  highlights: the fragment belongs to a different note.
- Tests: nested headings, the last section, setext headings, a heading that
  does not exist (`None`).
- `ui/src/hover-preview.ts`: ⌘ and hover on a link in the preview
  (`a[href^=kknow:note/]`) and in the editor (`editor-links.ts` finds the
  link). A delay of about 300 ms. A cache by `(vault, id, subpath)` that
  `vault-changed` clears.
- The popup closes on Esc, when the pointer leaves, and when ⌘ goes up while
  the pointer is outside the popup. A click opens the note at the line. A
  broken link shows "no such note".

### Step 5. Embeds in the preview

- `to_html` gets a content source: a trait `EmbedSource` in `kknow-index`.
  `Workspace` implements it, so the core does not depend on the UI.
- `![[note#H]]` becomes a marker. After the render, the marker becomes
  `<div class="embed" data-note=… data-sub=…>` with the HTML of the section.
- The depth limit is 3. A set of visited `(id, subpath)` stops a loop. A loop
  or a too deep embed stays a link with a mark.
- No `data-line` inside an embed: the line sync must not see it.
- An embed inside a paragraph renders as a block too. A test shows this.
- Search does not index the embedded text. Write this decision in "Key
  decisions" in ARCHITECTURE.md.
- `kknow render` in the CLI gets embeds with no extra work.
- On `vault-changed`, the window renders the open note again when a changed
  note is the target of one of its `Embed` edges. Deeper embeds update on the
  next render.

### Step 6. Embeds in live preview and docs

- `EmbedWidget`: a block decoration, like `DiagramWidget`. It calls
  `render_section` asynchronously, shares the cache with the hover popup and
  keeps a placeholder height until the answer comes.
- The widget is read-only. A click opens the note. The cursor on the line
  shows the source `![[…]]`.
- Check WebKit traps in Playwright WebKit: the mock in Chrome does not show
  them.
- Remove "Real content embedding" from "Postponed on purpose" in TODO.md.
- Docs: the links and embeds page, ⌘ and hover in the shortcuts, ARCHITECTURE.md
  ("Live preview", "Key decisions"), README.

## Open questions

- Mentions: a "Link" button on each line only. "Link all" can come later.
- Obsidian also resolves `[[alias]]` to the note. This plan does not do that.
