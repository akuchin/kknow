use kknow_core::{NoteId, Trash};

use crate::*;

impl Workspace {
    pub fn trash(&self) -> Trash {
        self.vault.trash()
    }

    pub fn restore(&mut self, entry: &str) -> Result<Vec<NoteId>> {
        let restored = self.vault.restore(entry)?;
        for id in &restored {
            let content = self.vault.read(id)?;
            self.ingest(id, &content)?;
        }
        if !restored.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }
        Ok(restored)
    }
}
