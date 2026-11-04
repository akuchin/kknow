# Agents (MCP)

kknow gives your vaults to an AI agent through MCP (Model Context Protocol).
Claude Code, Cursor and other MCP clients can search your notes, read them,
mark tasks as done and do the tasks that you leave in comments. An agent can
ask you a question about a task, and it can propose edits that you accept or
reject in the window.

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
| `agent_comments` | the comments for the agent, with their threads |
| `claim_comment` | take a task, so that a second agent does not do it too |
| `reply_comment` | write in the thread of a comment; with `question` the task waits for you |
| `resolve_comment` | remove a comment after the agent did its task, with a summary |
| `propose_edit` | propose edits to a note; the note does not change until you accept |
| `propose_note` | propose a new note |
| `proposal_status` | the state of a proposal and your feedback |
| `list_proposals` | the proposals that wait for review |
| `withdraw_proposal` | remove a proposal of the agent |

With several vaults, each tool has an optional `vault` argument. The tools
that take an ID (`claim_comment`, `proposal_status` and others) find the vault
themselves.

The server changes the text of a note only in three cases: it marks a task, it
removes a done comment, or you accept a proposal. An agent with file access,
for example Claude Code, can also edit the file at the path from `read_note`.
kknow shows the change in the open note at once.

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

## Threads: the agent asks, you answer

Each comment can have a thread: the messages between you and the agent about
the task. The agent asks when the task is unclear or the sources disagree,
and it does not guess:

```text
you:    check this number
claude: the report says 14 weeks, the letter says 12. Which source is right?
you:    the report
```

The 💬 list shows the thread under each comment, with its state:

| State | Meaning |
|---|---|
| open | nobody works on the task |
| in progress | an agent took the task |
| question | the agent waits for your answer |
| answered | you answered; the agent can go on |
| in review | the agent proposed an edit; it waits for your review |
| done | the task is done, the comment is gone from the note |

When an agent asks a question, the 💬 button gets a dot. Click **Reply** under
a comment to write in its thread. A question opens the reply field at once.

A comment gets an ID with the first message of its thread:
`<!-- @agent#k3f «14 weeks»: check this number -->`. The task stays in the
note, so an agent without MCP still sees it. The messages are in
`.kknow/threads/k3f.json`.

### An agent without MCP

An agent that only edits the file (the `edit` mode of the
[`agent` command](/features/commands#shipped-commands)) asks in the text: it
writes `<!-- @agent-note: … -->` just after the comment. When the window opens
the note, or reloads it after the agent, kknow moves this note into the thread
of the comment. The question shows in the 💬 list with the reply field, and
the text of the note stays clean.

Answer in the list and run the agent again. It reads the thread file of each
comment with an ID: it skips a comment that still waits for your answer and
does the task of a comment that you answered.

A `@agent-note` that does not follow a comment (after an empty line, or in a
paragraph without a comment) stays in the text.

## Proposals: you accept each edit

With `propose_edit`, the agent does not change the note. It sends a list of
replacements, and the ✎ button in the header shows how many proposals wait.

1. Click ✎. Each edit shows the removed text in red and the new text in
   green.
2. Clear the box of an edit that you do not want.
3. Optional: write a note for the agent, for example "the month is right".
4. Click **Apply selected**. **Reject all** applies nothing.

The agent reads your decision and your note with `proposal_status`, and it can
try again. **Remove** deletes the proposal without an answer.

Each edit finds its place by text, not by line. The old text must occur
exactly once in the note (text inside `@agent` comments does not count). You
can edit the note while a proposal waits. If the text of an edit is gone, the
edit shows as stale and kknow does not apply it.

A proposal can answer a comment: the agent passes the comment ID. Then the
review closes the task. An accepted edit removes the comment, and a rejected
one gives the task back to the agent with your note.

Proposals let an agent without file access, for example Claude Desktop, edit
your notes. They are also a safe way to run an agent on a schedule: it only
proposes, and you review in the morning.

The CLI has the same review:

```bash
kknow proposals
kknow proposals accept 7f3a2b --only 1,3 --feedback "the month is right"
kknow proposals reject 7f3a2b --feedback "the report says 14"
```

## Rule for doubt

The server tells each agent: when you are not sure, do not guess. If the task
is unclear, ask with `reply_comment`. If there are several good versions,
propose each one, or list them in the reply, and let the person choose.

The shipped [`agent` command](/features/commands#shipped-commands) has the
same rule. In its `propose` mode it works only through proposals and
questions.

Threads and proposals sync with the notes: see
[What sync copies](/features/sync#what-sync-copies-from-kknow).

## Security

- The server listens only on `127.0.0.1`.
- Each request needs the token.
- The server refuses a request with a foreign `Host` or `Origin`, so a web page
  in your browser cannot use it.
