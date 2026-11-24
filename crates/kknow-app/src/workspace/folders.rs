use std::collections::{BTreeSet, HashMap};

use kknow_core::NoteId;
use kknow_index::{Relink, Resolver};

use crate::*;

impl Workspace {
    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<Relinked> {
        let moved = self.vault.rename_folder(from, to)?;
        for (old, new) in &moved {
            self.moved(old, new)?;
        }
        self.edit_bookmarks(|bookmarks| bookmarks.rename(&moved))?;
        self.move_proposals(&moved)?;
        self.move_threads(&moved)?;
        if moved.is_empty() {
            return Ok(Relinked::default());
        }
        let relinked = self.relink(&moved)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    pub(crate) fn relink(&mut self, moves: &[(NoteId, NoteId)]) -> Result<Relinked> {
        let map: HashMap<NoteId, NoteId> = moves.iter().cloned().collect();
        let back: HashMap<&NoteId, &NoteId> = moves.iter().map(|(old, new)| (new, old)).collect();
        let names: BTreeSet<String> = moves
            .iter()
            .map(|(_, new)| new.name().to_lowercase())
            .collect();

        let mut sources: BTreeSet<&NoteId> = moves.iter().map(|(old, _)| old).collect();
        let namesakes = self
            .parsed
            .keys()
            .filter(|id| names.contains(&id.name().to_lowercase()))
            .map(|id| back.get(id).copied().unwrap_or(id));
        for target in moves.iter().map(|(old, _)| old).chain(namesakes) {
            sources.extend(self.graph.backlinks(target).iter().map(|edge| &edge.from));
        }

        let after = Resolver::new(self.parsed.keys());
        let relink = Relink {
            before: &self.resolver,
            after: &after,
            moves: &map,
        };
        let mut edits = Vec::new();
        for old in sources {
            let id = map.get(old).unwrap_or(old);
            let Some(parsed) = self.parsed.get(id) else {
                continue;
            };
            let content = self.vault.read(id)?;
            if let Some((text, links)) = relink.apply(old, &content, parsed) {
                edits.push((id.clone(), text, links));
            }
        }

        let mut relinked = Relinked::default();
        for (id, text, links) in edits {
            self.store(&id, &text)?;
            relinked.notes += 1;
            relinked.links += links;
        }
        Ok(relinked)
    }

    pub fn folders(&self) -> Result<Vec<String>> {
        Ok(self.vault.folders()?)
    }

    pub fn create_folder(&self, raw: &str) -> Result<()> {
        Ok(self.vault.create_folder(raw)?)
    }

    pub fn delete_folder(&mut self, raw: &str) -> Result<()> {
        let removed = self.vault.remove_folder(raw)?;
        for id in &removed {
            self.parsed.remove(id);
            self.hashes.remove(id);
            self.locked.remove(id);
            self.search.remove(id)?;
        }
        if !removed.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }
        self.edit_bookmarks(|bookmarks| {
            for id in &removed {
                bookmarks.forget(id);
            }
        })
    }
}
