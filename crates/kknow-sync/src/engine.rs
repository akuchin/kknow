use std::collections::{BTreeMap, BTreeSet};

use kknow_core::{NoteId, Vault};

use crate::manifest::{Entry, Manifest, state_path};
use crate::plan::{Action, ConflictPolicy, FileAction, Plan, diff, diff_files};
use crate::provider::SyncProvider;
use crate::shared::{self, Merge};
use crate::{Error, Result};

pub const MANIFEST_KEY: &str = ".kknow/manifest.json";

pub fn note_key(id: &NoteId) -> String {
    format!("{id}.md")
}

pub fn note_of_key(key: &str) -> Option<NoteId> {
    let path = key.strip_suffix(".md")?;
    if path.split('/').any(|part| part.starts_with('.')) {
        return None;
    }
    NoteId::parse(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug)]
pub struct Staged {
    plan: Plan,
    outgoing: BTreeMap<NoteId, String>,
    outgoing_files: BTreeMap<String, Vec<u8>>,
}

impl Staged {
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

#[derive(Debug)]
pub struct Fetched {
    incoming: Vec<(NoteId, String)>,
    remove_local: Vec<NoteId>,
    incoming_files: Vec<(String, Vec<u8>)>,
    remove_local_files: Vec<String>,
    next: Manifest,
    report: Report,
}

impl Fetched {
    pub fn is_empty(&self) -> bool {
        self.report.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub id: NoteId,
    pub copy: NoteId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub uploaded: Vec<NoteId>,
    pub downloaded: Vec<NoteId>,
    pub removed_local: Vec<NoteId>,
    pub removed_remote: Vec<NoteId>,
    pub conflicts: Vec<Conflict>,
    pub files_local: Vec<String>,
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

    pub fn local_updated(&self) -> impl Iterator<Item = &NoteId> {
        self.downloaded
            .iter()
            .chain(self.conflicts.iter().map(|conflict| &conflict.copy))
    }

    pub fn local_removed(&self) -> impl Iterator<Item = &NoteId> {
        self.removed_local.iter()
    }
}

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

    pub fn plan(&self, vault: &Vault) -> Result<Plan> {
        self.plan_after(vault, &[])
    }

    fn plan_after(&self, vault: &Vault, changed: &[String]) -> Result<Plan> {
        let (mut remote, revision) = self.fetch_manifest()?;
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

    pub fn stage(&self, vault: &Vault) -> Result<Staged> {
        let changed = self.provider.prepare()?;
        let plan = self.plan_after(vault, &changed)?;

        let mut outgoing = BTreeMap::new();
        for action in plan.actions() {
            match action {
                Action::Upload(id) => {
                    outgoing.insert(id.clone(), vault.read(id)?);
                }
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

    pub fn run(&self, vault: &mut Vault) -> Result<Report> {
        let staged = self.stage(vault)?;
        let fetched = self.transfer(staged, &mut |_| {})?;
        self.commit_local(vault, fetched)
    }

    pub fn remote_keys(&self) -> Result<Vec<String>> {
        let mut keys = self.provider.list("")?;
        keys.retain(|key| note_of_key(key).is_some());
        Ok(keys)
    }

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
