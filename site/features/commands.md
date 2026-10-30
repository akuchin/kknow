# Custom commands

You can attach your own program to a note, a folder or a vault. For example:

- publish a note to Confluence;
- run a linter on a folder;
- build an archive of the vault;
- give the comments of a note to a coding agent.

## Run a command

- In the window, open the row menu in the tree (**⋯** or right-click). Each
  command has a key on the right, for example <kbd>1</kbd> for the first one.
- The <kbd>⌘K</kbd> palette shows the commands of the open note.
- The ▶ button in the note header runs the last command again.
- In the terminal, use `kknow commands` and `kknow run`.

The status bar shows the result, for example "done in 1.2 s". Click it to see
the output of both streams. The output shows line by line while the command
runs, and the window has a stop button. If a command fails, the output window
opens automatically.

## Create a command

Go to **Settings → Commands**, or select **Configure commands…** in the row
menu. The form has these fields:

- the label and the identifier;
- the interpreter: `shell` (that is, `sh`), `bash`, `fish`, `zsh` or `python`;
- the script;
- the scope: on what, in which vaults, for which paths, with which note fields;
- the running options: the start directory, the timeout, a confirmation;
- the environment variables.

The form shows a problem in a command, for example an empty script, before
the first run.

## The file

kknow keeps the commands in `commands.json` next to the vault list. You can
edit it by hand. The **Reveal** button shows the file.

```json
{
  "version": 1,
  "commands": [
    {
      "id": "confluence",
      "name": "Update the Confluence page",
      "kind": "fish",
      "script": "wiki-publish replace $KKNOW_FM_CONFLUENCE \"$KKNOW_PATH\"",
      "targets": ["note"],
      "requires": ["confluence"]
    }
  ]
}
```

Only `id`, `name`, `kind` and `script` are necessary.

| Field | Meaning |
|---|---|
| `targets` | `note`, `folder`, `vault` |
| `vaults` | vault names from `kknow vaults` |
| `match` | path patterns from the vault root: `*`, `?`, `**` |
| `requires` | frontmatter fields that the note must have |
| `cwd` | `vault` or `target`: the start directory |
| `timeoutSeconds` | the time limit, 60 by default |
| `confirm` | ask before the command runs |
| `login` | run `sh`, `bash` or `zsh` as a login shell |
| `env` | environment variables |
| `program` | a program to run instead of the default interpreter |

A field that you do not set does not limit anything.

## Variables

::: warning The script gets the path in a variable, not in its text
A file name like `"; rm -rf ~; #` must not run as a command. File names come
from other people's vaults and from sync. Always quote the variable:
`"$KKNOW_PATH"`.
:::

| Variable | Value |
|---|---|
| `KKNOW_PATH` | full path of the target: the note file, the folder or the vault |
| `KKNOW_TARGET` | `note`, `folder` or `vault` |
| `KKNOW_RELATIVE` | path from the vault root; empty for the vault |
| `KKNOW_NOTE` | note identifier (path without `.md`), notes only |
| `KKNOW_VAULT` | vault root |
| `KKNOW_VAULT_ID`, `KKNOW_VAULT_NAME` | the vault in the list |
| `KKNOW_FM_*` | the frontmatter fields of the note |

## A per-note parameter

Sometimes a value is different for each note, for example a Confluence page
number. Write it in the note itself, in one of two ways.

An HTML comment is invisible in the preview, on GitHub and on the published
page:

```markdown
# About the project

<!-- confluence: 987654321 -->
```

Frontmatter is the more common convention:

```markdown
---
confluence: 987654321
---
```

The command gets the value as `$KKNOW_FM_CONFLUENCE`. `"requires":
["confluence"]` shows the command only on notes with the number. Thus you can
rename or move the note, and nothing breaks.

The editor has a shortcut for this: right-click and select **Link to
Confluence**. The comment shows as a chip with the page number.

## Shipped commands

If you have no `commands.json`, kknow gives two commands:

- `confluence` — runs `confluence-tools replace` with the page number of the
  note.
- `agent` — gives the [comments for the agent](/features/agents#comments-for-the-agent)
  of the note to a coding agent. The `AGENT` variable selects `claude` (Claude
  Code, the default) or `copilot` (GitHub Copilot CLI).

## The command does not find my program

An app that starts from Finder or the Dock gets the system `PATH`:
`/usr/bin:/bin:/usr/sbin:/sbin`. It does not have `/opt/homebrew/bin` or
`~/.local/bin`. Select one of these fixes:

1. **Use `"kind": "fish"`** if you use fish. fish reads `config.fish` at each
   start, so the command sees the same environment as your terminal.
2. **Set `env` at the top of the file.** It applies to all commands:
   ```json
   { "version": 1, "env": { "PATH": "/Users/you/.local/bin:/opt/homebrew/bin:/usr/bin:/bin" }, "commands": [] }
   ```
3. **Use `"login": true`** for `shell`, `bash` and `zsh`. The shell then reads
   your profile.
4. **Use a full path**: `"program": "~/.local/bin/wiki-publish"`.

Keep tokens in your shell profile, not in `env`. `commands.json` is plain text.

## Why commands are not in the vault

`commands.json` belongs to the app, not to a vault. A vault can come from sync
or from another person. When you open an unknown folder, kknow must not run
the commands in it.
