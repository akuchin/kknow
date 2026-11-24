use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use crate::error::{Error, Result};
use crate::note::{MARKDOWN_EXTENSIONS, Note, NoteId, is_markdown_extension};
use crate::trash::Trash;

#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    notes: BTreeMap<NoteId, Note>,
    duplicates: Vec<Duplicate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub id: NoteId,
    pub kept: PathBuf,
    pub skipped: PathBuf,
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
            duplicates: Vec::new(),
        };
        vault.rescan()?;
        Ok(vault)
    }

    pub fn rescan(&mut self) -> Result<()> {
        let root = self.root.clone();
        let mut notes: BTreeMap<NoteId, Note> = BTreeMap::new();
        let mut duplicates = Vec::new();

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

            let note = Note {
                id: id.clone(),
                path: entry.path().to_path_buf(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
            };
            match notes.entry(id.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(note);
                }
                Entry::Occupied(mut slot) => {
                    let skipped = if file_order(&note.path) < file_order(&slot.get().path) {
                        slot.insert(note).path
                    } else {
                        note.path
                    };
                    duplicates.push(Duplicate {
                        id,
                        kept: PathBuf::new(),
                        skipped,
                    });
                }
            }
        }

        for duplicate in &mut duplicates {
            duplicate.kept = notes[&duplicate.id].path.clone();
        }
        duplicates.sort_by(|a, b| (&a.id, &a.skipped).cmp(&(&b.id, &b.skipped)));
        self.notes = notes;
        self.duplicates = duplicates;
        Ok(())
    }

    pub fn duplicates(&self) -> &[Duplicate] {
        &self.duplicates
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

    pub fn path_for(&self, id: &NoteId) -> PathBuf {
        match self.notes.get(id) {
            Some(note) => note.path.clone(),
            None => self.root.join(id.to_relative_path()),
        }
    }

    fn find_file(&self, id: &NoteId) -> Option<PathBuf> {
        MARKDOWN_EXTENSIONS
            .iter()
            .map(|extension| self.root.join(format!("{id}.{extension}")))
            .find(|path| path.is_file())
    }

    fn path_like(&self, id: &NoteId, model: &Path) -> PathBuf {
        match model.extension().and_then(|extension| extension.to_str()) {
            Some(extension) => self.root.join(format!("{id}.{extension}")),
            None => self.root.join(id.to_relative_path()),
        }
    }

    pub fn read(&self, id: &NoteId) -> Result<String> {
        let note = self
            .get(id)
            .ok_or_else(|| Error::NoteNotFound(id.clone()))?;
        fs::read_to_string(&note.path).map_err(|source| Error::io(&note.path, source))
    }

    pub fn write(&mut self, id: &NoteId, content: &str) -> Result<&Note> {
        let path = self.path_for(id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut temp_name = path.as_os_str().to_owned();
        temp_name.push(".tmp");
        let temp = PathBuf::from(temp_name);
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

    pub fn refresh(&mut self, id: &NoteId) -> Result<Option<&Note>> {
        let stored = self.path_for(id);
        let path = if self.contains(id) && stored.is_file() {
            stored
        } else {
            self.find_file(id).unwrap_or(stored)
        };

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

    pub fn restore(&mut self, entry: &str) -> Result<Vec<NoteId>> {
        let restored = self.trash().restore(entry)?;
        for id in &restored {
            self.refresh(id)?;
        }
        Ok(restored)
    }

    pub fn rename(&mut self, from: &NoteId, to: &NoteId) -> Result<&Note> {
        let source = self
            .get(from)
            .ok_or_else(|| Error::NoteNotFound(from.clone()))?
            .path
            .clone();
        let target = self.path_like(to, &source);
        if from != to && (self.contains(to) || is_taken(&target, &source)) {
            return Err(Error::AlreadyExists(target));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        fs::rename(&source, &target).map_err(|source| Error::io(&target, source))?;

        let moved = self.notes.remove(from).expect("the note was found above");
        let note = Note {
            id: to.clone(),
            path: target,
            size: moved.size,
            modified: moved.modified,
        };
        Ok(self.notes.entry(to.clone()).or_insert(note))
    }

    pub fn copy(&mut self, from: &NoteId, to: &NoteId) -> Result<&Note> {
        let source = self
            .get(from)
            .ok_or_else(|| Error::NoteNotFound(from.clone()))?
            .path
            .clone();
        let target = self.path_like(to, &source);
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
            let note = self
                .notes
                .remove(&old)
                .expect("collected from the keys above");
            let renamed = format!("{to_key}/{}", &old.as_str()[from_prefix.len()..]);
            let new = NoteId::parse(&renamed).expect("the parts were valid");
            let moved_path = match note.path.strip_prefix(&source) {
                Ok(rest) => target.join(rest),
                Err(_) => self.root.join(new.to_relative_path()),
            };
            self.notes.insert(
                new.clone(),
                Note {
                    id: new.clone(),
                    path: moved_path,
                    size: note.size,
                    modified: note.modified,
                },
            );
            moved.push((old, new));
        }
        Ok(moved)
    }

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

    pub fn create_folder(&self, relative: &str) -> Result<()> {
        let key = folder_key(relative)?;
        let path = self.root.join(&key);
        fs::create_dir_all(&path).map_err(|source| Error::io(&path, source))
    }

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

fn file_order(path: &Path) -> (usize, &Path) {
    let rank = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| {
            MARKDOWN_EXTENSIONS
                .iter()
                .position(|known| known.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(MARKDOWN_EXTENSIONS.len());
    (rank, path)
}

fn has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_markdown_extension)
}

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
        assert_eq!(vault.trash().entries().unwrap()[0].notes.len(), 1);

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

    fn vault_with(files: &[&str]) -> (tempfile::TempDir, Vault) {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "text").unwrap();
        }
        let vault = Vault::open(dir.path()).unwrap();
        (dir, vault)
    }

    fn id(text: &str) -> NoteId {
        NoteId::parse(text).unwrap()
    }

    #[test]
    fn write_keeps_the_real_file_of_a_note() {
        let (dir, mut vault) = vault_with(&["a.markdown"]);
        vault.write(&id("a"), "new").unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join("a.markdown")).unwrap(),
            "new"
        );
        assert!(!dir.path().join("a.md").exists());
        assert!(!dir.path().join("a.markdown.tmp").exists());
    }

    #[test]
    fn a_new_note_gets_the_md_extension() {
        let (dir, mut vault) = vault_with(&["a.markdown"]);
        assert_eq!(vault.path_for(&id("b")), vault.root().join("b.md"));
        vault.write(&id("b"), "x").unwrap();
        assert!(dir.path().join("b.md").is_file());
    }

    #[test]
    fn refresh_keeps_the_real_file_after_an_external_change() {
        let (dir, mut vault) = vault_with(&["a.markdown"]);
        fs::write(dir.path().join("a.markdown"), "changed").unwrap();

        let note = vault.refresh(&id("a")).unwrap().unwrap();

        assert!(note.path.ends_with("a.markdown"));
        assert_eq!(vault.read(&id("a")).unwrap(), "changed");
    }

    #[test]
    fn refresh_finds_an_unknown_note_with_another_extension() {
        let (dir, mut vault) = vault_with(&[]);
        fs::write(dir.path().join("b.mdown"), "text").unwrap();

        let note = vault.refresh(&id("b")).unwrap().unwrap();

        assert!(note.path.ends_with("b.mdown"));
    }

    #[test]
    fn rename_and_copy_keep_the_source_extension() {
        let (dir, mut vault) = vault_with(&["a.markdown"]);
        vault.copy(&id("a"), &id("b")).unwrap();
        vault.rename(&id("a"), &id("c")).unwrap();

        assert!(dir.path().join("b.markdown").is_file());
        assert!(dir.path().join("c.markdown").is_file());
        assert!(!dir.path().join("a.markdown").exists());
        assert!(!dir.path().join("b.md").exists());
        assert!(!dir.path().join("c.md").exists());
    }

    #[test]
    fn rename_folder_keeps_the_extensions() {
        let (dir, mut vault) = vault_with(&["Old/a.markdown", "Old/b.md"]);
        vault.rename_folder("Old", "New").unwrap();

        assert!(
            vault
                .get(&id("New/a"))
                .unwrap()
                .path
                .ends_with("New/a.markdown")
        );
        assert!(vault.get(&id("New/b")).unwrap().path.ends_with("New/b.md"));
        assert!(dir.path().join("New/a.markdown").is_file());
    }

    #[test]
    fn rescan_keeps_one_file_per_note_and_reports_the_other() {
        for order in [["a.md", "a.markdown"], ["a.markdown", "a.md"]] {
            let (dir, vault) = vault_with(&order);

            assert_eq!(vault.len(), 1);
            assert!(vault.get(&id("a")).unwrap().path.ends_with("a.md"));
            assert_eq!(
                vault.duplicates(),
                [Duplicate {
                    id: id("a"),
                    kept: vault.root().join("a.md"),
                    skipped: vault.root().join("a.markdown"),
                }]
            );
            assert!(dir.path().join("a.md").is_file());
            assert!(dir.path().join("a.markdown").is_file());
        }
    }

    #[test]
    fn open_rejects_a_file_as_root() {
        let dir = fixture();
        let file = dir.path().join("Index.md");
        assert!(matches!(Vault::open(&file), Err(Error::NotADirectory(_))));
    }
}
