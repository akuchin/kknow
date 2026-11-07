//! The sync itself: compute the plan and run it.

use std::collections::{BTreeMap, BTreeSet};

use kknow_core::{NoteId, Vault};

use crate::manifest::{Entry, Manifest, state_path};
use crate::plan::{Action, ConflictPolicy, FileAction, Plan, diff, diff_files};
use crate::provider::SyncProvider;
use crate::shared::{self, Merge};
use crate::{Error, Result};

/// Manifest key in the storage. It is hidden in the same way as
/// `.kknow/sync.json` in the vault, so the storage looks like the vault.
pub const MANIFEST_KEY: &str = ".kknow/manifest.json";

/// Key of the object with the note contents: the same path as in the vault.
pub fn note_key(id: &NoteId) -> String {
    format!("{id}.md")
}

/// The note that a storage object holds, if it is a note.
///
/// The reverse of [`note_key`]. Hidden paths are not notes, as in the vault:
/// they hold the manifest and the files of other programs.
pub fn note_of_key(key: &str) -> Option<NoteId> {
    let path = key.strip_suffix(".md")?;
    if path.split('/').any(|part| part.starts_with('.')) {
        return None;
    }
    NoteId::parse(path)
}

/// How many sync objects are already transferred.
///
/// Counts plan actions, not bytes: for a sync of eight notes, "3 of 8" is
/// clearer than megabytes, and notes are never large single files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

/// A sync ready for transfer: the plan and the contents of all outgoing notes.
///
/// The contents are read in advance while the vault is locked. The network
/// part comes next, and the vault must not stay locked during it: an edit in
/// the editor would wait for the sync to end. The cost is that outgoing notes
/// stay in memory. For note sizes this is cheaper than a busy vault.
#[derive(Debug)]
pub struct Staged {
    plan: Plan,
    outgoing: BTreeMap<NoteId, String>,
    /// Local bytes of the shared files to upload or merge.
    outgoing_files: BTreeMap<String, Vec<u8>>,
}

impl Staged {
    /// Number of actions to do. Progress counts them.
    pub fn len(&self) -> usize {
        self.plan.actions().len() + self.plan.file_actions().len()
    }

    pub fn is_empty(&self) -> bool {
        self.plan.is_empty()
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }
}

/// What the sync brought: contents to write to disk and the new state.
#[derive(Debug)]
pub struct Fetched {
    /// Notes to write: downloaded notes and conflict copies.
    incoming: Vec<(NoteId, String)>,
    /// Notes that disappeared from the storage. They go to the vault trash.
    remove_local: Vec<NoteId>,
    /// Shared files to write: downloaded or merged.
    incoming_files: Vec<(String, Vec<u8>)>,
    /// Shared files that disappeared from the storage.
    remove_local_files: Vec<String>,
    /// The state both sides agreed on.
    next: Manifest,
    report: Report,
}

impl Fetched {
    pub fn is_empty(&self) -> bool {
        self.report.is_empty()
    }
}

/// A note that diverged on two devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The note that keeps the local version.
    pub id: NoteId,
    /// The copy that gets the cloud version.
    pub copy: NoteId,
}

/// What the sync did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub uploaded: Vec<NoteId>,
    pub downloaded: Vec<NoteId>,
    pub removed_local: Vec<NoteId>,
    pub removed_remote: Vec<NoteId>,
    pub conflicts: Vec<Conflict>,
    /// Shared files of `.kknow` that sync changed on disk. A caller that keeps
    /// bookmarks in memory must read them again.
    pub files_local: Vec<String>,
    /// Shared files that sync sent to the storage.
    pub files_remote: Vec<String>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.uploaded.is_empty()
            && self.downloaded.is_empty()
            && self.removed_local.is_empty()
            && self.removed_remote.is_empty()
            && self.conflicts.is_empty()
            && self.files_local.is_empty()
            && self.files_remote.is_empty()
    }

    /// Notes that the sync wrote to disk. A caller that keeps the vault open
    /// must parse them again.
    pub fn local_updated(&self) -> impl Iterator<Item = &NoteId> {
        self.downloaded
            .iter()
            .chain(self.conflicts.iter().map(|conflict| &conflict.copy))
    }

    /// Notes removed from disk, that is, moved to the vault trash.
    pub fn local_removed(&self) -> impl Iterator<Item = &NoteId> {
        self.removed_local.iter()
    }
}

/// Sync of a vault with one remote storage.
///
/// The storage is a mirror of the vault: each note is at its own path
/// (`<id>.md`). The manifest (`.kknow/manifest.json`) keeps the hashes of
/// the notes that kknow wrote. Notes are
/// written first and the manifest last. An extra object in the cloud does no
/// harm until the manifest changes. But a manifest that lists a note not yet
/// uploaded would cause an error on another device.
pub struct SyncEngine<P> {
    provider: P,
    policy: ConflictPolicy,
    merge: Option<Merge>,
}

impl<P: SyncProvider> SyncEngine<P> {
    pub fn new(provider: P) -> Self {
        SyncEngine {
            provider,
            policy: ConflictPolicy::default(),
            merge: None,
        }
    }

    /// The function that joins two versions of a shared file. Without it the
    /// local version wins a conflict.
    pub fn with_merge(mut self, merge: Merge) -> Self {
        self.merge = Some(merge);
        self
    }

    pub fn with_policy(mut self, policy: ConflictPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Computes what to do and changes nothing.
    ///
    /// This does not run [`SyncProvider::prepare`], which writes: a plaintext
    /// note in an encrypted storage shows up only in a real sync.
    pub fn plan(&self, vault: &Vault) -> Result<Plan> {
        self.plan_after(vault, &[])
    }

    /// The plan after `prepare` changed the given keys in the storage.
    fn plan_after(&self, vault: &Vault, changed: &[String]) -> Result<Plan> {
        let (mut remote, revision) = self.fetch_manifest()?;
        // A changed key gets its hash from the contents again in `reconcile`.
        for key in changed {
            match note_of_key(key) {
                Some(id) => remote.remove(&id),
                None => remote.remove_file(key),
            }
        }
        self.reconcile(&mut remote)?;
        let base = Manifest::load(&state_path(vault))?;
        let local = Manifest::of_vault(vault)?;

        Ok(Plan {
            actions: diff(&local, &remote, &base, self.policy),
            files: diff_files(&local, &remote, &base, self.policy),
            local,
            remote,
            revision,
        })
    }

    /// Prepares the sync: computes the plan and reads all outgoing notes.
    ///
    /// The vault is read-only here. This is the first of three sync parts, and
    /// the only one that holds the vault during network access (the manifest).
    pub fn stage(&self, vault: &Vault) -> Result<Staged> {
        let changed = self.provider.prepare()?;
        let plan = self.plan_after(vault, &changed)?;

        let mut outgoing = BTreeMap::new();
        for action in plan.actions() {
            match action {
                Action::Upload(id) => {
                    outgoing.insert(id.clone(), vault.read(id)?);
                }
                // On a conflict the local version goes out too: the storage
                // must get both versions.
                Action::KeepBoth { id, .. } => {
                    outgoing.insert(id.clone(), vault.read(id)?);
                }
                _ => {}
            }
        }

        let mut outgoing_files = BTreeMap::new();
        for action in plan.file_actions() {
            if let FileAction::Upload(key) | FileAction::Merge(key) = action {
                let bytes = shared::read(vault.root(), key)?.unwrap_or_default();
                outgoing_files.insert(key.clone(), bytes);
            }
        }

        Ok(Staged {
            plan,
            outgoing,
            outgoing_files,
        })
    }

    /// Transfer: uploads, downloads, remote deletes, and the manifest write.
    ///
    /// The vault is not needed here at all. This is why the sync has parts:
    /// the network runs without the lock, and an edit in the editor does not
    /// wait for it.
    pub fn transfer(&self, staged: Staged, progress: &mut dyn FnMut(Progress)) -> Result<Fetched> {
        let Staged {
            plan,
            outgoing,
            outgoing_files,
        } = staged;
        let Plan {
            actions,
            files,
            local,
            remote,
            revision,
        } = plan;

        // Build the final state step by step, do not compute it from disk at
        // the end. Otherwise an edit made during the sync would go into base
        // as "already synced" and would never be uploaded.
        let mut next = Manifest::new();
        for (id, entry) in local.entries() {
            if remote.hash_of(id) == Some(entry.hash.as_str()) {
                next.insert(id.clone(), entry.clone());
            }
        }
        for (key, entry) in local.file_entries() {
            if remote.file_hash(key) == Some(entry.hash.as_str()) {
                next.insert_file(key.clone(), entry.clone());
            }
        }

        let total = actions.len() + files.len();
        let mut fetched = Fetched {
            incoming: Vec::new(),
            remove_local: Vec::new(),
            incoming_files: Vec::new(),
            remove_local_files: Vec::new(),
            next: Manifest::new(),
            report: Report::default(),
        };
        progress(Progress { done: 0, total });

        for (done, action) in actions.iter().enumerate() {
            match action {
                Action::Upload(id) => {
                    let content = outgoing.get(id).cloned().unwrap_or_default();
                    self.provider.put(&note_key(id), content.as_bytes())?;
                    next.insert(id.clone(), Entry::of(&content));
                    fetched.report.uploaded.push(id.clone());
                }
                Action::Download(id) => {
                    let content = self.download(id)?;
                    next.insert(id.clone(), Entry::of(&content));
                    fetched.incoming.push((id.clone(), content));
                    fetched.report.downloaded.push(id.clone());
                }
                Action::RemoveLocal(id) => {
                    // The local commit step moves the file to the trash, not
                    // this step.
                    next.remove(id);
                    fetched.remove_local.push(id.clone());
                    fetched.report.removed_local.push(id.clone());
                }
                Action::RemoveRemote(id) => {
                    self.provider.delete(&note_key(id))?;
                    next.remove(id);
                    fetched.report.removed_remote.push(id.clone());
                }
                Action::KeepBoth { id, copy } => {
                    let theirs = self.download(id)?;
                    self.provider.put(&note_key(copy), theirs.as_bytes())?;
                    next.insert(copy.clone(), Entry::of(&theirs));
                    fetched.incoming.push((copy.clone(), theirs));

                    let mine = outgoing.get(id).cloned().unwrap_or_default();
                    self.provider.put(&note_key(id), mine.as_bytes())?;
                    next.insert(id.clone(), Entry::of(&mine));

                    fetched.report.conflicts.push(Conflict {
                        id: id.clone(),
                        copy: copy.clone(),
                    });
                }
            }

            progress(Progress {
                done: done + 1,
                total,
            });
        }

        for (done, action) in files.iter().enumerate() {
            self.transfer_file(action, &outgoing_files, &mut next, &mut fetched)?;
            progress(Progress {
                done: actions.len() + done + 1,
                total,
            });
        }

        if next != remote {
            self.commit(&next, revision.as_deref())?;
        }
        fetched.next = next;

        Ok(fetched)
    }

    /// Writes the fetched notes to disk and saves the state.
    ///
    /// A note that another device deleted goes to the vault trash, not away
    /// for good: that delete can be a mistake.
    ///
    /// This is the third sync part, and the only one that needs the vault lock.
    pub fn commit_local(&self, vault: &mut Vault, fetched: Fetched) -> Result<Report> {
        let Fetched {
            incoming,
            remove_local,
            incoming_files,
            remove_local_files,
            next,
            report,
        } = fetched;

        for (id, content) in &incoming {
            vault.write(id, content)?;
        }
        for id in &remove_local {
            vault.remove(id)?;
        }
        for (key, bytes) in &incoming_files {
            shared::write(vault.root(), key, bytes)?;
        }
        for key in &remove_local_files {
            shared::remove(vault.root(), key)?;
        }

        next.save(&state_path(vault))?;
        Ok(report)
    }

    /// Syncs the vault: all three parts in one call.
    ///
    /// The terminal uses this because it has no lock to release. The window
    /// calls the parts one by one so the vault is not busy during network
    /// access.
    pub fn run(&self, vault: &mut Vault) -> Result<Report> {
        let staged = self.stage(vault)?;
        let fetched = self.transfer(staged, &mut |_| {})?;
        self.commit_local(vault, fetched)
    }

    /// Keys of all notes in the storage. This shows what is really there, not
    /// what the manifest says.
    pub fn remote_keys(&self) -> Result<Vec<String>> {
        let mut keys = self.provider.list("")?;
        keys.retain(|key| note_of_key(key).is_some());
        Ok(keys)
    }

    /// Makes the remote manifest match the objects in the storage.
    ///
    /// Other programs also write to the storage: a person drops a file in the
    /// bucket, an agent saves its research there. A note that the manifest
    /// does not list gets its hash from its contents, and the plan downloads
    /// it as any other remote note. A listed note with no object was deleted
    /// in the storage, and the plan removes it locally.
    ///
    /// A note that another program changed in place keeps its old hash: the
    /// list gives keys, not contents, and a download of each note on each
    /// sync is too expensive.
    fn reconcile(&self, remote: &mut Manifest) -> Result<()> {
        let keys = self.provider.list("")?;
        let present: BTreeSet<NoteId> = keys.iter().filter_map(|key| note_of_key(key)).collect();

        let gone: Vec<NoteId> = remote
            .ids()
            .filter(|id| !present.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            remote.remove(&id);
        }

        for id in present {
            if !remote.contains(&id) {
                let content = self.download(&id)?;
                remote.insert(id, Entry::of(&content));
            }
        }

        // The same for shared files. An older build drops the `files` field
        // when it writes the manifest; this finds the objects again.
        let present: BTreeSet<String> = keys
            .into_iter()
            .filter(|key| shared::is_shared(key))
            .collect();
        let gone: Vec<String> = remote
            .file_keys()
            .filter(|key| !present.contains(*key))
            .cloned()
            .collect();
        for key in gone {
            remote.remove_file(&key);
        }
        for key in present {
            if remote.file_hash(&key).is_none() {
                if let Some(bytes) = self.provider.get(&key)? {
                    remote.insert_file(key, Entry::of_bytes(&bytes));
                }
            }
        }
        Ok(())
    }

    /// One action on a shared file during the transfer.
    fn transfer_file(
        &self,
        action: &FileAction,
        outgoing: &BTreeMap<String, Vec<u8>>,
        next: &mut Manifest,
        fetched: &mut Fetched,
    ) -> Result<()> {
        match action {
            FileAction::Upload(key) => {
                let bytes = outgoing.get(key).cloned().unwrap_or_default();
                self.provider.put(key, &bytes)?;
                next.insert_file(key.clone(), Entry::of_bytes(&bytes));
                fetched.report.files_remote.push(key.clone());
            }
            FileAction::Download(key) => {
                let bytes = self.provider.get(key)?.unwrap_or_default();
                next.insert_file(key.clone(), Entry::of_bytes(&bytes));
                fetched.incoming_files.push((key.clone(), bytes));
                fetched.report.files_local.push(key.clone());
            }
            FileAction::RemoveLocal(key) => {
                next.remove_file(key);
                fetched.remove_local_files.push(key.clone());
                fetched.report.files_local.push(key.clone());
            }
            FileAction::RemoveRemote(key) => {
                self.provider.delete(key)?;
                next.remove_file(key);
                fetched.report.files_remote.push(key.clone());
            }
            FileAction::Merge(key) => {
                let mine = outgoing.get(key).cloned().unwrap_or_default();
                let theirs = self.provider.get(key)?.unwrap_or_default();
                let merged = self
                    .merge
                    .and_then(|merge| merge(key, &mine, &theirs))
                    .unwrap_or_else(|| mine.clone());
                self.provider.put(key, &merged)?;
                next.insert_file(key.clone(), Entry::of_bytes(&merged));
                fetched.report.files_remote.push(key.clone());
                if merged != mine {
                    fetched.incoming_files.push((key.clone(), merged));
                    fetched.report.files_local.push(key.clone());
                }
            }
        }
        Ok(())
    }

    /// The remote manifest and its revision, that is, the hash of the bytes read.
    fn fetch_manifest(&self) -> Result<(Manifest, Option<String>)> {
        match self.provider.get(MANIFEST_KEY)? {
            Some(bytes) => Ok((
                Manifest::from_json(&bytes)?,
                Some(crate::manifest::hash(&bytes)),
            )),
            None => Ok((Manifest::new(), None)),
        }
    }

    fn download(&self, id: &NoteId) -> Result<String> {
        let key = note_key(id);
        let bytes = self
            .provider
            .get(&key)?
            .ok_or_else(|| Error::MissingRemote(id.clone()))?;
        String::from_utf8(bytes).map_err(|_| Error::NotUtf8(id.clone()))
    }

    /// Writes the manifest if the cloud did not move ahead.
    ///
    /// A gap of a few milliseconds stays between the check and the write: S3
    /// gives no real lock. Without the check, the gap between "read the
    /// manifest" and "write the new one" lasts the whole sync. A second device
    /// that wrote its manifest in that time would lose its notes: other
    /// devices would see them as deleted.
    fn commit(&self, manifest: &Manifest, revision: Option<&str>) -> Result<()> {
        let (_, current) = self.fetch_manifest()?;
        if current.as_deref() != revision {
            return Err(Error::RemoteChanged);
        }

        self.provider.put(MANIFEST_KEY, &manifest.to_json())?;
        Ok(())
    }
}

impl<P> std::fmt::Debug for SyncEngine<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncEngine")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}
