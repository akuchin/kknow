use kknow_core::NoteId;

use crate::*;

impl Workspace {
    pub fn save(&mut self, id: &NoteId, content: &str) -> Result<()> {
        self.store(id, content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    pub(crate) fn store(&mut self, id: &NoteId, content: &str) -> Result<()> {
        if self.locked.contains(id) {
            self.store_sealed(id, content)
        } else {
            self.store_plain(id, content)
        }
    }

    pub(crate) fn store_sealed(&mut self, id: &NoteId, content: &str) -> Result<()> {
        let sealed = self.notes_key()?.seal(content)?;
        self.vault.write(id, &sealed)?;
        self.ingest(id, &sealed)?;
        self.anchor_bookmarks(id, &sealed)
    }

    pub(crate) fn store_plain(&mut self, id: &NoteId, content: &str) -> Result<()> {
        self.vault.write(id, content)?;
        self.ingest(id, content)?;
        self.anchor_bookmarks(id, content)
    }

    pub(crate) fn anchor_bookmarks(&mut self, id: &NoteId, content: &str) -> Result<()> {
        if self.locked.contains(id) {
            return self.edit_bookmarks(|bookmarks| {
                bookmarks.forget(id);
            });
        }
        self.edit_bookmarks(|bookmarks| bookmarks.reanchor(id, content))
    }

    pub fn save_if_unchanged(&mut self, id: &NoteId, content: &str, base: &str) -> Result<Saved> {
        let path = self.path_for(id);
        let locked = self.locked.contains(id);
        let disk = match std::fs::read_to_string(&path) {
            Ok(disk) if kknow_crypto::locked::is_locked(disk.as_bytes()) => {
                Some(self.notes_key()?.open(&disk)?)
            }
            Ok(disk) => Some(disk),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(Error::io(&path, source)),
        };

        match disk {
            Some(disk) if disk != base && disk != content => {
                let copy = conflict_copy(id, content);
                if locked {
                    self.store_sealed(&copy, content)?;
                } else {
                    self.store_plain(&copy, content)?;
                }
                self.search.commit()?;
                self.rebuild_graph();
                Ok(Saved::Conflict { copy, disk })
            }
            _ => {
                self.save(id, content)?;
                Ok(Saved::Written)
            }
        }
    }

    pub fn create(&mut self, raw_name: &str) -> Result<NoteId> {
        let id = NoteId::parse(raw_name).ok_or_else(|| Error::InvalidName(raw_name.to_string()))?;
        if self.contains(&id) {
            return Err(Error::AlreadyExists(id));
        }

        let content = format!("# {}\n\n", id.name());
        self.save(&id, &content)?;
        Ok(id)
    }

    pub fn delete(&mut self, id: &NoteId) -> Result<()> {
        self.vault.remove(id)?;
        self.parsed.remove(id);
        self.hashes.remove(id);
        self.locked.remove(id);
        self.search.remove(id)?;
        self.search.commit()?;
        self.rebuild_graph();
        self.edit_bookmarks(|bookmarks| {
            bookmarks.forget(id);
        })
    }

    pub fn rename(&mut self, from: &NoteId, to: &NoteId) -> Result<Relinked> {
        if from == to {
            return Ok(Relinked::default());
        }
        if self.contains(to) {
            return Err(Error::AlreadyExists(to.clone()));
        }
        self.vault.rename(from, to)?;
        self.moved(from, to)?;
        let moves = [(from.clone(), to.clone())];
        self.edit_bookmarks(|bookmarks| bookmarks.rename(&moves))?;
        self.move_proposals(&moves)?;
        self.move_threads(&moves)?;
        let relinked = self.relink(&moves)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    pub fn duplicate(&mut self, id: &NoteId) -> Result<NoteId> {
        let copy = self.free_copy_name(id)?;
        self.vault.copy(id, &copy)?;
        let content = self.vault.read(&copy)?;
        self.ingest(&copy, &content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(copy)
    }

    pub(crate) fn moved(&mut self, from: &NoteId, to: &NoteId) -> Result<()> {
        self.parsed.remove(from);
        self.hashes.remove(from);
        self.locked.remove(from);
        self.search.remove(from)?;
        let content = self.vault.read(to)?;
        self.ingest(to, &content)
    }

    pub(crate) fn free_copy_name(&self, id: &NoteId) -> Result<NoteId> {
        let folder = id
            .folder()
            .map(|folder| format!("{folder}/"))
            .unwrap_or_default();
        let name = id.name();
        for attempt in 1.. {
            let raw = match attempt {
                1 => format!("{folder}{name} copy"),
                n => format!("{folder}{name} copy {n}"),
            };
            let candidate = NoteId::parse(&raw).ok_or_else(|| Error::InvalidName(raw.clone()))?;
            if !self.contains(&candidate) && !self.vault.path_for(&candidate).exists() {
                return Ok(candidate);
            }
        }
        unreachable!("the endless name search ended")
    }
}
