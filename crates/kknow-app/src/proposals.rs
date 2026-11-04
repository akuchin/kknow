//! Proposals: edits that an agent suggests and a person accepts or rejects.
//!
//! An agent without file access (Claude Desktop, ChatGPT) cannot change a
//! note, and an agent with file access changes it without a question. A
//! proposal is the middle way: the note stays as it is until the person says
//! yes to each edit.
//!
//! A proposal lives in `.kknow/proposals/<id>.json`, not in the note. Inline
//! marks (CriticMarkup `{~~old~>new~~}`) would make the note dirty in other
//! editors, and a large rewrite in that form is not readable.
//!
//! An edit finds its place by text, not by line: `old` must occur exactly once
//! in the note. This is the rule of the Edit tool of coding agents, so they
//! already know it. Lines move when the person types above; the text stays.
//! At review time the text is searched again. An edit whose `old` is gone or
//! occurs twice is stale, and kknow does not guess where it goes.

use serde::{Deserialize, Serialize};

use kknow_core::NoteId;

use crate::meta::{RecordDir, fresh_id, now};
use crate::{Error, Result, Workspace};

/// The folder of proposals, relative to the vault root.
pub const PROPOSALS_DIR: &str = ".kknow/proposals";

/// The decision on one edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EditState {
    Pending,
    Accepted,
    Rejected,
}

/// One replacement: `old` becomes `new`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ProposalEdit"))]
pub struct Edit {
    pub old: String,
    pub new: String,
    pub state: EditState,
}

/// The state of the whole proposal. It is computed from the edits, not
/// stored: a stored copy could disagree with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "ProposalStatus"))]
pub enum Status {
    /// At least one edit waits for a decision.
    Pending,
    Accepted,
    Rejected,
    /// Some edits accepted, some rejected.
    Partial,
}

impl Status {
    /// The word for the CLI and the agent.
    pub fn word(self) -> &'static str {
        match self {
            Status::Pending => "ждёт",
            Status::Accepted => "принято",
            Status::Rejected => "отклонено",
            Status::Partial => "принято частично",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Proposal {
    pub id: String,
    /// Note ID ([`NoteId::as_str`]).
    pub note: String,
    /// Why the agent suggests this, in its words.
    pub reason: String,
    /// The program that made the proposal, if it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub author: Option<String>,
    /// The comment thread that this proposal answers. Its review closes or
    /// opens the thread again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub comment: Option<String>,
    /// The note does not exist yet: the only edit has an empty `old` and the
    /// whole text in `new`.
    #[serde(default)]
    pub create: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated: u64,
    pub edits: Vec<Edit>,
    /// What the person said on review. The agent reads it with
    /// `proposal_status` and tries again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub feedback: Option<String>,
}

impl Proposal {
    pub fn status(&self) -> Status {
        let count = |state| self.edits.iter().filter(|edit| edit.state == state).count();
        if count(EditState::Pending) > 0 {
            Status::Pending
        } else if count(EditState::Rejected) == 0 {
            Status::Accepted
        } else if count(EditState::Accepted) == 0 {
            Status::Rejected
        } else {
            Status::Partial
        }
    }

    pub fn note_id(&self) -> Option<NoteId> {
        NoteId::parse(&self.note)
    }

    /// The proposal in rows for the CLI and the agent: the head, then each
    /// edit with its state, then the feedback of the person.
    pub fn describe(&self) -> String {
        let mut text = format!(
            "{} {} ({}): {}",
            self.id,
            self.note,
            self.status().word(),
            self.reason
        );
        for (number, edit) in self.edits.iter().enumerate() {
            let state = match edit.state {
                EditState::Pending => "ждёт",
                EditState::Accepted => "принято",
                EditState::Rejected => "отклонено",
            };
            let old = short(&edit.old);
            let new = short(&edit.new);
            text.push_str(&format!("\n    {}. {state}: «{old}» → «{new}»", number + 1));
        }
        if let Some(feedback) = &self.feedback {
            text.push_str(&format!("\n    отзыв человека: {feedback}"));
        }
        text
    }
}

/// One line of at most 80 characters: an edit can be a whole paragraph.
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    }
}

/// A proposal for the window: the record and, for each edit, whether its
/// place is still in the note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProposalView {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub proposal: Proposal,
    pub status: Status,
    /// `stale[i]` is true when edit `i` waits and its `old` is not exactly
    /// once in the note now.
    pub stale: Vec<bool>,
}

/// What a person decided in one review.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Review {
    /// Indexes of the edits to apply.
    #[serde(default)]
    pub accept: Vec<usize>,
    /// Indexes of the edits to drop.
    #[serde(default)]
    pub reject: Vec<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub feedback: Option<String>,
}

/// Where `old` is in `text`: the only match, or why there is none.
///
/// A match inside an `@agent` comment does not count. The comment repeats
/// the words it is about (`14 weeks<!-- @agent «14 weeks»: … -->`), so
/// without this rule each commented phrase would occur twice.
fn locate(text: &str, old: &str) -> std::result::Result<usize, &'static str> {
    let comments: Vec<std::ops::Range<usize>> = kknow_index::parse::parse(text)
        .agent_comments
        .into_iter()
        .map(|comment| comment.span)
        .collect();
    let mut matches = text
        .match_indices(old)
        .filter(|(at, _)| !comments.iter().any(|span| span.contains(at)));
    match (matches.next(), matches.next()) {
        (Some((at, _)), None) => Ok(at),
        (None, _) => Err("этого текста нет в заметке"),
        (Some(_), Some(_)) => {
            Err("этот текст встречается в заметке несколько раз, возьми больше слов вокруг")
        }
    }
}

/// Checks new edits against the note before they are saved. The agent gets
/// the error at once, not the person at review.
fn check(text: &str, edits: &[(String, String)]) -> std::result::Result<(), String> {
    if edits.is_empty() {
        return Err("нужна хотя бы одна правка".to_string());
    }
    let mut spans = Vec::new();
    for (index, (old, new)) in edits.iter().enumerate() {
        let number = index + 1;
        if old.is_empty() {
            return Err(format!("правка {number}: пустой old"));
        }
        if old == new {
            return Err(format!("правка {number}: old и new одинаковые"));
        }
        let at = locate(text, old).map_err(|why| format!("правка {number}: {why}"))?;
        spans.push((at, at + old.len(), number));
    }
    spans.sort();
    for pair in spans.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(format!(
                "правки {} и {} задевают один и тот же текст: объедини их",
                pair[0].2, pair[1].2
            ));
        }
    }
    Ok(())
}

/// Applies the edits one after another. Each one is found again in the text
/// that the previous ones made. `Err` gives the index of the first edit
/// without a place.
fn apply<'a>(
    text: &str,
    edits: impl Iterator<Item = (usize, &'a Edit)>,
) -> std::result::Result<String, usize> {
    let mut out = text.to_string();
    for (index, edit) in edits {
        let at = locate(&out, &edit.old).map_err(|_| index)?;
        out.replace_range(at..at + edit.old.len(), &edit.new);
    }
    Ok(out)
}

impl Workspace {
    fn proposal_dir(&self) -> RecordDir {
        RecordDir::new(self.root(), PROPOSALS_DIR)
    }

    /// Saves a proposal of edits to an existing note.
    ///
    /// Each `old` must occur exactly once in the note now, and two edits must
    /// not touch the same text. Otherwise nothing is saved and the error
    /// tells the agent what to fix.
    pub fn propose(
        &mut self,
        id: &NoteId,
        edits: Vec<(String, String)>,
        reason: &str,
        author: Option<&str>,
        comment: Option<&str>,
    ) -> Result<Proposal> {
        let content = self.read(id)?;
        check(&content, &edits).map_err(Error::BadProposal)?;
        let edits = edits
            .into_iter()
            .map(|(old, new)| Edit {
                old,
                new,
                state: EditState::Pending,
            })
            .collect();
        self.store_proposal(id, edits, reason, author, comment, false)
    }

    /// Saves a proposal of a new note with this text.
    pub fn propose_note(
        &mut self,
        id: &NoteId,
        content: &str,
        reason: &str,
        author: Option<&str>,
    ) -> Result<Proposal> {
        if self.contains(id) {
            return Err(Error::AlreadyExists(id.clone()));
        }
        let edits = vec![Edit {
            old: String::new(),
            new: content.to_string(),
            state: EditState::Pending,
        }];
        self.store_proposal(id, edits, reason, author, None, true)
    }

    fn store_proposal(
        &mut self,
        id: &NoteId,
        edits: Vec<Edit>,
        reason: &str,
        author: Option<&str>,
        comment: Option<&str>,
        create: bool,
    ) -> Result<Proposal> {
        let dir = self.proposal_dir();
        let mut key = fresh_id();
        while dir.get::<Proposal>(&key)?.is_some() {
            key = fresh_id();
        }
        let time = now();
        let proposal = Proposal {
            id: key,
            note: id.as_str().to_string(),
            reason: reason.trim().to_string(),
            author: author
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(String::from),
            comment: comment.map(String::from),
            create,
            created: time,
            updated: time,
            edits,
            feedback: None,
        };
        dir.put(&proposal.id, &proposal)?;
        Ok(proposal)
    }

    /// All proposals of the vault, newest first.
    pub fn proposals(&self) -> Result<Vec<Proposal>> {
        let mut all: Vec<Proposal> = self.proposal_dir().all()?;
        all.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id)));
        Ok(all)
    }

    pub fn proposal(&self, key: &str) -> Result<Proposal> {
        self.proposal_dir()
            .get(key)?
            .ok_or_else(|| Error::UnknownProposal(key.to_string()))
    }

    /// The proposal with the stale marks for the window.
    pub fn proposal_view(&self, proposal: Proposal) -> ProposalView {
        let content = proposal.note_id().and_then(|id| self.read(&id).ok());
        let stale = proposal
            .edits
            .iter()
            .map(|edit| {
                edit.state == EditState::Pending
                    && match (&content, proposal.create) {
                        // A new note is stale when someone made the note.
                        (Some(_), true) => true,
                        (None, true) => false,
                        (Some(text), false) => locate(text, &edit.old).is_err(),
                        (None, false) => true,
                    }
            })
            .collect();
        ProposalView {
            status: proposal.status(),
            proposal,
            stale,
        }
    }

    /// Waiting proposals of one note, for the window.
    pub fn note_proposals(&self, id: &NoteId) -> Result<Vec<ProposalView>> {
        Ok(self
            .proposals()?
            .into_iter()
            .filter(|proposal| proposal.note == id.as_str() && proposal.status() == Status::Pending)
            .map(|proposal| self.proposal_view(proposal))
            .collect())
    }

    /// Applies the accepted edits to the note and marks the decisions.
    ///
    /// The edits go to the text on disk now. If one accepted edit has no
    /// place, nothing changes: a half-applied review would be harder to
    /// understand than a refusal. Only waiting edits can get a decision.
    ///
    /// Returns the proposal after the review and the new note text if it
    /// changed.
    pub fn review_proposal(
        &mut self,
        key: &str,
        review: &Review,
    ) -> Result<(Proposal, Option<String>)> {
        let mut proposal = self.proposal(key)?;
        let id = proposal
            .note_id()
            .ok_or_else(|| Error::InvalidName(proposal.note.clone()))?;
        let waiting = |index: &usize| {
            proposal
                .edits
                .get(*index)
                .is_some_and(|edit| edit.state == EditState::Pending)
        };
        let accept: Vec<usize> = review.accept.iter().copied().filter(waiting).collect();
        let reject: Vec<usize> = review
            .reject
            .iter()
            .copied()
            .filter(waiting)
            .filter(|index| !accept.contains(index))
            .collect();

        let mut written = None;
        if !accept.is_empty() {
            if proposal.create {
                if self.contains(&id) {
                    return Err(Error::AlreadyExists(id));
                }
                let content = proposal.edits[0].new.clone();
                self.save(&id, &content)?;
                written = Some(content);
            } else {
                let content = self.read(&id)?;
                let mut order = accept.clone();
                order.sort_unstable();
                let changed = apply(
                    &content,
                    order.iter().map(|&index| (index, &proposal.edits[index])),
                )
                .map_err(|index| Error::StaleEdit(key.to_string(), index + 1))?;
                if changed != content {
                    self.save(&id, &changed)?;
                    written = Some(changed);
                }
            }
        }

        for index in &accept {
            proposal.edits[*index].state = EditState::Accepted;
        }
        for index in &reject {
            proposal.edits[*index].state = EditState::Rejected;
        }
        if let Some(feedback) = review.feedback.as_deref().map(str::trim) {
            if !feedback.is_empty() {
                proposal.feedback = Some(feedback.to_string());
            }
        }
        proposal.updated = now();
        self.proposal_dir().put(&proposal.id, &proposal)?;
        self.proposal_reviewed(&proposal)?;
        Ok((proposal, written))
    }

    /// Removes a proposal: the agent withdraws it, or the person clears the
    /// list.
    pub fn remove_proposal(&mut self, key: &str) -> Result<()> {
        self.proposal(key)?;
        self.proposal_dir().remove(key)
    }

    /// Proposals follow their notes on a rename, like bookmarks.
    pub(crate) fn move_proposals(&self, moves: &[(NoteId, NoteId)]) -> Result<()> {
        let dir = self.proposal_dir();
        for mut proposal in dir.all::<Proposal>()? {
            if let Some((_, to)) = moves
                .iter()
                .find(|(from, _)| from.as_str() == proposal.note)
            {
                proposal.note = to.as_str().to_string();
                dir.put(&proposal.id.clone(), &proposal)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn vault(note: &str) -> (tempfile::TempDir, Workspace, NoteId) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Запуск.md"), note).unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace, NoteId::parse("Запуск").unwrap())
    }

    fn pair(old: &str, new: &str) -> (String, String) {
        (old.to_string(), new.to_string())
    }

    #[test]
    fn a_proposal_does_not_touch_the_note() {
        let (_dir, mut workspace, id) = vault("Проект занял 14 недель.\n");

        let proposal = workspace
            .propose(
                &id,
                vec![pair("14 недель", "12 недель")],
                "по отчёту",
                None,
                None,
            )
            .unwrap();

        assert_eq!(workspace.read(&id).unwrap(), "Проект занял 14 недель.\n");
        assert_eq!(proposal.status(), Status::Pending);
        assert_eq!(workspace.proposals().unwrap(), vec![proposal]);
    }

    #[test]
    fn an_edit_must_have_exactly_one_place() {
        let (_dir, mut workspace, id) = vault("да и да\n");

        let twice = workspace.propose(&id, vec![pair("да", "нет")], "", None, None);
        let missing = workspace.propose(&id, vec![pair("может", "нет")], "", None, None);

        assert!(matches!(twice, Err(Error::BadProposal(text)) if text.contains("несколько раз")));
        assert!(matches!(missing, Err(Error::BadProposal(text)) if text.contains("нет в заметке")));
        assert!(workspace.proposals().unwrap().is_empty());
    }

    #[test]
    fn the_words_inside_a_comment_do_not_count() {
        let (_dir, mut workspace, id) =
            vault("Занял 14 недель<!-- @agent «14 недель»: проверь -->.\n");

        let proposal = workspace
            .propose(&id, vec![pair("14 недель", "12 недель")], "", None, None)
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

        assert_eq!(
            workspace.read(&id).unwrap(),
            "Занял 12 недель<!-- @agent «14 недель»: проверь -->.\n"
        );
    }

    #[test]
    fn overlapping_edits_are_refused() {
        let (_dir, mut workspace, id) = vault("один два три\n");

        let result = workspace.propose(
            &id,
            vec![pair("один два", "1 2"), pair("два три", "2 3")],
            "",
            None,
            None,
        );

        assert!(matches!(result, Err(Error::BadProposal(text)) if text.contains("объедини")));
    }

    #[test]
    fn a_review_applies_only_the_accepted_edits() {
        let (_dir, mut workspace, id) = vault("Занял 14 недель в октябре.\n");
        let proposal = workspace
            .propose(
                &id,
                vec![
                    pair("14 недель", "12 недель"),
                    pair("в октябре", "в сентябре"),
                ],
                "",
                None,
                None,
            )
            .unwrap();

        let (after, written) = workspace
            .review_proposal(
                &proposal.id,
                &Review {
                    accept: vec![0],
                    reject: vec![1],
                    feedback: Some("месяц верный".to_string()),
                },
            )
            .unwrap();

        assert_eq!(written.as_deref(), Some("Занял 12 недель в октябре.\n"));
        assert_eq!(workspace.read(&id).unwrap(), "Занял 12 недель в октябре.\n");
        assert_eq!(after.status(), Status::Partial);
        assert_eq!(after.feedback.as_deref(), Some("месяц верный"));
        assert_eq!(workspace.proposal(&proposal.id).unwrap(), after);
    }

    #[test]
    fn an_edit_whose_text_changed_is_stale_and_nothing_is_applied() {
        let (_dir, mut workspace, id) = vault("Занял 14 недель в октябре.\n");
        let proposal = workspace
            .propose(
                &id,
                vec![
                    pair("14 недель", "12 недель"),
                    pair("в октябре", "в сентябре"),
                ],
                "",
                None,
                None,
            )
            .unwrap();
        workspace.save(&id, "Занял 14 недель осенью.\n").unwrap();

        let view = workspace.proposal_view(workspace.proposal(&proposal.id).unwrap());
        let result = workspace.review_proposal(
            &proposal.id,
            &Review {
                accept: vec![0, 1],
                ..Review::default()
            },
        );

        assert_eq!(view.stale, vec![false, true]);
        assert!(matches!(result, Err(Error::StaleEdit(_, 2))));
        assert_eq!(workspace.read(&id).unwrap(), "Занял 14 недель осенью.\n");
        assert_eq!(
            workspace.proposal(&proposal.id).unwrap().status(),
            Status::Pending
        );
    }

    #[test]
    fn a_new_note_is_written_on_accept() {
        let (_dir, mut workspace, _) = vault("x\n");
        let id = NoteId::parse("Идеи/Новая").unwrap();
        let proposal = workspace
            .propose_note(&id, "# Новая\n\nтекст\n", "сводка", Some("claude"))
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

        assert_eq!(workspace.read(&id).unwrap(), "# Новая\n\nтекст\n");
    }

    #[test]
    fn proposals_follow_a_renamed_note() {
        let (_dir, mut workspace, id) = vault("старый текст\n");
        let proposal = workspace
            .propose(&id, vec![pair("старый", "новый")], "", None, None)
            .unwrap();
        let to = NoteId::parse("Архив/Запуск").unwrap();

        workspace.rename(&id, &to).unwrap();

        assert_eq!(
            workspace.proposal(&proposal.id).unwrap().note,
            "Архив/Запуск"
        );
    }
}
