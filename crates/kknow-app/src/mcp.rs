//! MCP (Model Context Protocol): vaults for an AI agent.
//!
//! This module is the protocol without a transport. It takes one JSON-RPC 2.0
//! message and gives the reply. Two frontends carry the messages:
//!
//! - the app: an HTTP server on `127.0.0.1` while the window is open
//!   (`src-tauri/src/mcp_http.rs`). It uses the vaults that the window has
//!   open, so an edit of the agent shows in the window at once;
//! - the CLI: `kknow mcp` on stdin and stdout, for a machine without the
//!   window.
//!
//! The protocol needs only a few methods, so it is written by hand on
//! `serde_json`: an SDK would bring an async runtime for a loop over messages.
//!
//! The tools give what the window gives: search, notes, links, tasks and the
//! `@agent` comments. The comments are the reason for the server: a person
//! leaves a task in the text, and the agent finds all of them in one call and
//! removes each one when it is done.
//!
//! With several vaults each tool takes an optional `vault`, and each row starts
//! with `[vault]`. With one vault the rows have no prefix: the agent does not
//! need a word that is the same in each row.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kknow_core::NoteId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::proposals::Status as ProposalStatus;
use crate::text::{backlink_row, describe, plain_snippet, task_row};
use crate::threads::{CommentRef, ME, Status as ThreadStatus, Thread};
use crate::{Error, Result as AppResult, Workspace};

/// Protocol versions the server knows, the newest last. The server answers
/// with the version of the client if it knows it, and else with the newest.
pub const VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

// JSON-RPC error codes.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// A vault that a request serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultRef {
    pub id: String,
    pub name: String,
    pub labels: Vec<String>,
}

/// Where the vaults are. The CLI owns its workspaces; the app has them in
/// shared slots behind a lock. The tools see only this trait.
pub trait Host {
    /// The vaults of this request, in list order.
    fn vaults(&self) -> &[VaultRef];

    /// Reads the workspace of vault `index`. An error is a vault that cannot
    /// open, in words for the agent.
    fn read<R>(&self, index: usize, action: impl FnOnce(&Workspace) -> R) -> Result<R, String>;

    /// Changes the workspace of vault `index`.
    fn write<R>(
        &mut self,
        index: usize,
        action: impl FnOnce(&mut Workspace) -> R,
    ) -> Result<R, String>;

    /// A tool changed a note. The app tells the window: its own write does not
    /// come back from the file watcher.
    fn changed(&mut self, _index: usize, _note: &NoteId) {}
}

/// The reply to one message. A notification (no `id`) gets no reply.
pub fn handle(host: &mut impl Host, message: &Value) -> Option<Value> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Some(failure(
            message.get("id").cloned().unwrap_or(Value::Null),
            INVALID_REQUEST,
            "нет поля method",
        ));
    };
    let id = message.get("id")?.clone();
    let params = message.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call(host, &params),
        _ => Err((METHOD_NOT_FOUND, format!("неизвестный метод: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, text)) => failure(id, code, &text),
    })
}

/// A JSON-RPC error reply.
pub fn failure(id: Value, code: i64, text: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": text } })
}

fn initialize(params: &Value) -> Value {
    let wanted = params.get("protocolVersion").and_then(Value::as_str);
    let version = wanted
        .filter(|wanted| VERSIONS.contains(wanted))
        .unwrap_or(VERSIONS[VERSIONS.len() - 1]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "kknow", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

/// What the agent reads first. The last paragraph is the rule for doubt:
/// a person would rather answer a question or pick from two versions than
/// find a guess in the note later.
pub const INSTRUCTIONS: &str = "kknow vaults: plain Markdown notes with [[wiki links]], #tags \
and `- [ ]` tasks. list_vaults shows which vaults you have.\n\n\
A person leaves tasks for you in the text as `words<!-- @agent «words»: task -->` \
(with a thread ID: `@agent#k3f`). Call agent_comments to find them. Take a task \
with claim_comment, do it, then call resolve_comment with a short summary.\n\n\
To change a note without file access, or when the person should check the change, \
call propose_edit: the note stays as it is until the person accepts each edit. \
Pass `comment` when the edit answers a comment. Check the result later with \
proposal_status: a rejected edit has the reason in `feedback`.\n\n\
When you are not sure, do not guess. If the task is unclear or the sources \
disagree, ask with reply_comment and question=true, and stop work on that task. \
If there are several good versions, propose them as separate proposals or list \
them in the reply, and let the person choose.";

/// Tool descriptions for `tools/list`. They are for the agent, so they are in
/// English, like the rest of the protocol. The results are CLI output and stay
/// in Russian.
fn tools() -> Value {
    let note =
        json!({ "type": "string", "description": "Note path without .md, or only its name" });
    let vault = json!({
        "type": "string",
        "description": "Vault name from list_vaults. Without it, all vaults",
    });
    let agent = json!({ "type": "string", "description": "Your name, for example claude" });
    let comment_ref = json!({
        "id": { "type": "string", "description": "Comment id from agent_comments" },
        "note": note,
        "line": { "type": "integer", "description": "Line from agent_comments, if there is no id" },
        "text": { "type": "string", "description": "Task text from agent_comments, if there is no id" },
        "agent": agent,
    });
    let mut tools = json!([
        {
            "name": "list_vaults",
            "description": "Vaults this server serves, with labels and note counts.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "search",
            "description": "Full-text search. Supports tantivy syntax: \
                tag:rust, title:plan, -word, \"exact phrase\".",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "description": "Per vault, default 10" },
                },
                "required": ["query"],
            },
        },
        {
            "name": "list_notes",
            "description": "List notes with their titles.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string", "description": "Only notes in this folder" },
                },
            },
        },
        {
            "name": "read_note",
            "description": "Read the Markdown of a note. The first line is the file path.",
            "inputSchema": { "type": "object", "properties": { "note": note }, "required": ["note"] },
        },
        {
            "name": "backlinks",
            "description": "Links to a note as `note:line  text of the line`.",
            "inputSchema": { "type": "object", "properties": { "note": note }, "required": ["note"] },
        },
        {
            "name": "links",
            "description": "Notes that a note links to.",
            "inputSchema": { "type": "object", "properties": { "note": note }, "required": ["note"] },
        },
        {
            "name": "tasks",
            "description": "Open tasks (`- [ ]`) as `note:line  text`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": note,
                    "all": { "type": "boolean", "description": "Also done tasks" },
                },
            },
        },
        {
            "name": "set_task",
            "description": "Mark the task on a line as done or open.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": note,
                    "line": { "type": "integer", "description": "1-based line from tasks" },
                    "done": { "type": "boolean" },
                },
                "required": ["note", "line", "done"],
            },
        },
        {
            "name": "agent_comments",
            "description": "Tasks that a person left for you in the text, as \
                `note:line [id] (status) «words»: task`, status only when not open, with the thread messages below. \
                «words» is the text the task is about; without it, the task is about \
                the whole paragraph. Do the tasks with status open or answered; \
                `question` waits for the person, `review` waits for a proposal review.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": note,
                    "all": { "type": "boolean", "description": "Also tasks that wait for the person" },
                },
            },
        },
        {
            "name": "claim_comment",
            "description": "Take a task before you start, so another agent does not do it too. \
                Use the id from agent_comments, or note+line+text for a comment without one.",
            "inputSchema": { "type": "object", "properties": comment_ref.clone() },
        },
        {
            "name": "reply_comment",
            "description": "Write in the thread of a comment. With question=true the task \
                waits for the person: ask when the task is unclear, when sources disagree, \
                or to let the person choose between versions. Do not guess.",
            "inputSchema": {
                "type": "object",
                "properties": merged(&comment_ref, json!({
                    "message": { "type": "string" },
                    "question": { "type": "boolean", "description": "Wait for the answer of the person" },
                })),
                "required": ["message"],
            },
        },
        {
            "name": "resolve_comment",
            "description": "Remove an @agent comment after you did its task. \
                The words it was about stay in the text, the thread keeps the summary.",
            "inputSchema": {
                "type": "object",
                "properties": merged(&comment_ref, json!({
                    "summary": { "type": "string", "description": "What you did, in one line" },
                })),
            },
        },
        {
            "name": "propose_edit",
            "description": "Suggest edits to a note. The note does not change until the person \
                accepts them in kknow. Each `old` must occur exactly once in the note \
                (text inside @agent comments does not count): take more words around it \
                if needed. Edits must not overlap. Returns the proposal id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": note,
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": { "type": "string" },
                                "new": { "type": "string" },
                            },
                            "required": ["old", "new"],
                        },
                    },
                    "reason": { "type": "string", "description": "Why, in one line, for the person" },
                    "comment": { "type": "string", "description": "Comment id that this proposal answers" },
                    "agent": agent.clone(),
                },
                "required": ["note", "edits", "reason"],
            },
        },
        {
            "name": "propose_note",
            "description": "Suggest a new note. It appears only after the person accepts it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": { "type": "string", "description": "Path of the new note without .md" },
                    "content": { "type": "string" },
                    "reason": { "type": "string" },
                    "agent": agent.clone(),
                },
                "required": ["note", "content", "reason"],
            },
        },
        {
            "name": "proposal_status",
            "description": "The state of a proposal and of each edit, and the feedback of the person.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
            },
        },
        {
            "name": "list_proposals",
            "description": "Proposals that wait for review, so you do not suggest the same twice.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note": note,
                    "all": { "type": "boolean", "description": "Also reviewed proposals" },
                },
            },
        },
        {
            "name": "withdraw_proposal",
            "description": "Remove your proposal that is no longer right.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
            },
        },
    ]);
    // Each tool but `list_vaults` gets the optional `vault`: one place for all.
    for tool in tools.as_array_mut().expect("список").iter_mut().skip(1) {
        tool["inputSchema"]["properties"]["vault"] = vault.clone();
    }
    tools
}

/// Two JSON objects in one: the properties that tools share and their own.
fn merged(base: &Value, extra: Value) -> Value {
    let mut out = base.clone();
    if let (Some(out), Value::Object(extra)) = (out.as_object_mut(), extra) {
        out.extend(extra);
    }
    out
}

/// `tools/call`. A tool that fails (no such note, a stale line) is a normal
/// result with `isError`: the agent reads the reason and tries again. Only a
/// call that does not fit the protocol is a JSON-RPC error.
fn call(host: &mut impl Host, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "нет имени инструмента".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let args = Args(&args);

    let outcome = match name {
        "list_vaults" => list_vaults(host),
        "search" => search(host, &args),
        "list_notes" => list_notes(host, &args),
        "read_note" => read_note(host, &args),
        "backlinks" => backlinks(host, &args),
        "links" => links(host, &args),
        "tasks" => tasks(host, &args),
        "set_task" => set_task(host, &args),
        "agent_comments" => agent_comments(host, &args),
        "claim_comment" => claim_comment(host, &args),
        "reply_comment" => reply_comment(host, &args),
        "resolve_comment" => resolve_comment(host, &args),
        "propose_edit" => propose_edit(host, &args),
        "propose_note" => propose_note(host, &args),
        "proposal_status" => proposal_status(host, &args),
        "list_proposals" => list_proposals(host, &args),
        "withdraw_proposal" => withdraw_proposal(host, &args),
        _ => return Err((INVALID_PARAMS, format!("неизвестный инструмент: {name}"))),
    };
    let (text, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

/// Tool arguments with errors in words for the agent.
struct Args<'a>(&'a Value);

impl Args<'_> {
    fn str(&self, key: &str) -> Result<&str, String> {
        self.0
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("нужен строковый аргумент {key}"))
    }

    fn opt_str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    fn uint(&self, key: &str) -> Result<usize, String> {
        self.0
            .get(key)
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .ok_or_else(|| format!("нужен числовой аргумент {key}"))
    }

    fn bool(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(Value::as_bool)
    }

    /// Indexes of the vaults to look in: the one from `vault`, or all.
    fn vaults(&self, host: &impl Host) -> Result<Vec<usize>, String> {
        let vaults = host.vaults();
        let Some(wanted) = self.opt_str("vault") else {
            return Ok((0..vaults.len()).collect());
        };
        let lower = wanted.to_lowercase();
        vaults
            .iter()
            .position(|vault| vault.id == wanted || vault.name.to_lowercase() == lower)
            .map(|index| vec![index])
            .ok_or_else(|| {
                let names: Vec<&str> = vaults.iter().map(|vault| vault.name.as_str()).collect();
                format!("хранилище не найдено: {wanted}; есть: {}", names.join(", "))
            })
    }

    /// The vault and the note from `note` (and `vault`, if given).
    ///
    /// A note name can be in two vaults. Then the agent must say which one:
    /// a guess would change a note in the wrong vault.
    fn note(&self, host: &impl Host) -> Result<(usize, NoteId), String> {
        let raw = self.str("note")?;
        let mut found = Vec::new();
        for index in self.vaults(host)? {
            if let Some(id) = host.read(index, |workspace| workspace.find(raw).cloned())? {
                found.push((index, id));
            }
        }
        match found.as_slice() {
            [] => Err(format!("заметка не найдена: {raw}")),
            [one] => Ok(one.clone()),
            many => {
                let names: Vec<&str> = many
                    .iter()
                    .map(|(index, _)| host.vaults()[*index].name.as_str())
                    .collect();
                Err(format!(
                    "заметка {raw} есть в нескольких хранилищах: {}; укажите vault",
                    names.join(", ")
                ))
            }
        }
    }

    /// The `note` argument if the agent gave it: a filter for a list.
    fn opt_note(&self, host: &impl Host) -> Result<Option<(usize, NoteId)>, String> {
        match self.0.get("note") {
            None | Some(Value::Null) => Ok(None),
            Some(_) => self.note(host).map(Some),
        }
    }

    /// The vaults of a list: the vault of `note`, if given, or `vault`, or all.
    fn listed(&self, host: &impl Host) -> Result<(Vec<usize>, Option<NoteId>), String> {
        Ok(match self.opt_note(host)? {
            Some((index, id)) => (vec![index], Some(id)),
            None => (self.vaults(host)?, None),
        })
    }
}

type Outcome = Result<String, String>;

/// A row with the vault in front, if there is more than one vault.
fn row(host: &impl Host, index: usize, text: impl AsRef<str>) -> String {
    let vaults = host.vaults();
    if vaults.len() > 1 {
        format!("[{}] {}", vaults[index].name, text.as_ref())
    } else {
        text.as_ref().to_string()
    }
}

/// Rows joined, or the text for an empty list.
fn rows_or(rows: Vec<String>, empty: &str) -> String {
    if rows.is_empty() {
        empty.to_string()
    } else {
        rows.join("\n")
    }
}

/// An app error in words for the agent.
fn text(error: Error) -> String {
    error.to_string()
}

fn list_vaults(host: &impl Host) -> Outcome {
    let mut rows = Vec::new();
    for (index, vault) in host.vaults().iter().enumerate() {
        let (count, root) = host.read(index, |workspace| {
            (workspace.len(), workspace.root().display().to_string())
        })?;
        let mut line = format!("{}  заметок: {count}  {root}", vault.name);
        if !vault.labels.is_empty() {
            let labels: Vec<String> = vault
                .labels
                .iter()
                .map(|label| format!("#{label}"))
                .collect();
            line.push_str(&format!("  {}", labels.join(" ")));
        }
        rows.push(line);
    }
    Ok(rows_or(rows, "хранилищ нет"))
}

/// Each vault has its own index, and their scores cannot be compared. So the
/// results go vault by vault, in list order, as in the palette.
fn search(host: &impl Host, args: &Args) -> Outcome {
    let query = args.str("query")?;
    let limit = args.uint("limit").unwrap_or(10);
    let mut rows = Vec::new();
    for index in args.vaults(host)? {
        let hits = host.read(index, |workspace| workspace.search(query, limit))?;
        for hit in hits.map_err(text)? {
            let snippet = plain_snippet(&hit.snippet);
            let head = row(host, index, format!("{}  ({})", hit.id, hit.title));
            rows.push(if snippet.is_empty() {
                head
            } else {
                format!("{head}\n    {snippet}")
            });
        }
    }
    Ok(rows_or(rows, "ничего не найдено"))
}

fn list_notes(host: &impl Host, args: &Args) -> Outcome {
    let folder = args
        .opt_str("folder")
        .map(|folder| format!("{}/", folder.trim_matches('/')));
    let mut rows = Vec::new();
    for index in args.vaults(host)? {
        let texts = host.read(index, |workspace| {
            workspace
                .notes()
                .filter(|(id, _)| {
                    folder
                        .as_deref()
                        .is_none_or(|folder| id.to_string().starts_with(folder))
                })
                .map(|(id, parsed)| match parsed.title() {
                    Some(title) if title != id.name() => format!("{id}  ({title})"),
                    _ => id.to_string(),
                })
                .collect::<Vec<_>>()
        })?;
        rows.extend(texts.into_iter().map(|text| row(host, index, text)));
    }
    Ok(rows_or(rows, "заметок нет"))
}

fn read_note(host: &impl Host, args: &Args) -> Outcome {
    let (index, id) = args.note(host)?;
    host.read(index, |workspace| {
        let content = workspace.read(&id).map_err(text)?;
        Ok(format!(
            "путь: {}\n\n{content}",
            workspace.path_for(&id).display()
        ))
    })?
}

fn backlinks(host: &impl Host, args: &Args) -> Outcome {
    let (index, id) = args.note(host)?;
    let texts = host.read(index, |workspace| {
        workspace
            .backlink_lines(&id)
            .iter()
            .map(backlink_row)
            .collect::<Vec<_>>()
    })?;
    let rows = texts
        .into_iter()
        .map(|text| row(host, index, text))
        .collect();
    Ok(rows_or(
        rows,
        &format!("{id}: на эту заметку никто не ссылается"),
    ))
}

fn links(host: &impl Host, args: &Args) -> Outcome {
    let (index, id) = args.note(host)?;
    let texts = host.read(index, |workspace| {
        workspace
            .outgoing(&id)
            .iter()
            .map(|edge| describe(edge, &edge.to))
            .collect::<Vec<_>>()
    })?;
    let rows = texts
        .into_iter()
        .map(|text| row(host, index, text))
        .collect();
    Ok(rows_or(rows, &format!("{id}: исходящих ссылок нет")))
}

fn tasks(host: &impl Host, args: &Args) -> Outcome {
    let (picked, only) = args.listed(host)?;
    let all = args.bool("all").unwrap_or(false);
    let mut rows = Vec::new();
    for index in picked {
        let texts = host.read(index, |workspace| {
            workspace
                .tasks()
                .into_iter()
                .filter(|(id, task)| {
                    only.as_ref().is_none_or(|only| only == *id) && (all || !task.done)
                })
                .map(|(id, task)| task_row(id, task, all))
                .collect::<Vec<_>>()
        })?;
        rows.extend(texts.into_iter().map(|text| row(host, index, text)));
    }
    Ok(rows_or(
        rows,
        if all {
            "задач нет"
        } else {
            "открытых задач нет"
        },
    ))
}

fn set_task(host: &mut impl Host, args: &Args) -> Outcome {
    let (index, id) = args.note(host)?;
    let line = args.uint("line")?;
    let done = args.bool("done").ok_or("нужен логический аргумент done")?;
    host.write(index, |workspace| workspace.set_task(&id, line, done))?
        .map_err(text)?;
    host.changed(index, &id);
    let state = if done {
        "выполнена"
    } else {
        "открыта"
    };
    Ok(row(host, index, format!("задача {id}:{line} {state}")))
}

fn agent_comments(host: &impl Host, args: &Args) -> Outcome {
    let (picked, only) = args.listed(host)?;
    let all = args.bool("all").unwrap_or(false);
    let mut rows = Vec::new();
    for index in picked {
        let texts = host.read(index, |workspace| {
            workspace
                .agent_comments()
                .into_iter()
                .filter(|(id, _)| only.as_ref().is_none_or(|only| only == *id))
                .filter_map(|(id, comment)| {
                    let thread = comment
                        .id
                        .as_deref()
                        .and_then(|key| workspace.thread(key).ok().flatten());
                    let status = thread.as_ref().map_or(ThreadStatus::Open, |t| t.status);
                    if !all && !status.for_agent() && status != ThreadStatus::InProgress {
                        return None;
                    }
                    let key = comment
                        .id
                        .as_deref()
                        .map(|key| format!(" [{key}]"))
                        .unwrap_or_default();
                    // "Open" is the usual case and says nothing: only the
                    // other states are shown.
                    let state = if status == ThreadStatus::Open {
                        String::new()
                    } else {
                        format!(" ({})", status.word())
                    };
                    let place = format!("{id}:{}{key}{state}", comment.line);
                    let mut text = if comment.anchor.is_empty() {
                        format!("{place}  (на абзац): {}", comment.text)
                    } else {
                        format!("{place}  «{}»: {}", comment.anchor, comment.text)
                    };
                    // The first message is the task itself, already in the row.
                    for message in thread.iter().flat_map(|t| t.messages.iter().skip(1)) {
                        let who = if message.from == ME {
                            "человек"
                        } else {
                            &message.from
                        };
                        text.push_str(&format!("\n    {who}: {}", message.text));
                    }
                    Some(text)
                })
                .collect::<Vec<_>>()
        })?;
        rows.extend(texts.into_iter().map(|text| row(host, index, text)));
    }
    Ok(rows_or(rows, "комментариев нет"))
}

/// The name an agent gave, or a neutral word.
fn agent_name<'a>(args: &'a Args) -> &'a str {
    args.opt_str("agent")
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != ME)
        .unwrap_or("агент")
}

/// The vault and the comment of a tool call: by `id` in any vault, or by
/// `note`, `line` and `text`.
fn comment_ref(host: &impl Host, args: &Args) -> Result<(usize, CommentRef), String> {
    if let Some(key) = args.opt_str("id") {
        for index in args.vaults(host)? {
            let found = host.read(index, |workspace| {
                workspace.thread(key).ok().flatten().is_some()
                    || workspace
                        .agent_comments()
                        .iter()
                        .any(|(_, comment)| comment.id.as_deref() == Some(key))
            })?;
            if found {
                return Ok((index, CommentRef::Id(key.to_string())));
            }
        }
        return Err(format!("комментарий {key} не найден"));
    }
    let (index, note) = args.note(host)?;
    let line = args.uint("line")?;
    let text = args.str("text")?.to_string();
    Ok((index, CommentRef::Place { note, line, text }))
}

/// The note of a thread, for `Host::changed`: a reply can give the comment
/// an ID in the note.
fn thread_note(thread: &Thread) -> Option<NoteId> {
    NoteId::parse(&thread.note)
}

fn claim_comment(host: &mut impl Host, args: &Args) -> Outcome {
    let (index, target) = comment_ref(host, args)?;
    let agent = agent_name(args);
    let thread = host
        .write(index, |workspace| workspace.claim_comment(&target, agent))?
        .map_err(text)?;
    if let Some(note) = thread_note(&thread) {
        host.changed(index, &note);
    }
    Ok(row(
        host,
        index,
        format!("комментарий {} взят в работу", thread.id),
    ))
}

fn reply_comment(host: &mut impl Host, args: &Args) -> Outcome {
    let (index, target) = comment_ref(host, args)?;
    let agent = agent_name(args);
    let message = args.str("message")?;
    let question = args.bool("question").unwrap_or(false);
    let thread = host
        .write(index, |workspace| {
            workspace.reply_comment(&target, agent, message, question)
        })?
        .map_err(text)?;
    if let Some(note) = thread_note(&thread) {
        host.changed(index, &note);
    }
    let tail = if question {
        "вопрос задан, ждите ответа человека; эту задачу пока не продолжайте"
    } else {
        "ответ записан"
    };
    Ok(row(
        host,
        index,
        format!("комментарий {}: {tail}", thread.id),
    ))
}

fn resolve_comment(host: &mut impl Host, args: &Args) -> Outcome {
    let (index, target) = comment_ref(host, args)?;
    let agent = agent_name(args);
    let summary = args.opt_str("summary");
    let note = match &target {
        CommentRef::Place { note, .. } => Some(note.clone()),
        CommentRef::Id(key) => host.read(index, |workspace| {
            workspace
                .agent_comments()
                .into_iter()
                .find(|(_, comment)| comment.id.as_deref() == Some(key))
                .map(|(note, _)| note.clone())
        })?,
    };
    host.write(index, |workspace| {
        workspace.resolve_comment(&target, agent, summary)
    })?
    .map_err(text)?;
    let place = match (&target, &note) {
        (CommentRef::Place { note, line, .. }, _) => format!("{note}:{line}"),
        (CommentRef::Id(key), _) => key.clone(),
    };
    if let Some(note) = note {
        host.changed(index, &note);
    }
    Ok(row(host, index, format!("комментарий {place} снят")))
}

fn propose_edit(host: &mut impl Host, args: &Args) -> Outcome {
    let (index, note) = args.note(host)?;
    let reason = args.str("reason")?;
    let agent = agent_name(args);
    let edits = args
        .0
        .get("edits")
        .and_then(Value::as_array)
        .ok_or("нужен массив edits из объектов {old, new}")?
        .iter()
        .map(|edit| {
            let field = |key| edit.get(key).and_then(Value::as_str).map(String::from);
            field("old")
                .zip(field("new"))
                .ok_or_else(|| "у каждой правки нужны строки old и new".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let comment = args.opt_str("comment");
    let proposal = host
        .write(index, |workspace| -> AppResult<_> {
            let proposal = workspace.propose(&note, edits, reason, Some(agent), comment)?;
            if let Some(key) = comment {
                // A proposal for a comment that is not there would never close
                // a task: take it back and tell the agent.
                let target = CommentRef::Id(key.to_string());
                if let Err(error) = workspace.thread_proposed(&target, agent, &proposal) {
                    workspace.remove_proposal(&proposal.id)?;
                    return Err(error);
                }
            }
            Ok(proposal)
        })?
        .map_err(text)?;
    host.changed(index, &note);
    Ok(row(
        host,
        index,
        format!(
            "правка {} к {note} ждёт проверки человеком, изменений: {}",
            proposal.id,
            proposal.edits.len()
        ),
    ))
}

fn propose_note(host: &mut impl Host, args: &Args) -> Outcome {
    let raw = args.str("note")?;
    let note = NoteId::parse(raw).ok_or_else(|| format!("некорректное имя заметки: {raw}"))?;
    let content = args.str("content")?;
    let reason = args.str("reason")?;
    let agent = agent_name(args);
    let index = *args.vaults(host)?.first().ok_or("у сервера нет хранилищ")?;
    if host.vaults().len() > 1 && args.opt_str("vault").is_none() {
        return Err("хранилищ несколько: укажите vault".to_string());
    }
    let proposal = host
        .write(index, |workspace| {
            workspace.propose_note(&note, content, reason, Some(agent))
        })?
        .map_err(text)?;
    Ok(row(
        host,
        index,
        format!(
            "новая заметка {note} ждёт проверки человеком (правка {})",
            proposal.id
        ),
    ))
}

/// The vault that has the proposal `key`.
fn proposal_vault(host: &impl Host, args: &Args, key: &str) -> Result<usize, String> {
    for index in args.vaults(host)? {
        if host.read(index, |workspace| workspace.proposal(key).is_ok())? {
            return Ok(index);
        }
    }
    Err(format!("правка {key} не найдена"))
}

fn proposal_status(host: &impl Host, args: &Args) -> Outcome {
    let key = args.str("id")?;
    let index = proposal_vault(host, args, key)?;
    let proposal = host
        .read(index, |workspace| workspace.proposal(key))?
        .map_err(text)?;
    Ok(row(host, index, proposal.describe()))
}

fn list_proposals(host: &impl Host, args: &Args) -> Outcome {
    let (picked, only) = args.listed(host)?;
    let all = args.bool("all").unwrap_or(false);
    let mut rows = Vec::new();
    for index in picked {
        let found = host
            .read(index, |workspace| workspace.proposals())?
            .map_err(text)?;
        rows.extend(
            found
                .iter()
                .filter(|p| all || p.status() == ProposalStatus::Pending)
                .filter(|p| only.as_ref().is_none_or(|only| only.as_str() == p.note))
                .map(|p| row(host, index, p.describe())),
        );
    }
    Ok(rows_or(rows, "правок нет"))
}

fn withdraw_proposal(host: &mut impl Host, args: &Args) -> Outcome {
    let key = args.str("id")?;
    let index = proposal_vault(host, args, key)?;
    let note = host
        .write(index, |workspace| -> AppResult<_> {
            let proposal = workspace.proposal(key)?;
            workspace.remove_proposal(key)?;
            Ok(proposal.note_id())
        })?
        .map_err(text)?;
    if let Some(note) = note {
        host.changed(index, &note);
    }
    Ok(row(host, index, format!("правка {key} отозвана")))
}

/// File of the HTTP server settings in the app config directory.
pub const CONFIG_FILE: &str = "mcp.json";

/// The port when nobody chose one. Above the usual development ports, and
/// not one that a known program takes by default.
pub const DEFAULT_PORT: u16 = 7377;

/// Settings of the HTTP server in the app.
///
/// A separate file, not a field of `AppSettings`: the settings window sends
/// the whole `AppSettings` back on each change, and a copy it read earlier
/// would undo a port that the MCP float changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    /// The agent sends it as `Authorization: Bearer <token>`. Any program of
    /// any user on the machine can reach `127.0.0.1`, and the notes are
    /// private.
    pub token: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        McpConfig {
            enabled: true,
            port: DEFAULT_PORT,
            token: String::new(),
        }
    }
}

impl McpConfig {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(CONFIG_FILE)
    }

    /// Reads the settings, and writes them back if they had no token yet.
    ///
    /// No file and a broken file both give the defaults: the file has
    /// nothing a person cannot set again in two clicks. A new token means
    /// that agents set up before need the new one, so the token stays once
    /// it exists.
    pub fn load_or_create(path: &Path) -> AppResult<McpConfig> {
        let mut config = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<McpConfig>(&bytes).ok())
            .unwrap_or_default();
        if config.port == 0 {
            config.port = DEFAULT_PORT;
        }
        if config.token.is_empty() {
            config.token = fresh_token();
            config.save(path)?;
        }
        Ok(config)
    }

    /// Writes through a temporary file, like the other settings.
    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        let mut bytes = serde_json::to_vec_pretty(self).expect("настройки сериализуемы");
        bytes.push(b'\n');
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }
}

/// 128 random bits in hex.
///
/// `RandomState` takes its keys from the system random source, so a hash of
/// the time with it is unpredictable. A crate for randomness would be one
/// more dependency for one string.
fn fresh_token() -> String {
    use std::hash::{BuildHasher, Hasher};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (0..2u8)
        .map(|part| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u128(now);
            hasher.write_u8(part);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

/// Constant-time comparison: the time of a refusal must not tell how many
/// first characters of a guessed token were right.
pub fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host over workspaces in temporary folders.
    struct TestHost {
        refs: Vec<VaultRef>,
        workspaces: Vec<Workspace>,
        changed: Vec<(usize, NoteId)>,
        _dirs: Vec<tempfile::TempDir>,
    }

    impl Host for TestHost {
        fn vaults(&self) -> &[VaultRef] {
            &self.refs
        }

        fn read<R>(&self, index: usize, action: impl FnOnce(&Workspace) -> R) -> Result<R, String> {
            Ok(action(&self.workspaces[index]))
        }

        fn write<R>(
            &mut self,
            index: usize,
            action: impl FnOnce(&mut Workspace) -> R,
        ) -> Result<R, String> {
            Ok(action(&mut self.workspaces[index]))
        }

        fn changed(&mut self, index: usize, note: &NoteId) {
            self.changed.push((index, note.clone()));
        }
    }

    /// A vault for a test: name, labels, files with their text.
    type TestVault<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);

    fn host(vaults: &[TestVault]) -> TestHost {
        let mut host = TestHost {
            refs: Vec::new(),
            workspaces: Vec::new(),
            changed: Vec::new(),
            _dirs: Vec::new(),
        };
        for (name, labels, files) in vaults {
            let dir = tempfile::tempdir().unwrap();
            for (file, content) in *files {
                let path = dir.path().join(file);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
            host.refs.push(VaultRef {
                id: name.to_lowercase(),
                name: name.to_string(),
                labels: labels.iter().map(|label| label.to_string()).collect(),
            });
            host.workspaces.push(Workspace::open(dir.path()).unwrap());
            host._dirs.push(dir);
        }
        host
    }

    fn one(files: &[(&str, &str)]) -> TestHost {
        host(&[("Дом", &[], files)])
    }

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    /// The text of a tool result and whether it is an error.
    fn tool(host: &mut TestHost, name: &str, arguments: Value) -> (String, bool) {
        let reply = handle(
            host,
            &request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            ),
        )
        .unwrap();
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }

    #[test]
    fn initialize_answers_with_a_known_version_and_tools() {
        let mut host = one(&[]);

        let reply = handle(
            &mut host,
            &request("initialize", json!({ "protocolVersion": "2025-03-26" })),
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["serverInfo"]["name"], "kknow");

        let reply = handle(
            &mut host,
            &request("initialize", json!({ "protocolVersion": "1999-01-01" })),
        )
        .unwrap();
        assert_eq!(
            reply["result"]["protocolVersion"],
            VERSIONS[VERSIONS.len() - 1]
        );
    }

    #[test]
    fn a_notification_gets_no_reply() {
        let mut host = one(&[]);

        let reply = handle(
            &mut host,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );
        assert_eq!(reply, None);
    }

    #[test]
    fn an_unknown_method_is_a_protocol_error() {
        let mut host = one(&[]);

        let reply = handle(&mut host, &request("resources/list", json!({}))).unwrap();
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn every_listed_tool_can_be_called_and_has_the_vault_argument() {
        let mut host = one(&[("План.md", "# План\n")]);

        for (index, listed) in tools().as_array().unwrap().iter().enumerate() {
            let name = listed["name"].as_str().unwrap();
            let reply = handle(
                &mut host,
                &request("tools/call", json!({ "name": name, "arguments": {} })),
            )
            .unwrap();
            assert!(reply.get("result").is_some(), "{name}: {reply}");
            let has_vault = listed["inputSchema"]["properties"]["vault"].is_object();
            assert_eq!(has_vault, index > 0, "{name}");
        }
    }

    #[test]
    fn a_missing_note_is_a_tool_error_not_a_protocol_error() {
        let mut host = one(&[]);

        let (text, is_error) = tool(&mut host, "read_note", json!({ "note": "Нет" }));
        assert!(is_error);
        assert_eq!(text, "заметка не найдена: Нет");
    }

    #[test]
    fn search_read_and_links_see_the_vault() {
        let mut host = one(&[
            ("План.md", "# План\n\nсинхронизация и [[Идеи]]\n"),
            ("Идеи.md", "# Идеи\n"),
        ]);

        let (found, _) = tool(&mut host, "search", json!({ "query": "синхронизация" }));
        assert!(found.starts_with("План  (План)"), "{found}");
        assert!(found.contains("«синхронизация»"), "{found}");

        let (text, _) = tool(&mut host, "read_note", json!({ "note": "план" }));
        let path = host._dirs[0].path().canonicalize().unwrap().join("План.md");
        assert!(
            text.starts_with(&format!("путь: {}\n\n# План", path.display())),
            "{text}"
        );

        let (backlinks, _) = tool(&mut host, "backlinks", json!({ "note": "Идеи" }));
        assert_eq!(backlinks, "План:3  синхронизация и [[Идеи]]");
        let (links, _) = tool(&mut host, "links", json!({ "note": "План" }));
        assert_eq!(links, "Идеи");
    }

    #[test]
    fn list_notes_filters_by_folder() {
        let mut host = one(&[
            ("Работа/Отчёт.md", "# Итоги квартала\n"),
            ("Дом.md", "текст\n"),
        ]);

        let (all, _) = tool(&mut host, "list_notes", json!({}));
        assert_eq!(all, "Дом\nРабота/Отчёт  (Итоги квартала)");
        let (work, _) = tool(&mut host, "list_notes", json!({ "folder": "Работа/" }));
        assert_eq!(work, "Работа/Отчёт  (Итоги квартала)");
    }

    #[test]
    fn tasks_are_listed_and_set_and_the_host_hears_the_change() {
        let mut host = one(&[
            ("План.md", "# План\n\n- [ ] начать\n- [x] выбрать\n"),
            ("Дом.md", "- [ ] купить\n"),
        ]);

        let (open, _) = tool(&mut host, "tasks", json!({ "note": "План" }));
        assert_eq!(open, "План:3  начать");

        let (text, is_error) = tool(
            &mut host,
            "set_task",
            json!({ "note": "План", "line": 3, "done": true }),
        );
        assert!(!is_error, "{text}");
        assert_eq!(host.changed, [(0, NoteId::parse("План").unwrap())]);
        let (open, _) = tool(&mut host, "tasks", json!({}));
        assert_eq!(open, "Дом:1  купить");

        let (_, is_error) = tool(
            &mut host,
            "set_task",
            json!({ "note": "План", "line": 1, "done": true }),
        );
        assert!(is_error, "в заголовке нет задачи");
        assert_eq!(host.changed.len(), 1, "отказ ничего не меняет");
    }

    #[test]
    fn a_question_a_proposal_and_a_review_close_the_task() {
        let mut host = host(&[(
            "Личное",
            &[],
            &[(
                "План.md",
                "Занял 14 недель<!-- @agent «14 недель»: проверь цифру -->.\n",
            )],
        )]);
        let place =
            json!({ "note": "План", "line": 1, "text": "проверь цифру", "agent": "claude" });

        let (asked, error) = tool(
            &mut host,
            "reply_comment",
            merged(&place, json!({ "message": "12 или 14?", "question": true })),
        );
        assert!(!error, "{asked}");
        let key = host.workspaces[0].threads().unwrap()[0].id.clone();
        let (waiting, _) = tool(&mut host, "agent_comments", json!({}));
        let (everything, _) = tool(&mut host, "agent_comments", json!({ "all": true }));
        assert_eq!(waiting, "комментариев нет");
        assert!(
            everything.contains(&format!("План:1 [{key}] (вопрос)")),
            "{everything}"
        );
        assert!(everything.contains("claude: 12 или 14?"), "{everything}");

        let target = CommentRef::Id(key.clone());
        host.workspaces[0]
            .reply_comment(&target, ME, "12", false)
            .unwrap();
        let (claimed, error) = tool(&mut host, "claim_comment", json!({ "id": key }));
        assert!(!error, "{claimed}");
        let (proposed, error) = tool(
            &mut host,
            "propose_edit",
            json!({
                "note": "План",
                "edits": [{ "old": "14 недель", "new": "12 недель" }],
                "reason": "человек ответил 12",
                "comment": key,
                "agent": "claude",
            }),
        );
        assert!(!error, "{proposed}");
        let proposal = host.workspaces[0].proposals().unwrap()[0].id.clone();
        assert!(
            host.workspaces[0]
                .read(&NoteId::parse("План").unwrap())
                .unwrap()
                .contains("14 недель<")
        );

        host.workspaces[0]
            .review_proposal(
                &proposal,
                &crate::proposals::Review {
                    accept: vec![0],
                    ..Default::default()
                },
            )
            .unwrap();
        let (status, _) = tool(&mut host, "proposal_status", json!({ "id": proposal }));

        assert!(status.contains("(принято)"), "{status}");
        assert_eq!(
            host.workspaces[0]
                .read(&NoteId::parse("План").unwrap())
                .unwrap(),
            "Занял 12 недель.\n"
        );
        assert_eq!(
            host.workspaces[0].thread(&key).unwrap().unwrap().status,
            ThreadStatus::Done
        );
    }

    #[test]
    fn a_bad_edit_is_an_error_for_the_agent() {
        let mut host = host(&[("Личное", &[], &[("План.md", "да и да\n")])]);

        let (reply, error) = tool(
            &mut host,
            "propose_edit",
            json!({ "note": "План", "edits": [{ "old": "да", "new": "нет" }], "reason": "" }),
        );

        assert!(error);
        assert!(reply.contains("несколько раз"), "{reply}");
    }

    #[test]
    fn agent_comments_are_found_and_resolved() {
        let mut host = one(&[(
            "План.md",
            "# План\n\nПоиск быстрый<!-- @agent «быстрый»: замерить -->.\n\nАбзац.<!-- @agent: сократить -->\n",
        )]);

        let (found, _) = tool(&mut host, "agent_comments", json!({}));
        assert_eq!(
            found,
            "План:3  «быстрый»: замерить\nПлан:5  (на абзац): сократить"
        );

        let (text, is_error) = tool(
            &mut host,
            "resolve_comment",
            json!({ "note": "План", "line": 3, "text": "замерить" }),
        );
        assert!(!is_error, "{text}");
        assert_eq!(
            fs::read_to_string(host._dirs[0].path().join("План.md")).unwrap(),
            "# План\n\nПоиск быстрый.\n\nАбзац.<!-- @agent: сократить -->\n"
        );
        let (found, _) = tool(&mut host, "agent_comments", json!({ "note": "План" }));
        assert_eq!(found, "План:5  (на абзац): сократить");
    }

    fn two_vaults() -> TestHost {
        host(&[
            (
                "Работа",
                &["work"],
                &[("План.md", "- [ ] отчёт<!-- @agent: проверить цифры -->\n")],
            ),
            (
                "Дом",
                &[],
                &[("План.md", "- [ ] купить\n"), ("Идеи.md", "")],
            ),
        ])
    }

    #[test]
    fn with_several_vaults_each_row_names_its_vault() {
        let mut host = two_vaults();

        let (list, _) = tool(&mut host, "list_vaults", json!({}));
        let lines: Vec<&str> = list.lines().collect();
        assert!(lines[0].starts_with("Работа  заметок: 1  "), "{list}");
        assert!(lines[0].ends_with("  #work"), "{list}");
        assert!(lines[1].starts_with("Дом  заметок: 2  "), "{list}");

        let (tasks, _) = tool(&mut host, "tasks", json!({}));
        assert_eq!(tasks, "[Работа] План:1  отчёт\n[Дом] План:1  купить");
        let (tasks, _) = tool(&mut host, "tasks", json!({ "vault": "дом" }));
        assert_eq!(tasks, "[Дом] План:1  купить");

        let (comments, _) = tool(&mut host, "agent_comments", json!({}));
        assert_eq!(comments, "[Работа] План:1  (на абзац): проверить цифры");
    }

    #[test]
    fn a_note_in_two_vaults_needs_the_vault() {
        let mut host = two_vaults();

        let (text, is_error) = tool(&mut host, "read_note", json!({ "note": "План" }));
        assert!(is_error);
        assert_eq!(
            text,
            "заметка План есть в нескольких хранилищах: Работа, Дом; укажите vault"
        );

        let (text, is_error) = tool(
            &mut host,
            "read_note",
            json!({ "note": "План", "vault": "Дом" }),
        );
        assert!(!is_error, "{text}");
        assert!(text.ends_with("- [ ] купить\n"), "{text}");

        // A unique note needs no vault.
        let (_, is_error) = tool(&mut host, "read_note", json!({ "note": "Идеи" }));
        assert!(!is_error);

        let (text, is_error) = tool(&mut host, "tasks", json!({ "vault": "Офис" }));
        assert!(is_error);
        assert_eq!(text, "хранилище не найдено: Офис; есть: Работа, Дом");
    }

    #[test]
    fn config_gets_a_token_once_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = McpConfig::path(dir.path());

        let first = McpConfig::load_or_create(&path).unwrap();
        assert_eq!(first.port, DEFAULT_PORT);
        assert!(first.enabled);
        assert_eq!(first.token.len(), 32);
        assert!(first.token.chars().all(|ch| ch.is_ascii_hexdigit()));

        let again = McpConfig::load_or_create(&path).unwrap();
        assert_eq!(again, first, "токен не меняется");

        fs::write(&path, "не json").unwrap();
        let repaired = McpConfig::load_or_create(&path).unwrap();
        assert_ne!(repaired.token, first.token);
        assert_eq!(repaired.port, DEFAULT_PORT);
    }

    #[test]
    fn tokens_compare_whole() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abd", "abc"));
        assert!(!same_token("ab", "abc"));
        assert!(!same_token("", "abc"));
    }
}
