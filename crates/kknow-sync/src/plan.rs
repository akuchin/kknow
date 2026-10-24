//! What to do so that disk and cloud match.
//!
//! The plan comes from three manifests with no network or disk access. So a
//! test can check the decision for each note, and the user can see it in
//! advance (`kknow sync --dry-run`).

use std::collections::BTreeSet;
use std::fmt;

use kknow_core::NoteId;
use serde::{Deserialize, Serialize};

use crate::manifest::Manifest;

/// Which version wins when both sides changed the same note.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ConflictPolicy {
    /// Lose nothing: the local version stays, and the remote version goes next
    /// to it as a separate note.
    #[default]
    KeepBoth,
    /// The version on this device wins.
    PreferLocal,
    /// The cloud version wins.
    PreferRemote,
}

/// One action on one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send the local version to the cloud.
    Upload(NoteId),
    /// Get the cloud version to disk.
    Download(NoteId),
    /// Remove the note from disk: another device deleted it.
    RemoveLocal(NoteId),
    /// Remove the note from the cloud: it was deleted here.
    RemoveRemote(NoteId),
    /// Conflict: `id` keeps the local version, `copy` gets the remote one.
    KeepBoth { id: NoteId, copy: NoteId },
}

impl Action {
    pub fn id(&self) -> &NoteId {
        match self {
            Action::Upload(id)
            | Action::Download(id)
            | Action::RemoveLocal(id)
            | Action::RemoveRemote(id)
            | Action::KeepBoth { id, .. } => id,
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Upload(id) => write!(f, "выгрузить        {id}"),
            Action::Download(id) => write!(f, "загрузить        {id}"),
            Action::RemoveLocal(id) => write!(f, "удалить локально {id}"),
            Action::RemoveRemote(id) => write!(f, "удалить в облаке {id}"),
            Action::KeepBoth { id, copy } => write!(f, "конфликт         {id} → {copy}"),
        }
    }
}

/// The sync plan with the states it was computed from.
#[derive(Debug, Clone)]
pub struct Plan {
    pub(crate) actions: Vec<Action>,
    pub(crate) local: Manifest,
    pub(crate) remote: Manifest,
    /// Hash of the manifest as we read it. Before the new write, we check that
    /// the cloud did not move ahead since then.
    pub(crate) revision: Option<String>,
}

impl Plan {
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for action in &self.actions {
            writeln!(f, "{action}")?;
        }
        Ok(())
    }
}

/// Turns three states into a list of actions.
///
/// `base` is the state both sides agreed on last time. It tells "the note
/// changed" from "the note was not touched". Without it, a delete on one
/// device looks like a new note on another, and deleted notes would come back
/// from the cloud forever.
pub(crate) fn diff(
    local: &Manifest,
    remote: &Manifest,
    base: &Manifest,
    policy: ConflictPolicy,
) -> Vec<Action> {
    let ids: BTreeSet<&NoteId> = local.ids().chain(remote.ids()).chain(base.ids()).collect();

    ids.into_iter()
        .filter_map(|id| decide(id, local, remote, base, policy))
        .collect()
}

fn decide(
    id: &NoteId,
    local: &Manifest,
    remote: &Manifest,
    base: &Manifest,
    policy: ConflictPolicy,
) -> Option<Action> {
    let here = local.hash_of(id);
    let there = remote.hash_of(id);
    let known = base.hash_of(id);

    match (here, there) {
        // Same bytes: nothing to do, even if both sides changed.
        (Some(here), Some(there)) if here == there => None,

        (Some(here), Some(_)) => {
            if known == Some(here) {
                // Not touched here, so the cloud moved ahead.
                Some(Action::Download(id.clone()))
            } else if known == there {
                Some(Action::Upload(id.clone()))
            } else {
                Some(conflict(id, remote, policy))
            }
        }

        (Some(here), None) => match known {
            // The note appeared here after the last sync.
            None => Some(Action::Upload(id.clone())),
            // Another device deleted it, and it did not change here.
            Some(known) if known == here => Some(Action::RemoveLocal(id.clone())),
            // Deleted there, changed here. An edit is more valuable than a
            // delete, so by default the note goes back to the cloud.
            Some(_) => match policy {
                ConflictPolicy::PreferRemote => Some(Action::RemoveLocal(id.clone())),
                _ => Some(Action::Upload(id.clone())),
            },
        },

        (None, Some(there)) => match known {
            None => Some(Action::Download(id.clone())),
            Some(known) if known == there => Some(Action::RemoveRemote(id.clone())),
            Some(_) => match policy {
                ConflictPolicy::PreferLocal => Some(Action::RemoveRemote(id.clone())),
                _ => Some(Action::Download(id.clone())),
            },
        },

        // Gone on both sides: only the base entry is left.
        (None, None) => None,
    }
}

fn conflict(id: &NoteId, remote: &Manifest, policy: ConflictPolicy) -> Action {
    match policy {
        ConflictPolicy::PreferLocal => Action::Upload(id.clone()),
        ConflictPolicy::PreferRemote => Action::Download(id.clone()),
        ConflictPolicy::KeepBoth => {
            let short = remote.get(id).map(|entry| entry.short_hash()).unwrap_or("");
            Action::KeepBoth {
                id: id.clone(),
                copy: conflict_copy(id, short),
            }
        }
    }
}

/// This mark in a name shows a copy left by a conflict.
///
/// It is a separate constant because the list of conflicts to resolve comes
/// from the files in the directory: a copy is a file and needs no other state.
pub const CONFLICT_MARK: &str = " (конфликт ";

pub fn is_conflict_copy(id: &NoteId) -> bool {
    let name = id.name();
    name.contains(CONFLICT_MARK) && name.ends_with(')')
}

/// Name of the copy for a conflict version: the note ID, [`CONFLICT_MARK`],
/// the short hash, and `)`.
///
/// The name has a hash, not a time: the same contents give the same name. So a
/// repeated sync does not make more copies, it reuses the existing one.
fn conflict_copy(id: &NoteId, short_hash: &str) -> NoteId {
    NoteId::parse(&format!("{id}{CONFLICT_MARK}{short_hash})"))
        .expect("имя копии отличается от исходного только суффиксом")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Entry;

    fn manifest(notes: &[(&str, &str)]) -> Manifest {
        let mut manifest = Manifest::new();
        for (id, content) in notes {
            manifest.insert(NoteId::parse(id).unwrap(), Entry::of(content));
        }
        manifest
    }

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    fn plan(local: &[(&str, &str)], remote: &[(&str, &str)], base: &[(&str, &str)]) -> Vec<Action> {
        diff(
            &manifest(local),
            &manifest(remote),
            &manifest(base),
            ConflictPolicy::KeepBoth,
        )
    }

    #[test]
    fn first_sync_uploads_everything() {
        let actions = plan(&[("Index", "# Оглавление")], &[], &[]);

        assert_eq!(actions, [Action::Upload(id("Index"))]);
    }

    #[test]
    fn a_new_remote_note_is_downloaded() {
        let actions = plan(&[], &[("Index", "# Оглавление")], &[]);

        assert_eq!(actions, [Action::Download(id("Index"))]);
    }

    #[test]
    fn matching_content_needs_nothing() {
        let both = [("Index", "# Оглавление")];

        assert!(plan(&both, &both, &[]).is_empty());
        assert!(plan(&both, &both, &both).is_empty());
    }

    #[test]
    fn a_local_edit_goes_up_and_a_remote_edit_comes_down() {
        let base = [("Index", "старое")];

        assert_eq!(
            plan(&[("Index", "новое")], &base, &base),
            [Action::Upload(id("Index"))]
        );
        assert_eq!(
            plan(&base, &[("Index", "новое")], &base),
            [Action::Download(id("Index"))]
        );
    }

    #[test]
    fn a_delete_travels_in_both_directions() {
        let base = [("Index", "текст")];

        assert_eq!(
            plan(&base, &[], &base),
            [Action::RemoveLocal(id("Index"))],
            "удалили в облаке — убираем с диска"
        );
        assert_eq!(
            plan(&[], &base, &base),
            [Action::RemoveRemote(id("Index"))],
            "удалили здесь — убираем из облака"
        );
    }

    #[test]
    fn both_sides_edited_gives_a_conflict_copy() {
        let actions = plan(
            &[("Index", "моё")],
            &[("Index", "чужое")],
            &[("Index", "старое")],
        );

        let Action::KeepBoth { id: note, copy } = &actions[0] else {
            panic!("ожидался конфликт, а не {:?}", actions[0]);
        };
        assert_eq!(note, &id("Index"));
        assert!(copy.name().starts_with("Index (конфликт "), "{copy}");
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn the_conflict_copy_is_named_after_the_content() {
        let remote = manifest(&[("Index", "чужое")]);
        let first = conflict(&id("Index"), &remote, ConflictPolicy::KeepBoth);
        let second = conflict(&id("Index"), &remote, ConflictPolicy::KeepBoth);

        assert_eq!(first, second, "повтор не должен плодить копии");
    }

    #[test]
    fn policy_picks_a_side_when_asked() {
        let local = manifest(&[("Index", "моё")]);
        let remote = manifest(&[("Index", "чужое")]);
        let base = manifest(&[("Index", "старое")]);

        assert_eq!(
            diff(&local, &remote, &base, ConflictPolicy::PreferLocal),
            [Action::Upload(id("Index"))]
        );
        assert_eq!(
            diff(&local, &remote, &base, ConflictPolicy::PreferRemote),
            [Action::Download(id("Index"))]
        );
    }

    #[test]
    fn an_edit_beats_a_delete_from_the_other_side() {
        let base = [("Index", "старое")];

        assert_eq!(
            plan(&[("Index", "правка")], &[], &base),
            [Action::Upload(id("Index"))],
            "удалили в облаке, но здесь правили — заметка возвращается"
        );
        assert_eq!(
            plan(&[], &[("Index", "правка")], &base),
            [Action::Download(id("Index"))],
            "удалили здесь, но правили в облаке — заметка возвращается"
        );
    }

    #[test]
    fn a_delete_wins_when_the_policy_says_so() {
        let base = [("Index", "старое")];

        assert_eq!(
            diff(
                &manifest(&[("Index", "правка")]),
                &manifest(&[]),
                &manifest(&base),
                ConflictPolicy::PreferRemote
            ),
            [Action::RemoveLocal(id("Index"))]
        );
    }

    #[test]
    fn a_note_gone_from_both_sides_leaves_only_base() {
        assert!(plan(&[], &[], &[("Index", "текст")]).is_empty());
    }
}
