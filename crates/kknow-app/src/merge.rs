//! Joins two versions of a shared file of `.kknow` after both machines
//! changed it.
//!
//! The sync engine knows only bytes; this module knows the formats. A note
//! conflict gives two copies, and the person picks. A shared file has no such
//! person: nobody opens a copy of `bookmarks.json`. So each format joins both
//! sides itself, and the result is a superset of both where possible.

use kknow_sync::{DataKey, RemoteConfig, SyncEngine, SyncProvider};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::bookmarks::{BOOKMARKS_PATH, Bookmarks};
use crate::proposals::{EditState, PROPOSALS_DIR, Proposal};
use crate::threads::{THREADS_DIR, Thread};

/// The sync engine for the saved settings, with the merge of shared files.
///
/// Both frontends build the engine here, so neither one can forget the merge.
pub fn sync_engine(
    config: &RemoteConfig,
    secret: Option<String>,
    key: Option<&DataKey>,
) -> kknow_sync::Result<SyncEngine<Box<dyn SyncProvider>>> {
    Ok(kknow_sync::engine(config, secret, key)?.with_merge(merge_shared))
}

/// [`kknow_sync::shared::Merge`] for the files of this app. `None` for a file
/// that does not parse: then sync keeps the local version.
pub fn merge_shared(key: &str, mine: &[u8], theirs: &[u8]) -> Option<Vec<u8>> {
    if key == BOOKMARKS_PATH {
        let mine: Bookmarks = serde_json::from_slice(mine).ok()?;
        let theirs: Bookmarks = serde_json::from_slice(theirs).ok()?;
        return Some(to_bytes(&mine.merged(&theirs)));
    }
    if key.starts_with(THREADS_DIR) {
        return join(mine, theirs, merge_threads);
    }
    if key.starts_with(PROPOSALS_DIR) {
        return join(mine, theirs, merge_proposals);
    }
    None
}

fn join<T: Serialize + DeserializeOwned>(
    mine: &[u8],
    theirs: &[u8],
    merge: fn(T, T) -> T,
) -> Option<Vec<u8>> {
    let mine: T = serde_json::from_slice(mine).ok()?;
    let theirs: T = serde_json::from_slice(theirs).ok()?;
    Some(to_bytes(&merge(mine, theirs)))
}

fn to_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("запись сериализуема");
    bytes.push(b'\n');
    bytes
}

/// All messages of both sides in time order; the state of the side that
/// changed the thread last.
fn merge_threads(mine: Thread, theirs: Thread) -> Thread {
    let (mut newer, older) = if theirs.updated > mine.updated {
        (theirs, mine)
    } else {
        (mine, theirs)
    };
    for message in older.messages {
        if !newer.messages.contains(&message) {
            newer.messages.push(message);
        }
    }
    newer.messages.sort_by_key(|message| message.at);
    newer.updated = newer.updated.max(older.updated);
    newer
}

/// A decision on an edit wins over "pending": one machine reviewed it, the
/// other did not see the review yet. Of two decisions, the newer side wins.
fn merge_proposals(mine: Proposal, theirs: Proposal) -> Proposal {
    let (mut newer, older) = if theirs.updated > mine.updated {
        (theirs, mine)
    } else {
        (mine, theirs)
    };
    for (edit, other) in newer.edits.iter_mut().zip(older.edits.iter()) {
        if edit.state == EditState::Pending && edit.old == other.old {
            edit.state = other.state;
        }
    }
    if newer.feedback.is_none() {
        newer.feedback = older.feedback;
    }
    newer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threads::{Message, Status};

    fn thread(updated: u64, status: Status, messages: &[(&str, u64)]) -> Vec<u8> {
        to_bytes(&Thread {
            id: "k3f".to_string(),
            note: "План".to_string(),
            anchor: String::new(),
            task: "проверь".to_string(),
            status,
            messages: messages
                .iter()
                .map(|(text, at)| Message {
                    from: "me".to_string(),
                    text: text.to_string(),
                    at: *at,
                })
                .collect(),
            proposal: None,
            created: 1,
            updated,
        })
    }

    #[test]
    fn threads_keep_the_messages_of_both_sides() {
        let mine = thread(5, Status::Answered, &[("проверь", 1), ("отчёт", 5)]);
        let theirs = thread(4, Status::Question, &[("проверь", 1), ("12 или 14?", 4)]);

        let merged: Thread = serde_json::from_slice(
            &merge_shared(".kknow/threads/k3f.json", &mine, &theirs).unwrap(),
        )
        .unwrap();

        let texts: Vec<&str> = merged.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["проверь", "12 или 14?", "отчёт"]);
        assert_eq!(merged.status, Status::Answered);
    }

    #[test]
    fn a_review_on_one_machine_is_not_lost() {
        let proposal = |updated, state| {
            to_bytes(&Proposal {
                id: "p1".to_string(),
                note: "План".to_string(),
                reason: String::new(),
                author: None,
                comment: None,
                create: false,
                created: 1,
                updated,
                edits: vec![crate::proposals::Edit {
                    old: "a".to_string(),
                    new: "b".to_string(),
                    state,
                }],
                feedback: None,
            })
        };
        let reviewed = proposal(3, EditState::Accepted);
        let untouched = proposal(5, EditState::Pending);

        let merged: Proposal = serde_json::from_slice(
            &merge_shared(".kknow/proposals/p1.json", &untouched, &reviewed).unwrap(),
        )
        .unwrap();

        assert_eq!(merged.edits[0].state, EditState::Accepted);
    }

    #[test]
    fn a_file_that_does_not_parse_is_not_merged() {
        assert_eq!(merge_shared(".kknow/threads/k3f.json", b"{", b"{}"), None);
        assert_eq!(merge_shared(".kknow/other.json", b"{}", b"{}"), None);
    }
}
