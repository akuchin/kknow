use kknow_core::NoteId;
use kknow_index::{LinkGraph, Resolver, SearchIndex};

use crate::watch::VaultEvent;
use crate::*;

impl Workspace {
    pub(crate) fn reparse_all(&mut self) -> Result<()> {
        let ids: Vec<NoteId> = self.vault.ids().cloned().collect();

        self.parsed.clear();
        self.hashes.clear();
        self.locked.clear();
        self.search = SearchIndex::new()?;

        for id in ids {
            let content = self.vault.read(&id)?;
            self.ingest(&id, &content)?;
        }

        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    pub(crate) fn ingest(&mut self, id: &NoteId, content: &str) -> Result<()> {
        let locked = kknow_crypto::locked::is_locked(content.as_bytes());
        let parsed = kknow_index::parse(if locked { "" } else { content });
        if locked {
            self.locked.insert(id.clone());
        } else {
            self.locked.remove(id);
        }

        let title = parsed.title().unwrap_or_else(|| id.name()).to_string();
        let tags: Vec<String> = parsed.tags.iter().map(|tag| tag.name.clone()).collect();
        self.search.upsert(id, &title, &parsed.text, &tags)?;

        self.hashes.insert(id.clone(), hash(content));
        self.parsed.insert(id.clone(), parsed);
        Ok(())
    }

    pub(crate) fn rebuild_graph(&mut self) {
        self.graph = LinkGraph::build(self.parsed.iter().map(|(id, note)| (id.clone(), note)));
        self.resolver = Resolver::new(self.parsed.keys());
    }

    pub fn apply(&mut self, events: &[VaultEvent]) -> Result<Changes> {
        let mut changes = Changes::default();

        for event in events {
            match event {
                VaultEvent::Changed(id) => {
                    if self.vault.refresh(id)?.is_none() {
                        continue;
                    }
                    let content = self.vault.read(id)?;

                    if self.hashes.get(id) == Some(&hash(&content)) {
                        continue;
                    }

                    self.ingest(id, &content)?;
                    self.anchor_bookmarks(id, &content)?;
                    changes.updated.push(id.clone());
                }
                VaultEvent::Removed(id) => {
                    self.vault.refresh(id)?;
                    if self.parsed.remove(id).is_some() {
                        self.hashes.remove(id);
                        self.locked.remove(id);
                        self.search.remove(id)?;
                        changes.removed.push(id.clone());
                    }
                }
            }
        }

        if !changes.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }

        Ok(changes)
    }
}
