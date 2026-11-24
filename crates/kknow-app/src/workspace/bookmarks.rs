use kknow_core::NoteId;

use crate::bookmarks::{Bookmark, Bookmarks, Mark};
use crate::*;

impl Workspace {
    pub(crate) fn load_bookmarks(&mut self) -> Result<()> {
        self.bookmarks = Bookmarks::load(&Bookmarks::path(self.vault.root()))?.bookmarks;
        Ok(())
    }

    pub(crate) fn edit_bookmarks(&mut self, edit: impl FnOnce(&mut Bookmarks)) -> Result<()> {
        let before = self.bookmarks.clone();
        edit(&mut self.bookmarks);
        if self.bookmarks == before {
            return Ok(());
        }
        self.bookmarks.save(&Bookmarks::path(self.vault.root()))
    }

    pub fn bookmarks(&self) -> Vec<&Bookmark> {
        self.bookmarks
            .all()
            .iter()
            .filter(|bookmark| NoteId::parse(&bookmark.note).is_some_and(|id| self.contains(&id)))
            .collect()
    }

    pub fn note_bookmarks(&self, id: &NoteId, content: &str) -> Vec<Mark> {
        let last = content.lines().count();
        if last == 0 {
            return Vec::new();
        }
        let mut marks: Vec<Mark> = self
            .bookmarks
            .for_note(id)
            .into_iter()
            .map(|bookmark| Mark {
                line: bookmark
                    .locate(content)
                    .unwrap_or(bookmark.line)
                    .clamp(1, last),
                name: bookmark.name.clone(),
            })
            .collect();
        marks.sort_by_key(|mark| (mark.line, mark.name.is_none()));
        marks.dedup_by_key(|mark| mark.line);
        marks
    }

    pub fn toggle_bookmark(&mut self, id: &NoteId, line: usize) -> Result<bool> {
        let content = self.read_plain(id)?;
        let mut set = false;
        self.edit_bookmarks(|bookmarks| set = bookmarks.toggle(id, line, &content))?;
        Ok(set)
    }

    pub fn name_bookmark(&mut self, id: &NoteId, line: usize, name: Option<&str>) -> Result<bool> {
        if self.locked.contains(id) {
            return Err(Error::NoteLocked(id.clone()));
        }
        let mut found = false;
        self.edit_bookmarks(|bookmarks| found = bookmarks.rename_mark(id, line, name))?;
        Ok(found)
    }

    pub fn set_bookmarks(&mut self, id: &NoteId, marks: &[Mark], content: &str) -> Result<()> {
        if self.locked.contains(id) {
            return self.anchor_bookmarks(id, "");
        }
        self.edit_bookmarks(|bookmarks| bookmarks.place(id, marks, content))
    }
}
