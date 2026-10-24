use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use crate::error::{Error, Result};
use crate::note::{Note, NoteId, is_markdown_extension};
use crate::trash::Trash;

/// A folder of notes on disk.
///
/// The vault keeps only metadata in memory and reads content on demand. So
/// opening a large vault costs one directory walk.
#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    notes: BTreeMap<NoteId, Note>,
}

impl Vault {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(Error::NotADirectory(root.to_path_buf()));
        }
        let root = root
            .canonicalize()
            .map_err(|source| Error::io(root, source))?;

        let mut vault = Vault {
            root,
            notes: BTreeMap::new(),
        };
        vault.rescan()?;
        Ok(vault)
    }

    /// Reads the file tree again from scratch. For one changed file, use
    /// [`Vault::refresh`].
    pub fn rescan(&mut self) -> Result<()> {
        let root = self.root.clone();
        let mut notes = BTreeMap::new();

        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_hidden(entry));

        for entry in walker {
            let entry = entry.map_err(|err| walk_error(&root, err))?;

            if !entry.file_type().is_file() || !has_markdown_extension(entry.path()) {
                continue;
            }

            let relative = entry
                .path()
                .strip_prefix(&root)
                .map_err(|_| Error::OutsideVault(entry.path().to_path_buf()))?;
            let Some(id) = NoteId::from_relative_path(relative) else {
                continue;
            };

            let metadata = entry
                .metadata()
                .map_err(|err| Error::io(entry.path(), io::Error::other(err)))?;

            notes.insert(
                id.clone(),
                Note {
                    id,
                    path: entry.path().to_path_buf(),
                    size: metadata.len(),
                    modified: metadata.modified().ok(),
                },
            );
        }

        self.notes = notes;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Notes in lexicographic order of their ids.
    pub fn notes(&self) -> impl ExactSizeIterator<Item = &Note> {
        self.notes.values()
    }

    pub fn ids(&self) -> impl ExactSizeIterator<Item = &NoteId> {
        self.notes.keys()
    }

    pub fn get(&self, id: &NoteId) -> Option<&Note> {
        self.notes.get(id)
    }

    pub fn contains(&self, id: &NoteId) -> bool {
        self.notes.contains_key(id)
    }

    /// Absolute path to the note file. The note can exist or not exist yet.
    pub fn path_for(&self, id: &NoteId) -> PathBuf {
        self.root.join(id.to_relative_path())
    }

    pub fn read(&self, id: &NoteId) -> Result<String> {
        let note = self
            .get(id)
            .ok_or_else(|| Error::NoteNotFound(id.clone()))?;
        fs::read_to_string(&note.path).map_err(|source| Error::io(&note.path, source))
    }

    /// Writes the note and creates it if necessary.
    ///
    /// The write goes through a temporary file next to the target. A rename on
    /// one file system is atomic, so a reader never sees half a note.
    pub fn write(&mut self, id: &NoteId, content: &str) -> Result<&Note> {
        let path = self.path_for(id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let temp = path.with_extension("md.tmp");
        fs::write(&temp, content).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, &path).map_err(|source| Error::io(&path, source))?;

        let metadata = fs::metadata(&path).map_err(|source| Error::io(&path, source))?;
        let note = Note {
            id: id.clone(),
            path,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        };
        match self.notes.entry(id.clone()) {
            Entry::Occupied(mut existing) => {
                existing.insert(note);
                Ok(existing.into_mut())
            }
            Entry::Vacant(slot) => Ok(slot.insert(note)),
        }
    }

    /// Updates the metadata of one note from its file on disk.
    ///
    /// If the file is gone, the note leaves the vault. This handles edits made
    /// outside the app: a full [`Vault::rescan`] for one file is too costly.
    pub fn refresh(&mut self, id: &NoteId) -> Result<Option<&Note>> {
        let path = self.path_for(id);

        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.notes.remove(id);
                return Ok(None);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.notes.remove(id);
                return Ok(None);
            }
            Err(source) => return Err(Error::io(&path, source)),
        };

        let note = Note {
            id: id.clone(),
            path,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        };
        self.notes.insert(id.clone(), note);
        Ok(self.notes.get(id))
    }

    /// Moves the note to the [`Trash`]. Nothing leaves the disk for good.
    pub fn remove(&mut self, id: &NoteId) -> Result<()> {
        let note = self
            .get(id)
            .ok_or_else(|| Error::NoteNotFound(id.clone()))?;
        let relative = note
            .path
            .strip_prefix(&self.root)
            .map_err(|_| Error::OutsideVault(note.path.clone()))?
            .to_path_buf();
        self.trash().put(&relative)?;
        self.notes.remove(id);
        Ok(())
    }

    pub fn trash(&self) -> Trash {
        Trash::new(&self.root)
    }

    /// Restores a trash entry and returns its notes, now in the vault again.
    pub fn restore(&mut self, entry: &str) -> Result<Vec<NoteId>> {
        let restored = self.trash().restore(entry)?;
        for id in &restored {
            self.refresh(id)?;
        }
        Ok(restored)
    }

    /// Moves the note to a new id.
    ///
    /// Rename and move to another folder are one operation: the id is the
    /// path, and only a different part of it changes. Creates the target
    /// folder. Does not overwrite a taken path.
    ///
    /// On a case-insensitive disk `Note` and `note` are one file, and this
    /// rename is valid too: "taken" here means "taken by another file".
    pub fn rename(&mut self, from: &NoteId, to: &NoteId) -> Result<&Note> {
        let source = self
            .get(from)
            .ok_or_else(|| Error::NoteNotFound(from.clone()))?
            .path
            .clone();
        let target = self.path_for(to);
        if from != to && is_taken(&target, &source) {
            return Err(Error::AlreadyExists(target));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        fs::rename(&source, &target).map_err(|source| Error::io(&target, source))?;

        let moved = self.notes.remove(from).expect("заметка найдена выше");
        let note = Note {
            id: to.clone(),
            path: target,
            size: moved.size,
            modified: moved.modified,
        };
        Ok(self.notes.entry(to.clone()).or_insert(note))
    }

    /// Copies the note to a new id. Does not overwrite a taken path.
    pub fn copy(&mut self, from: &NoteId, to: &NoteId) -> Result<&Note> {
        let source = self
            .get(from)
            .ok_or_else(|| Error::NoteNotFound(from.clone()))?
            .path
            .clone();
        let target = self.path_for(to);
        if self.contains(to) || target.exists() {
            return Err(Error::AlreadyExists(target));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        fs::copy(&source, &target).map_err(|source| Error::io(&target, source))?;

        let metadata = fs::metadata(&target).map_err(|source| Error::io(&target, source))?;
        let note = Note {
            id: to.clone(),
            path: target,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        };
        Ok(self.notes.entry(to.clone()).or_insert(note))
    }

    /// Moves a folder with all its content to a new path.
    ///
    /// Returns "old, new" id pairs for the notes inside: the vault knows
    /// nothing about search and the graph, and the caller must rebuild them.
    /// A move of a folder into itself has no meaning, so it is rejected before
    /// any disk operation.
    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<Vec<(NoteId, NoteId)>> {
        let from_key = folder_key(from)?;
        let to_key = folder_key(to)?;
        if from_key == to_key {
            return Ok(Vec::new());
        }
        let from_prefix = format!("{from_key}/");
        if to_key.starts_with(&from_prefix) {
            return Err(Error::InvalidFolder(to.to_string()));
        }

        let source = self.root.join(&from_key);
        if !source.is_dir() {
            return Err(Error::NotADirectory(source));
        }
        let target = self.root.join(&to_key);
        if is_taken(&target, &source) {
            return Err(Error::AlreadyExists(target));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        fs::rename(&source, &target).map_err(|source| Error::io(&target, source))?;

        let inside: Vec<NoteId> = self
            .notes
            .keys()
            .filter(|id| id.as_str().starts_with(&from_prefix))
            .cloned()
            .collect();

        let mut moved = Vec::with_capacity(inside.len());
        for old in inside {
            let note = self.notes.remove(&old).expect("собрано из ключей выше");
            let renamed = format!("{to_key}/{}", &old.as_str()[from_prefix.len()..]);
            let new = NoteId::parse(&renamed).expect("части были корректными");
            self.notes.insert(
                new.clone(),
                Note {
                    id: new.clone(),
                    path: self.path_for(&new),
                    size: note.size,
                    modified: note.modified,
                },
            );
            moved.push((old, new));
        }
        Ok(moved)
    }

    /// All vault folders, empty ones included. Empty folders have no notes, so
    /// the note list cannot give them, but the side panel tree shows them.
    pub fn folders(&self) -> Result<Vec<String>> {
        let walker = WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| !is_hidden(entry));

        let mut folders = Vec::new();
        for entry in walker {
            let entry = entry.map_err(|err| walk_error(&self.root, err))?;
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&self.root)
                .map_err(|_| Error::OutsideVault(entry.path().to_path_buf()))?;
            if let Some(text) = relative.to_str() {
                folders.push(text.replace(std::path::MAIN_SEPARATOR, "/"));
            }
        }
        Ok(folders)
    }

    /// Creates the folder with its parents. An existing folder is not an error.
    pub fn create_folder(&self, relative: &str) -> Result<()> {
        let key = folder_key(relative)?;
        let path = self.root.join(&key);
        fs::create_dir_all(&path).map_err(|source| Error::io(&path, source))
    }

    /// Moves a folder with all its content to the [`Trash`].
    ///
    /// Returns the ids of the removed notes: the vault knows nothing about
    /// search and the graph, and the caller must clean them up.
    pub fn remove_folder(&mut self, relative: &str) -> Result<Vec<NoteId>> {
        let key = folder_key(relative)?;
        let path = self.root.join(&key);
        if !path.is_dir() {
            return Err(Error::NotADirectory(path));
        }
        self.trash().put(Path::new(&key))?;

        let prefix = format!("{key}/");
        let removed: Vec<NoteId> = self
            .notes
            .keys()
            .filter(|id| id.as_str().starts_with(&prefix))
            .cloned()
            .collect();
        for id in &removed {
            self.notes.remove(id);
        }
        Ok(removed)
    }
}

/// Folder path from the vault root in canonical form: `/` as the separator,
/// no empty segments.
///
/// All segments that start with a dot are rejected: `..` leaves the vault,
/// and the scanner does not see hidden folders, so such a "created" folder
/// looks as if it does not exist.
fn folder_key(relative: &str) -> Result<String> {
    let normalized = relative.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');
    let valid = !trimmed.is_empty()
        && trimmed
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'));
    if !valid {
        return Err(Error::InvalidFolder(relative.to_string()));
    }
    Ok(trimmed.to_string())
}

/// Checks if the path is taken by something other than the moved file.
///
/// On a case-insensitive disk `exists()` for `note.md` says "yes" when the disk
/// has `Note.md`, and this is the same file. Both have one canonical path, so
/// it tells "taken" from "the same file".
fn is_taken(target: &Path, source: &Path) -> bool {
    if !target.exists() {
        return false;
    }
    match (target.canonicalize(), source.canonicalize()) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    }
}

fn walk_error(root: &Path, err: walkdir::Error) -> Error {
    let path = err.path().unwrap_or(root).to_path_buf();
    let source = err
        .into_io_error()
        .unwrap_or_else(|| io::Error::other("directory loop"));
    Error::io(path, source)
}

fn has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_markdown_extension)
}

/// Hidden files and service folders (`.git`, `.trash`, our `.kknow`) are not
/// part of the vault.
pub(crate) fn is_hidden(entry: &DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .is_none_or(|name| name.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Projects/Rust")).unwrap();
        fs::create_dir_all(root.join(".trash")).unwrap();
        fs::write(root.join("Index.md"), "# Index\n[[Projects/Rust/kknow]]").unwrap();
        fs::write(root.join("Projects/Rust/kknow.md"), "# kknow\n").unwrap();
        fs::write(root.join("Projects/notes.txt"), "not a note").unwrap();
        fs::write(root.join(".trash/workspace.md"), "internal").unwrap();
        dir
    }

    #[test]
    fn scans_markdown_and_skips_hidden_and_other_extensions() {
        let dir = fixture();
        let vault = Vault::open(dir.path()).unwrap();

        let ids: Vec<&str> = vault.ids().map(NoteId::as_str).collect();
        assert_eq!(ids, ["Index", "Projects/Rust/kknow"]);
    }

    #[test]
    fn folders_include_empty_and_skip_hidden() {
        let dir = fixture();
        fs::create_dir(dir.path().join("Пустая")).unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        let mut folders = vault.folders().unwrap();
        folders.sort();
        assert_eq!(folders, ["Projects", "Projects/Rust", "Пустая"]);
    }

    #[test]
    fn create_folder_makes_nested_dirs_and_rejects_bad_paths() {
        let dir = fixture();
        let vault = Vault::open(dir.path()).unwrap();

        vault.create_folder("Новая/Вложенная").unwrap();
        assert!(dir.path().join("Новая/Вложенная").is_dir());

        assert!(matches!(
            vault.create_folder("../evil"),
            Err(Error::InvalidFolder(_))
        ));
        assert!(matches!(
            vault.create_folder(".hidden"),
            Err(Error::InvalidFolder(_))
        ));
        assert!(matches!(
            vault.create_folder(""),
            Err(Error::InvalidFolder(_))
        ));
    }

    #[test]
    fn remove_folder_moves_files_to_the_trash_and_returns_note_ids() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();

        let removed = vault.remove_folder("Projects").unwrap();
        let ids: Vec<&str> = removed.iter().map(NoteId::as_str).collect();
        assert_eq!(ids, ["Projects/Rust/kknow"]);
        assert!(!dir.path().join("Projects").exists());
        assert!(!vault.contains(&NoteId::parse("Projects/Rust/kknow").unwrap()));
        assert_eq!(vault.trash().entries().unwrap()[0].notes, 1);

        assert!(matches!(
            vault.remove_folder("Projects"),
            Err(Error::NotADirectory(_))
        ));
    }

    #[test]
    fn remove_moves_the_note_to_the_trash() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let id = NoteId::parse("Projects/Rust/kknow").unwrap();

        vault.remove(&id).unwrap();
        assert!(!vault.contains(&id));
        assert!(!dir.path().join("Projects/Rust/kknow.md").exists());

        // The trash is hidden: a rescan does not bring the note back.
        vault.rescan().unwrap();
        assert!(!vault.contains(&id));

        let entries = vault.trash().entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "Projects/Rust/kknow.md");

        assert_eq!(
            vault.restore(&entries[0].id).unwrap(),
            std::slice::from_ref(&id)
        );
        assert!(vault.contains(&id));
        assert_eq!(vault.read(&id).unwrap(), "# kknow\n");
    }

    #[test]
    fn rename_moves_the_file_and_the_id() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let from = NoteId::parse("Projects/Rust/kknow").unwrap();
        let to = NoteId::parse("Archive/2026/kknow-old").unwrap();

        let note = vault.rename(&from, &to).unwrap();
        assert_eq!(note.id, to);

        assert!(dir.path().join("Archive/2026/kknow-old.md").is_file());
        assert!(!dir.path().join("Projects/Rust/kknow.md").exists());
        assert!(!vault.contains(&from));
        assert_eq!(vault.read(&to).unwrap(), "# kknow\n");
    }

    #[test]
    fn rename_refuses_to_overwrite_another_note() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let from = NoteId::parse("Projects/Rust/kknow").unwrap();
        let to = NoteId::parse("Index").unwrap();

        assert!(matches!(
            vault.rename(&from, &to),
            Err(Error::AlreadyExists(_))
        ));
        assert!(vault.contains(&from));
        assert_eq!(vault.read(&to).unwrap(), "# Index\n[[Projects/Rust/kknow]]");
    }

    #[test]
    fn rename_to_itself_changes_nothing() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let id = NoteId::parse("Index").unwrap();

        vault.rename(&id, &id).unwrap();

        assert!(vault.contains(&id));
        assert_eq!(vault.len(), 2);
    }

    #[test]
    fn copy_makes_a_second_note_with_the_same_content() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let from = NoteId::parse("Projects/Rust/kknow").unwrap();
        let to = NoteId::parse("Projects/Rust/kknow copy").unwrap();

        vault.copy(&from, &to).unwrap();

        assert!(vault.contains(&from));
        assert!(vault.contains(&to));
        assert_eq!(vault.read(&to).unwrap(), "# kknow\n");
        assert!(matches!(
            vault.copy(&from, &to),
            Err(Error::AlreadyExists(_))
        ));
    }

    #[test]
    fn rename_folder_moves_notes_and_reports_their_new_ids() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();

        let moved = vault.rename_folder("Projects", "Work/Projects").unwrap();

        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].0.as_str(), "Projects/Rust/kknow");
        assert_eq!(moved[0].1.as_str(), "Work/Projects/Rust/kknow");
        assert!(dir.path().join("Work/Projects/Rust/kknow.md").is_file());
        assert!(!dir.path().join("Projects").exists());
        assert!(vault.contains(&moved[0].1));
        assert!(!vault.contains(&moved[0].0));
        assert_eq!(vault.read(&moved[0].1).unwrap(), "# kknow\n");
    }

    #[test]
    fn rename_folder_rejects_nonsense_targets() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        fs::create_dir(dir.path().join("Taken")).unwrap();

        assert!(matches!(
            vault.rename_folder("Projects", "Projects/Rust/inside"),
            Err(Error::InvalidFolder(_))
        ));
        assert!(matches!(
            vault.rename_folder("Projects", "Taken"),
            Err(Error::AlreadyExists(_))
        ));
        assert!(matches!(
            vault.rename_folder("Nope", "Other"),
            Err(Error::NotADirectory(_))
        ));
        assert!(
            vault
                .rename_folder("Projects", "Projects")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn reads_note_content_by_id() {
        let dir = fixture();
        let vault = Vault::open(dir.path()).unwrap();

        let id = NoteId::parse("Projects/Rust/kknow").unwrap();
        assert_eq!(vault.read(&id).unwrap(), "# kknow\n");
    }

    #[test]
    fn read_reports_missing_note() {
        let dir = fixture();
        let vault = Vault::open(dir.path()).unwrap();

        let missing = NoteId::parse("nope").unwrap();
        assert!(matches!(
            vault.read(&missing),
            Err(Error::NoteNotFound(id)) if id == missing
        ));
    }

    #[test]
    fn write_creates_folders_and_registers_the_note() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();

        let id = NoteId::parse("Daily/2026-07-18").unwrap();
        vault.write(&id, "# Today\n").unwrap();

        assert!(vault.contains(&id));
        assert_eq!(vault.read(&id).unwrap(), "# Today\n");
        assert!(dir.path().join("Daily/2026-07-18.md").is_file());

        // The temporary file must not survive the write.
        assert!(!dir.path().join("Daily/2026-07-18.md.tmp").exists());
    }

    #[test]
    fn rescan_picks_up_external_changes() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        assert_eq!(vault.len(), 2);

        fs::write(dir.path().join("Inbox.md"), "captured elsewhere").unwrap();
        vault.rescan().unwrap();

        assert_eq!(vault.len(), 3);
        assert!(vault.contains(&NoteId::parse("Inbox").unwrap()));
    }

    #[test]
    fn refresh_picks_up_a_single_external_change() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let id = NoteId::parse("Inbox").unwrap();

        fs::write(dir.path().join("Inbox.md"), "написано снаружи").unwrap();
        assert!(vault.refresh(&id).unwrap().is_some());

        assert!(vault.contains(&id));
        assert_eq!(vault.read(&id).unwrap(), "написано снаружи");
    }

    #[test]
    fn refresh_drops_a_deleted_note() {
        let dir = fixture();
        let mut vault = Vault::open(dir.path()).unwrap();
        let id = NoteId::parse("Index").unwrap();

        fs::remove_file(dir.path().join("Index.md")).unwrap();
        assert!(vault.refresh(&id).unwrap().is_none());

        assert!(!vault.contains(&id));
    }

    #[test]
    fn open_rejects_a_file_as_root() {
        let dir = fixture();
        let file = dir.path().join("Index.md");
        assert!(matches!(Vault::open(&file), Err(Error::NotADirectory(_))));
    }
}
