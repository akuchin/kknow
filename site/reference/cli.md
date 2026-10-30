# CLI

The `kknow` program works with the same vaults as the window. It uses the same
core, so the results are the same.

## Select a vault

Each command works on one vault. Give it in one of these ways:

```bash
kknow --vault ~/notes stats        # a path
kknow --vault Personal stats       # a name from `kknow vaults`
export KKNOW_VAULT=Personal        # an environment variable
```

Without `--vault`, kknow uses the current directory.

## Notes

| Command | What it does |
|---|---|
| `list` | list notes |
| `stats` | notes, folders and size |
| `show <note>` | the parsed structure of a note |
| `render <note>` | the note as HTML |
| `new <name>` | make an empty note |
| `search <query> [--limit N]` | full-text search, the same syntax as ⌘K |
| `links <note>` | outgoing links |
| `backlinks <note>` | notes that link to the note |
| `tags` | tags and their counts |
| `broken` | links to notes that do not exist |
| `orphans` | notes with no incoming links |
| `tasks [--all]` | open tasks; `--all` adds the done ones |
| `bookmarks` | bookmarks: `note:line  «name»  text` |
| `bookmark <note> <line> [--name <name>]` | add, remove or name a bookmark |
| `watch` | print the changes in the vault |
| `themes` | themes of the vault and their problems |

## Vaults

```bash
kknow vaults                               # the list; the active one is marked
kknow vaults add ~/notes --name Personal   # add a folder
kknow vaults use Personal                  # the window opens with it
kknow vaults label Personal home ai        # set labels; no labels removes them
kknow vaults rm Personal                   # forget; the files stay
```

## Sync

```bash
kknow sync                                 # use the vault configuration
kknow sync --dry-run                       # show the plan, change nothing
kknow sync --folder ~/Dropbox/notes        # sync through a folder
kknow sync --bucket kknow --endpoint http://localhost:9000 --prefix work
kknow sync --on-conflict both|local|remote
```

For a vault with git sync, `kknow sync` runs fetch, merge and push, and
`--dry-run` shows the branch, the unpushed commits and the changes. The command
does not commit.

| Variable | Meaning |
|---|---|
| `KKNOW_S3_BUCKET`, `KKNOW_S3_ENDPOINT`, `KKNOW_S3_REGION`, `KKNOW_S3_PREFIX` | the same as the flags |
| `KKNOW_S3_ACCESS_KEY`, `KKNOW_S3_SECRET_KEY` | access keys; there are no flags for them |
| `KKNOW_FOLDER` | the same as `--folder` |
| `KKNOW_GIT` | the git program; the default is `git` from `PATH` |
| `KKNOW_GIT_TOKEN` | the repository token for the "token" sign-in |

## Custom commands

```bash
kknow commands                    # all commands with their scope
kknow commands "Project plan"     # commands for one note
kknow commands --example          # write the shipped commands to the file
kknow run confluence "Project plan"
kknow run lint docs --kind folder
kknow run agent --file ~/Desktop/draft.md
```

`kknow run` returns the exit code of the command. On a timeout, the code is
124, as with `timeout`. The output goes to stdout and the errors go to stderr,
so `kknow run … | jq` works.

## Trash

```bash
kknow trash                       # entries: ID, path, age, number of notes
kknow trash restore 1761300000    # put an entry back
kknow trash empty                 # remove all entries for good
```

## MCP

```bash
kknow mcp --vault Personal              # one vault
kknow mcp --label work --label study    # all vaults with one of the labels
```

The server uses stdin and stdout. An agent starts it as a child process. See
[Agents](/features/agents).

## Output language

The CLI output and the error messages are in Russian.
