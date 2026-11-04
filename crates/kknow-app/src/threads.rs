//! Comment threads: the talk between a person and an agent about one
//! `@agent` comment.
//!
//! Before threads, an agent had two choices: guess or do nothing. Now it can
//! ask ("the report says 14, the letter says 12: which one?"), the person
//! answers, and the agent goes on.
//!
//! The task stays in the note (`<!-- @agent#k3f «words»: task -->`), so an
//! agent without MCP still sees it. Everything else is in
//! `.kknow/threads/k3f.json`: the messages and the state. One line in the
//! note cannot hold a talk: an empty line would break the paragraph, and each
//! reply would be noise in a git diff.
//!
//! A comment gets its ID with the first event of its thread. A comment that
//! nobody touched has no thread and is open.

use serde::{Deserialize, Serialize};

use kknow_core::NoteId;
use kknow_index::AgentComment;
use kknow_index::agent_note::with_id;
use kknow_index::parse::{
    agent_answers, remove_agent_comment, remove_agent_comment_by_id, set_agent_comment_id,
};

use crate::meta::{RecordDir, fresh_id, now};
use crate::proposals::{Proposal, Status as ProposalStatus};
use crate::{Error, Result, Workspace};

/// The folder of threads, relative to the vault root.
pub const THREADS_DIR: &str = ".kknow/threads";

/// The author of the messages of the person. Any other author is an agent.
pub const ME: &str = "me";

/// The author of answers that an agent wrote in the text: the text does not
/// say which agent it was.
pub const AGENT: &str = "агент";

/// A claim older than this is free again: the agent that took the task
/// probably stopped. Thirty minutes is longer than any task of one comment.
const CLAIM_TIMEOUT: u64 = 30 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ThreadStatus"))]
pub enum Status {
    /// Nobody works on the task.
    Open,
    /// An agent took the task.
    InProgress,
    /// The agent asked and waits for the person.
    Question,
    /// The person answered; the agent can go on.
    Answered,
    /// The agent made a proposal; it waits for review.
    Review,
    /// The task is done, the comment is gone from the note.
    Done,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Open => "открыт",
            Status::InProgress => "в работе",
            Status::Question => "вопрос",
            Status::Answered => "есть ответ",
            Status::Review => "на проверке",
            Status::Done => "готово",
        }
    }

    /// An agent can take the task: nobody works on it and nobody waits for
    /// the person.
    pub fn for_agent(self) -> bool {
        matches!(self, Status::Open | Status::Answered)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ThreadMessage"))]
pub struct Message {
    /// [`ME`] for the person, else the name of the agent.
    pub from: String,
    pub text: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Thread {
    pub id: String,
    /// Note ID ([`NoteId::as_str`]).
    pub note: String,
    /// The words of the comment, as it was when the thread started.
    #[serde(default)]
    pub anchor: String,
    /// The task, as it was when the thread started.
    pub task: String,
    pub status: Status,
    #[serde(default)]
    pub messages: Vec<Message>,
    /// The proposal that answers the task, while it waits for review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub proposal: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated: u64,
}

impl Thread {
    fn say(&mut self, from: &str, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.messages.push(Message {
                from: from.to_string(),
                text: text.to_string(),
                at: now(),
            });
        }
        self.updated = now();
    }
}

/// Which comment a tool means: by thread ID, or by place for a comment
/// without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentRef {
    Id(String),
    Place {
        note: NoteId,
        line: usize,
        text: String,
    },
}

impl Workspace {
    fn thread_dir(&self) -> RecordDir {
        RecordDir::new(self.root(), THREADS_DIR)
    }

    /// All threads, the last changed first.
    pub fn threads(&self) -> Result<Vec<Thread>> {
        let mut all: Vec<Thread> = self.thread_dir().all()?;
        all.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        Ok(all)
    }

    pub fn thread(&self, id: &str) -> Result<Option<Thread>> {
        self.thread_dir().get(id)
    }

    /// Threads of the comments in one note, for the window.
    pub fn note_threads(&self, note: &NoteId) -> Result<Vec<Thread>> {
        let Some(parsed) = self.parsed(note) else {
            return Ok(Vec::new());
        };
        let mut threads = Vec::new();
        for id in parsed.agent_comments.iter().filter_map(|c| c.id.as_deref()) {
            if let Some(thread) = self.thread(id)? {
                threads.push(thread);
            }
        }
        Ok(threads)
    }

    /// The state of a comment: of its thread, or open without one.
    pub fn comment_status(&self, comment: &AgentComment) -> Status {
        comment
            .id
            .as_deref()
            .and_then(|id| self.thread(id).ok().flatten())
            .map_or(Status::Open, |thread| thread.status)
    }

    /// Finds the comment and its thread, and makes both if needed: a comment
    /// without an ID gets one in the note, and a thread without a file gets
    /// one with the task as the first message.
    fn open_thread(&mut self, target: &CommentRef) -> Result<Thread> {
        let (note, comment) = match target {
            CommentRef::Id(id) => {
                if let Some(thread) = self.thread(id)? {
                    return Ok(thread);
                }
                let (note, comment) = self
                    .agent_comments()
                    .into_iter()
                    .find(|(_, comment)| comment.id.as_deref() == Some(id))
                    .map(|(note, comment)| (note.clone(), comment.clone()))
                    .ok_or_else(|| Error::UnknownComment(id.clone()))?;
                (note, comment)
            }
            CommentRef::Place { note, line, text } => {
                let content = self.read(note)?;
                let parsed = kknow_index::parse::parse(&content);
                let comment = parsed
                    .agent_comments
                    .into_iter()
                    .find(|comment| comment.line == *line && comment.text == text.trim())
                    .ok_or_else(|| {
                        Error::NotAnAgentComment(note.clone(), *line, text.trim().to_string())
                    })?;
                if let Some(id) = &comment.id {
                    if let Some(thread) = self.thread(id)? {
                        return Ok(thread);
                    }
                } else {
                    let mut id = fresh_id();
                    while self.thread(&id)?.is_some() {
                        id = fresh_id();
                    }
                    let changed =
                        set_agent_comment_id(&content, *line, text, &id).ok_or_else(|| {
                            Error::NotAnAgentComment(note.clone(), *line, text.trim().to_string())
                        })?;
                    self.save(note, &changed)?;
                    let mut comment = comment;
                    comment.id = Some(id);
                    return self.new_thread(note, &comment);
                }
                (note.clone(), comment)
            }
        };
        self.new_thread(&note, &comment)
    }

    /// Moves the answers that an agent wrote in the text into the threads.
    ///
    /// An agent without MCP (the `agent` command in the `edit` mode) can only
    /// edit the file, so it asks with `<!-- @agent-note: … -->` just after
    /// the comment. In the text such a question is noise, and the person
    /// cannot answer it there. Here it becomes a question in the thread of
    /// the comment: the comments panel shows it with a reply field, and the
    /// note text stays clean. Returns `false` when the note has no answers.
    pub fn adopt_agent_answers(&mut self, note: &NoteId) -> Result<bool> {
        let content = self.read(note)?;
        let answers = agent_answers(&content);
        if answers.is_empty() {
            return Ok(false);
        }
        let mut text = content.clone();
        let mut threads = Vec::new();
        // From the end: a cut does not move the offsets before it.
        for answer in answers.into_iter().rev() {
            text.replace_range(answer.span.clone(), "");
            let mut comment = answer.comment;
            if comment.id.is_none() {
                let mut id = fresh_id();
                while self.thread(&id)?.is_some() || threads.iter().any(|t: &Thread| t.id == id) {
                    id = fresh_id();
                }
                let marked = with_id(&text[comment.span.clone()], &id)
                    .expect("the span holds the comment");
                text.replace_range(comment.span.clone(), &marked);
                comment.id = Some(id);
            }
            let id = comment.id.clone().expect("the comment has an ID");
            let mut thread = match self.thread(&id)? {
                Some(thread) => thread,
                None => self.thread_value(note, &comment),
            };
            thread.say(AGENT, &answer.text);
            thread.status = Status::Question;
            threads.push(thread);
        }
        // The note first: a thread without its comment ID would be lost.
        self.save(note, &text)?;
        for thread in &threads {
            self.put_thread(thread)?;
        }
        Ok(true)
    }

    fn new_thread(&mut self, note: &NoteId, comment: &AgentComment) -> Result<Thread> {
        let thread = self.thread_value(note, comment);
        self.thread_dir().put(&thread.id, &thread)?;
        Ok(thread)
    }

    fn thread_value(&self, note: &NoteId, comment: &AgentComment) -> Thread {
        let id = comment.id.clone().expect("у комментария уже есть ID");
        let time = now();
        Thread {
            id,
            note: note.as_str().to_string(),
            anchor: comment.anchor.clone(),
            task: comment.text.clone(),
            status: Status::Open,
            messages: vec![Message {
                from: ME.to_string(),
                text: comment.text.clone(),
                at: time,
            }],
            proposal: None,
            created: time,
            updated: time,
        }
    }

    fn put_thread(&self, thread: &Thread) -> Result<()> {
        self.thread_dir().put(&thread.id, thread)
    }

    /// An agent takes the task. A second agent gets an error while the first
    /// one works, so two agents do not do the same task.
    pub fn claim_comment(&mut self, target: &CommentRef, agent: &str) -> Result<Thread> {
        let mut thread = self.open_thread(target)?;
        let busy = match thread.status {
            Status::InProgress => now().saturating_sub(thread.updated) < CLAIM_TIMEOUT,
            Status::Question | Status::Review | Status::Done => true,
            Status::Open | Status::Answered => false,
        };
        if busy {
            return Err(Error::CommentBusy(thread.id.clone(), thread.status.word()));
        }
        thread.status = Status::InProgress;
        // The claim is not a message: it only moves the time of the thread,
        // which the claim timeout counts from.
        thread.say(agent, "");
        self.put_thread(&thread)?;
        Ok(thread)
    }

    /// A message in the thread.
    ///
    /// From an agent: `question` makes the thread wait for the person, else
    /// it is a note on the way and the agent goes on. From the person
    /// ([`ME`]): an answer to a question, so the agent can go on.
    pub fn reply_comment(
        &mut self,
        target: &CommentRef,
        from: &str,
        text: &str,
        question: bool,
    ) -> Result<Thread> {
        if text.trim().is_empty() {
            return Err(Error::EmptyMessage);
        }
        let mut thread = self.open_thread(target)?;
        if thread.status == Status::Done {
            return Err(Error::CommentBusy(thread.id.clone(), thread.status.word()));
        }
        thread.say(from, text);
        thread.status = if from == ME {
            match thread.status {
                Status::Question | Status::InProgress => Status::Answered,
                other => other,
            }
        } else if question {
            Status::Question
        } else {
            Status::InProgress
        };
        self.put_thread(&thread)?;
        Ok(thread)
    }

    /// The task is done: the comment goes away from the note, the thread
    /// keeps the summary and stays as history.
    pub fn resolve_comment(
        &mut self,
        target: &CommentRef,
        from: &str,
        summary: Option<&str>,
    ) -> Result<String> {
        let (note, changed) = match target {
            CommentRef::Id(id) => {
                let note = self
                    .agent_comments()
                    .into_iter()
                    .find(|(_, comment)| comment.id.as_deref() == Some(id))
                    .map(|(note, _)| note.clone())
                    .ok_or_else(|| Error::UnknownComment(id.clone()))?;
                let content = self.read(&note)?;
                let changed = remove_agent_comment_by_id(&content, id)
                    .ok_or_else(|| Error::UnknownComment(id.clone()))?;
                (note, changed)
            }
            CommentRef::Place { note, line, text } => {
                let content = self.read(note)?;
                let changed = remove_agent_comment(&content, *line, text).ok_or_else(|| {
                    Error::NotAnAgentComment(note.clone(), *line, text.trim().to_string())
                })?;
                (note.clone(), changed)
            }
        };

        let thread_id = match target {
            CommentRef::Id(id) => Some(id.clone()),
            CommentRef::Place { note, line, text } => self
                .parsed(note)
                .and_then(|parsed| {
                    parsed
                        .agent_comments
                        .iter()
                        .find(|comment| comment.line == *line && comment.text == text.trim())
                })
                .and_then(|comment| comment.id.clone()),
        };
        self.save(&note, &changed)?;

        if let Some(mut thread) = thread_id.and_then(|id| self.thread(&id).ok().flatten()) {
            thread.say(from, summary.unwrap_or(""));
            thread.status = Status::Done;
            thread.proposal = None;
            self.put_thread(&thread)?;
        }
        Ok(changed)
    }

    /// Links a proposal to the thread: the task waits for review now.
    pub fn thread_proposed(
        &mut self,
        target: &CommentRef,
        agent: &str,
        proposal: &Proposal,
    ) -> Result<Thread> {
        let mut thread = self.open_thread(target)?;
        thread.say(
            agent,
            &format!("предложил правку {}: {}", proposal.id, proposal.reason),
        );
        thread.status = Status::Review;
        thread.proposal = Some(proposal.id.clone());
        self.put_thread(&thread)?;
        Ok(thread)
    }

    /// After a review: an accepted proposal closes the task, a rejected one
    /// gives it back to the agent with the reason.
    pub(crate) fn proposal_reviewed(&mut self, proposal: &Proposal) -> Result<()> {
        let Some(id) = proposal.comment.as_deref() else {
            return Ok(());
        };
        let status = proposal.status();
        if status == ProposalStatus::Pending {
            return Ok(());
        }
        let Some(mut thread) = self.thread(id)? else {
            return Ok(());
        };
        if thread.status == Status::Done {
            return Ok(());
        }
        let feedback = proposal.feedback.as_deref().unwrap_or("");
        if status == ProposalStatus::Rejected {
            let text = if feedback.is_empty() {
                format!("правка {} отклонена", proposal.id)
            } else {
                format!("правка {} отклонена: {feedback}", proposal.id)
            };
            thread.say(ME, &text);
            thread.status = Status::Answered;
            thread.proposal = None;
            self.put_thread(&thread)?;
            return Ok(());
        }
        // Accepted, fully or partly: the task is done. The comment may be gone
        // already (the accepted edit removed it), and that is fine.
        let target = CommentRef::Id(id.to_string());
        let text = if feedback.is_empty() {
            format!("правка {}: {}", proposal.id, status.word())
        } else {
            format!("правка {}: {}; {feedback}", proposal.id, status.word())
        };
        if self.resolve_comment(&target, ME, Some(&text)).is_err() {
            thread.say(ME, &text);
            thread.status = Status::Done;
            thread.proposal = None;
            self.put_thread(&thread)?;
        }
        Ok(())
    }

    /// Threads follow their notes on a rename.
    pub(crate) fn move_threads(&self, moves: &[(NoteId, NoteId)]) -> Result<()> {
        let dir = self.thread_dir();
        for mut thread in dir.all::<Thread>()? {
            if let Some((_, to)) = moves.iter().find(|(from, _)| from.as_str() == thread.note) {
                thread.note = to.as_str().to_string();
                dir.put(&thread.id.clone(), &thread)?;
            }
        }
        Ok(())
    }

    /// Notes whose threads or proposals are in these shared files. After a
    /// sync the window redraws them: their comments may have new answers.
    pub fn notes_of_shared(&self, keys: &[String]) -> Vec<NoteId> {
        let mut notes = Vec::new();
        for key in keys {
            let id = key
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"));
            let note = match id {
                Some(id) if key.starts_with(THREADS_DIR) => {
                    self.thread(id).ok().flatten().map(|thread| thread.note)
                }
                Some(id) if key.starts_with(crate::proposals::PROPOSALS_DIR) => {
                    self.proposal(id).ok().map(|proposal| proposal.note)
                }
                _ => None,
            };
            if let Some(note) = note.as_deref().and_then(NoteId::parse) {
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
        }
        notes
    }

    /// Removes a thread whose comment is gone, or a done one.
    pub fn remove_thread(&mut self, id: &str) -> Result<()> {
        self.thread_dir().remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposals::Review;
    use std::fs;

    const NOTE: &str = "Проект занял 14 недель<!-- @agent «14 недель»: проверь цифру -->.\n";

    fn vault() -> (tempfile::TempDir, Workspace, NoteId) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Запуск.md"), NOTE).unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace, NoteId::parse("Запуск").unwrap())
    }

    fn place(note: &NoteId) -> CommentRef {
        CommentRef::Place {
            note: note.clone(),
            line: 1,
            text: "проверь цифру".to_string(),
        }
    }

    #[test]
    fn the_first_reply_gives_the_comment_an_id() {
        let (_dir, mut workspace, note) = vault();

        let thread = workspace
            .reply_comment(
                &place(&note),
                "claude",
                "В отчёте 14, в письме 12. Какой источник?",
                true,
            )
            .unwrap();

        let content = workspace.read(&note).unwrap();
        assert!(content.contains(&format!(
            "<!-- @agent#{} «14 недель»: проверь цифру -->",
            thread.id
        )));
        assert_eq!(thread.status, Status::Question);
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].from, ME);
        assert_eq!(workspace.note_threads(&note).unwrap(), vec![thread]);
    }

    #[test]
    fn an_answer_in_the_text_moves_into_the_thread() {
        let dir = tempfile::tempdir().unwrap();
        let text = "Demo zero<!-- @agent «zero»: start for one -->\n<!-- @agent-note: VARIANTS: 1) a 2) b -->\nDemo one\n";
        fs::write(dir.path().join("Demo.md"), text).unwrap();
        let mut workspace = Workspace::open(dir.path()).unwrap();
        let note = NoteId::parse("Demo").unwrap();

        assert!(workspace.adopt_agent_answers(&note).unwrap());

        let threads = workspace.note_threads(&note).unwrap();
        assert_eq!(threads.len(), 1);
        let thread = &threads[0];
        assert_eq!(thread.status, Status::Question);
        assert_eq!(thread.messages[1].from, AGENT);
        assert_eq!(thread.messages[1].text, "VARIANTS: 1) a 2) b");
        assert_eq!(
            workspace.read(&note).unwrap(),
            format!("Demo zero<!-- @agent#{} «zero»: start for one -->\nDemo one\n", thread.id)
        );

        // The person answers, and the next answer of the agent joins the thread.
        workspace
            .reply_comment(&CommentRef::Id(thread.id.clone()), ME, "variant 2", false)
            .unwrap();
        let again = workspace.read(&note).unwrap().replace(
            "-->\nDemo one",
            "-->\n<!-- @agent-note: and the title? -->\nDemo one",
        );
        workspace.save(&note, &again).unwrap();
        assert!(workspace.adopt_agent_answers(&note).unwrap());
        let thread = workspace.thread(&thread.id).unwrap().unwrap();
        assert_eq!(thread.messages.len(), 4);
        assert_eq!(thread.status, Status::Question);
        assert!(!workspace.adopt_agent_answers(&note).unwrap());
    }

    #[test]
    fn an_answer_gives_the_task_back_to_the_agent() {
        let (_dir, mut workspace, note) = vault();
        let thread = workspace
            .reply_comment(&place(&note), "claude", "Какой источник?", true)
            .unwrap();
        let by_id = CommentRef::Id(thread.id.clone());

        let thread = workspace.reply_comment(&by_id, ME, "отчёт", false).unwrap();

        assert_eq!(thread.status, Status::Answered);
        assert!(thread.status.for_agent());
    }

    #[test]
    fn a_second_agent_cannot_take_a_claimed_task() {
        let (_dir, mut workspace, note) = vault();
        let thread = workspace.claim_comment(&place(&note), "claude").unwrap();

        let second = workspace.claim_comment(&CommentRef::Id(thread.id.clone()), "copilot");

        assert_eq!(thread.status, Status::InProgress);
        assert!(matches!(second, Err(Error::CommentBusy(_, _))));
    }

    #[test]
    fn resolve_removes_the_comment_and_keeps_the_history() {
        let (_dir, mut workspace, note) = vault();
        let thread = workspace.claim_comment(&place(&note), "claude").unwrap();

        workspace
            .resolve_comment(
                &CommentRef::Id(thread.id.clone()),
                "claude",
                Some("цифра верна"),
            )
            .unwrap();

        assert_eq!(workspace.read(&note).unwrap(), "Проект занял 14 недель.\n");
        let thread = workspace.thread(&thread.id).unwrap().unwrap();
        assert_eq!(thread.status, Status::Done);
        assert_eq!(thread.messages.last().unwrap().text, "цифра верна");
    }

    #[test]
    fn resolve_by_place_still_works_without_a_thread() {
        let (_dir, mut workspace, note) = vault();

        workspace
            .resolve_comment(&place(&note), "claude", None)
            .unwrap();

        assert_eq!(workspace.read(&note).unwrap(), "Проект занял 14 недель.\n");
        assert!(workspace.threads().unwrap().is_empty());
    }

    #[test]
    fn an_accepted_proposal_closes_the_task() {
        let (_dir, mut workspace, note) = vault();
        let thread = workspace.claim_comment(&place(&note), "claude").unwrap();
        let target = CommentRef::Id(thread.id.clone());
        let proposal = workspace
            .propose(
                &note,
                vec![("14 недель".to_string(), "12 недель".to_string())],
                "по отчёту",
                Some("claude"),
                Some(&thread.id),
            )
            .unwrap();
        workspace
            .thread_proposed(&target, "claude", &proposal)
            .unwrap();

        workspace
            .review_proposal(
                &proposal.id,
                &Review {
                    accept: vec![0],
                    ..Review::default()
                },
            )
            .unwrap();

        let content = workspace.read(&note).unwrap();
        assert!(content.starts_with("Проект занял 12 недель"));
        assert!(!content.contains("@agent"));
        assert_eq!(
            workspace.thread(&thread.id).unwrap().unwrap().status,
            Status::Done
        );
    }

    #[test]
    fn a_rejected_proposal_gives_the_task_back_with_the_reason() {
        let (_dir, mut workspace, note) = vault();
        let thread = workspace.claim_comment(&place(&note), "claude").unwrap();
        let proposal = workspace
            .propose(
                &note,
                vec![("14 недель".to_string(), "12 недель".to_string())],
                "",
                None,
                Some(&thread.id),
            )
            .unwrap();
        workspace
            .thread_proposed(&CommentRef::Id(thread.id.clone()), "claude", &proposal)
            .unwrap();

        workspace
            .review_proposal(
                &proposal.id,
                &Review {
                    reject: vec![0],
                    feedback: Some("в отчёте 14".to_string()),
                    ..Review::default()
                },
            )
            .unwrap();

        let thread = workspace.thread(&thread.id).unwrap().unwrap();
        assert_eq!(thread.status, Status::Answered);
        assert!(thread.messages.last().unwrap().text.contains("в отчёте 14"));
        assert!(workspace.read(&note).unwrap().contains("@agent#"));
    }
}
