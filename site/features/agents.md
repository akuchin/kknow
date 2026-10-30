# Agents (MCP)

kknow gives your vaults to an AI agent through MCP (Model Context Protocol).
Claude Code, Cursor and other MCP clients can search your notes, read them,
mark tasks as done and do the tasks that you leave in comments.

## Connect an agent

While the app runs, it has an MCP server on `127.0.0.1`. You do not install
anything else.

1. Click **MCP** in the status bar.
2. Select which vaults the agent gets.
3. Copy the command for Claude Code, or the JSON block for `.mcp.json`, Cursor
   and other clients.

The command looks like this:

```bash
claude mcp add --transport http kknow 'http://127.0.0.1:7377/mcp' \
  --header 'Authorization: Bearer <token>'
```

The dot in the status bar shows the server state:

- green — the server runs;
- gray — the server is off;
- red — the server could not start, for example another program uses the port.

The same window has the on/off switch and the port.

## Which vaults the agent sees

The address selects the vaults:

| Address | Vaults |
|---|---|
| `/mcp?vault=<id>` | one vault |
| `/mcp` | all vaults in the list |
| `/mcp?label=work&label=study` | the vaults with at least one of the labels |

A vault that gets the label later comes in at once.

## Without the window

The CLI has the same server on stdin and stdout. The agent starts it as a child
process:

```bash
claude mcp add kknow -- kknow mcp --vault Personal
claude mcp add kknow-work -- kknow mcp --label work --label study
```

## Tools

| Tool | What it does |
|---|---|
| `list_vaults` | the vaults of the server, with labels and note counts |
| `search` | full-text search, the same syntax as ⌘K |
| `list_notes` | notes with their titles, all or in one folder |
| `read_note` | the Markdown of a note and the path to its file |
| `backlinks`, `links` | incoming and outgoing links of a note |
| `tasks`, `set_task` | open tasks; mark a task as done or open |
| `agent_comments` | all comments for the agent |
| `resolve_comment` | remove a comment after the agent did its task |

With several vaults, each tool has an optional `vault` argument.

The server does not edit the text of notes. An agent with file access, for
example Claude Code, edits the file at the path from `read_note`. kknow shows
the change in the open note at once.

## Comments for the agent

Comments are the main use of the server. You mark words in a note and write
a task for them.

1. Select words in the window.
2. Press <kbd>⌘⇧M</kbd> (or right-click and select **Add comment**).
3. Write the task, for example "check this number".

kknow writes an HTML comment into the file:

```markdown
The project took 14 weeks<!-- @agent «14 weeks»: check this number -->.
```

The editor highlights the words and hides the comment. The 💬 button in the
header lists all comments of the note.

Then tell the agent: "Do the comments in my vault". The agent gets all comments
in one call with `agent_comments`, does each task, and calls
`resolve_comment`. The words stay in the text.

`set_task` and `resolve_comment` check the line first. If the line changed, the
tool changes nothing and tells the agent to get the list again.

## Security

- The server listens only on `127.0.0.1`.
- Each request needs the token.
- The server refuses a request with a foreign `Host` or `Origin`, so a web page
  in your browser cannot use it.
