use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kknow_core::{Note, NoteId};
use kknow_index::{LinkGraph, ParsedNote, SearchHit};

use crate::theme::ThemeSet;
use crate::*;

impl Workspace {
    pub fn themes(&self, global: Option<&Path>) -> ThemeSet {
        theme::load(Some(self.vault.root()), global)
    }

    pub fn stats(&self) -> VaultStats {
        let mut folders: BTreeSet<&str> = BTreeSet::new();
        let mut bytes = 0;

        for id in self.parsed.keys() {
            if let Some(folder) = id.folder() {
                for (at, _) in folder.match_indices('/') {
                    folders.insert(&folder[..at]);
                }
                folders.insert(folder);
            }

            bytes += std::fs::metadata(self.path_for(id))
                .map(|meta| meta.len())
                .unwrap_or(0);
        }

        VaultStats {
            notes: self.parsed.len(),
            folders: folders.len(),
            bytes,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        Ok(self.search.search(query, limit)?)
    }

    pub fn root(&self) -> &Path {
        self.vault.root()
    }

    pub fn len(&self) -> usize {
        self.parsed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parsed.is_empty()
    }

    pub fn graph(&self) -> &LinkGraph {
        &self.graph
    }

    pub fn notes(&self) -> impl ExactSizeIterator<Item = (&NoteId, &ParsedNote)> {
        self.parsed.iter()
    }

    pub fn note(&self, id: &NoteId) -> Option<&Note> {
        self.vault.get(id)
    }

    pub fn parsed(&self, id: &NoteId) -> Option<&ParsedNote> {
        self.parsed.get(id)
    }

    pub fn contains(&self, id: &NoteId) -> bool {
        self.parsed.contains_key(id)
    }

    pub fn duplicates(&self) -> &[kknow_core::Duplicate] {
        self.vault.duplicates()
    }

    pub fn path_for(&self, id: &NoteId) -> PathBuf {
        self.vault.path_for(id)
    }

    pub fn read(&self, id: &NoteId) -> Result<String> {
        let content = self.vault.read(id)?;
        if kknow_crypto::locked::is_locked(content.as_bytes()) {
            return Err(Error::NoteLocked(id.clone()));
        }
        Ok(content)
    }

    pub fn find(&self, raw: &str) -> Option<&NoteId> {
        let id = NoteId::parse(raw)?;
        if let Some((found, _)) = self.parsed.get_key_value(&id) {
            return Some(found);
        }

        let name = id.name().to_lowercase();
        self.parsed
            .keys()
            .find(|candidate| candidate.name().to_lowercase() == name)
    }
}
