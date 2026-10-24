//! The vault trash: deleted notes and folders wait here for a restore.
//!
//! The trash is `.kknow/trash` inside the vault. The scanner skips hidden
//! folders, and sync does not include `.kknow`. Each deletion gets its own
//! entry folder, named by the deletion time in Unix seconds. Inside the entry,
//! the item keeps its path relative to the vault root. So two deletions of
//! one path do not collide, and a restore knows where the file was.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::note::{NoteId, is_markdown_extension};

/// How long an entry stays in the trash. A move to the trash purges older
/// entries.
pub const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// One deletion in the trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    /// Name of the entry folder. `kknow trash restore` takes it.
    pub id: String,
    pub deleted: SystemTime,
    /// The deleted item relative to the vault root, with `/` as the separator.
    ///
    /// The entry stores no metadata. So this is the longest path with one
    /// child at each level: a deleted folder with one subfolder shows the
    /// subfolder.
    pub path: String,
    /// Number of notes in the entry.
    pub notes: usize,
}

/// The trash of one vault.
#[derive(Debug, Clone)]
pub struct Trash {
    root: PathBuf,
    dir: PathBuf,
}

impl Trash {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let dir = root.join(".kknow").join("trash");
        Trash { root, dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Moves a file or a folder from the vault into a new entry.
    ///
    /// The trash is on the same file system as the vault, so this is one
    /// atomic `rename`. The watcher sees the old path disappear, as on a
    /// delete. The new path is hidden, and the watcher skips it.
    pub(crate) fn put(&self, relative: &Path) -> Result<()> {
        let now = unix_now();
        self.purge(now)?;

        let entry = self.new_entry(now)?;
        let source = self.root.join(relative);
        let target = entry.join(relative);
        let moved = target
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(&source, &target));
        if let Err(source) = moved {
            // An empty entry would show in the list as a deletion of nothing.
            let _ = fs::remove_dir_all(&entry);
            return Err(Error::io(&target, source));
        }
        Ok(())
    }

    /// Entries from the oldest to the newest. A vault without a trash has
    /// none.
    pub fn entries(&self) -> Result<Vec<TrashEntry>> {
        let mut entries = Vec::new();
        for (id, path, deleted) in self.entry_dirs()? {
            let notes = WalkDir::new(&path)
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_file() && is_note(entry.path()))
                .count();
            entries.push(TrashEntry {
                path: item_path(&path),
                deleted: UNIX_EPOCH + Duration::from_secs(deleted),
                id,
                notes,
            });
        }
        entries.sort_by(|a, b| (a.deleted, &a.id).cmp(&(b.deleted, &b.id)));
        Ok(entries)
    }

    /// Moves the entry content back to its place and removes the entry.
    ///
    /// Returns the restored notes. Overwrites nothing: if one file of the
    /// entry is taken, nothing moves. A folder that exists again gets the
    /// files of the entry next to its own.
    pub fn restore(&self, id: &str) -> Result<Vec<NoteId>> {
        let entry = self.entry_path(id)?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for item in WalkDir::new(&entry).min_depth(1).sort_by_file_name() {
            let item = item.map_err(|err| Error::io(&entry, io::Error::other(err)))?;
            let relative = item
                .path()
                .strip_prefix(&entry)
                .map_err(|_| Error::OutsideVault(item.path().to_path_buf()))?
                .to_path_buf();
            if item.file_type().is_dir() {
                dirs.push(relative);
            } else {
                files.push(relative);
            }
        }

        if let Some(taken) = files
            .iter()
            .map(|file| self.root.join(file))
            .find(|target| target.exists())
        {
            return Err(Error::AlreadyExists(taken));
        }

        for dir in &dirs {
            let target = self.root.join(dir);
            fs::create_dir_all(&target).map_err(|source| Error::io(&target, source))?;
        }
        for file in &files {
            let target = self.root.join(file);
            fs::rename(entry.join(file), &target).map_err(|source| Error::io(&target, source))?;
        }
        fs::remove_dir_all(&entry).map_err(|source| Error::io(&entry, source))?;

        Ok(files
            .iter()
            .filter(|file| is_note(file))
            .filter_map(|file| NoteId::from_relative_path(file))
            .collect())
    }

    /// Removes all entries for good. Returns their number.
    /// Removes one entry for good.
    pub fn remove(&self, id: &str) -> Result<()> {
        let entry = self.entry_path(id)?;
        fs::remove_dir_all(&entry).map_err(|source| Error::io(&entry, source))
    }

    pub fn empty(&self) -> Result<usize> {
        let dirs = self.entry_dirs()?;
        for (_, path, _) in &dirs {
            fs::remove_dir_all(path).map_err(|source| Error::io(path, source))?;
        }
        Ok(dirs.len())
    }

    /// Removes the entries that are older than [`RETENTION`].
    fn purge(&self, now: u64) -> Result<()> {
        for (_, path, deleted) in self.entry_dirs()? {
            if deleted.saturating_add(RETENTION.as_secs()) < now {
                fs::remove_dir_all(&path).map_err(|source| Error::io(&path, source))?;
            }
        }
        Ok(())
    }

    /// Creates a free entry folder for this second: `<secs>`, then
    /// `<secs>-2`… `create_dir` fails on a taken name, so two deletions in one
    /// second get different entries.
    fn new_entry(&self, now: u64) -> Result<PathBuf> {
        fs::create_dir_all(&self.dir).map_err(|source| Error::io(&self.dir, source))?;
        for attempt in 1.. {
            let name = match attempt {
                1 => now.to_string(),
                n => format!("{now}-{n}"),
            };
            let path = self.dir.join(name);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(Error::io(&path, source)),
            }
        }
        unreachable!("бесконечный перебор имён закончился")
    }

    /// The entry folder by its name. A name that is not an entry name is
    /// "not found": it must not lead outside the trash.
    fn entry_path(&self, id: &str) -> Result<PathBuf> {
        let path = self.dir.join(id);
        if deletion_time(id).is_none() || !path.is_dir() {
            return Err(Error::TrashEntryNotFound(id.to_string()));
        }
        Ok(path)
    }

    /// Entry folders with their names and deletion times. Other files in the
    /// trash folder are not entries.
    fn entry_dirs(&self) -> Result<Vec<(String, PathBuf, u64)>> {
        let read = match fs::read_dir(&self.dir) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::io(&self.dir, source)),
        };

        let mut dirs = Vec::new();
        for item in read {
            let item = item.map_err(|source| Error::io(&self.dir, source))?;
            let Ok(name) = item.file_name().into_string() else {
                continue;
            };
            let Some(deleted) = deletion_time(&name) else {
                continue;
            };
            if item.path().is_dir() {
                dirs.push((name, item.path(), deleted));
            }
        }
        Ok(dirs)
    }
}

/// Deletion time from an entry name: `<secs>` or `<secs>-<n>`.
fn deletion_time(name: &str) -> Option<u64> {
    let (secs, attempt) = match name.split_once('-') {
        Some((secs, attempt)) => (secs, Some(attempt)),
        None => (name, None),
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    if !digits(secs) || attempt.is_some_and(|attempt| !digits(attempt)) {
        return None;
    }
    secs.parse().ok()
}

/// The longest path inside the entry with one child at each level.
fn item_path(entry: &Path) -> String {
    let mut parts = Vec::new();
    let mut current = entry.to_path_buf();
    loop {
        let Ok(read) = fs::read_dir(&current) else {
            break;
        };
        let children: Vec<_> = read.filter_map(|item| item.ok()).collect();
        let [only] = children.as_slice() else {
            break;
        };
        parts.push(only.file_name().to_string_lossy().into_owned());
        current = only.path();
        if !current.is_dir() {
            break;
        }
    }
    parts.join("/")
}

fn is_note(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_markdown_extension)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Trash) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let trash = Trash::new(dir.path());
        (dir, trash)
    }

    #[test]
    fn put_keeps_the_relative_path_inside_an_entry() {
        let (dir, trash) = vault_with(&[("Проекты/план.md", "# План")]);

        trash.put(Path::new("Проекты/план.md")).unwrap();

        assert!(!dir.path().join("Проекты/план.md").exists());
        let entries = trash.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "Проекты/план.md");
        assert_eq!(entries[0].notes, 1);
        let kept = trash.dir().join(&entries[0].id).join("Проекты/план.md");
        assert_eq!(fs::read_to_string(kept).unwrap(), "# План");
    }

    #[test]
    fn two_deletions_of_one_path_coexist() {
        let (dir, trash) = vault_with(&[("план.md", "первая")]);
        trash.put(Path::new("план.md")).unwrap();
        fs::write(dir.path().join("план.md"), "вторая").unwrap();
        trash.put(Path::new("план.md")).unwrap();

        let entries = trash.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].id, entries[1].id);
    }

    #[test]
    fn restore_puts_files_back_and_refuses_to_overwrite() {
        let (dir, trash) = vault_with(&[("Проекты/план.md", "# План")]);
        trash.put(Path::new("Проекты")).unwrap();
        let id = trash.entries().unwrap()[0].id.clone();

        fs::create_dir_all(dir.path().join("Проекты")).unwrap();
        fs::write(dir.path().join("Проекты/план.md"), "новый").unwrap();
        assert!(matches!(trash.restore(&id), Err(Error::AlreadyExists(_))));
        assert_eq!(
            fs::read_to_string(dir.path().join("Проекты/план.md")).unwrap(),
            "новый"
        );

        fs::remove_file(dir.path().join("Проекты/план.md")).unwrap();
        let restored = trash.restore(&id).unwrap();
        assert_eq!(restored, [NoteId::parse("Проекты/план").unwrap()]);
        assert_eq!(
            fs::read_to_string(dir.path().join("Проекты/план.md")).unwrap(),
            "# План"
        );
        assert!(trash.entries().unwrap().is_empty());
    }

    #[test]
    fn restore_rejects_names_that_are_not_entries() {
        let (_dir, trash) = vault_with(&[]);
        for id in ["1700000000", "..", "../..", "a/b", ""] {
            assert!(matches!(
                trash.restore(id),
                Err(Error::TrashEntryNotFound(_))
            ));
        }
    }

    #[test]
    fn put_purges_old_entries() {
        let (_dir, trash) = vault_with(&[("план.md", "# План")]);
        let old = trash.dir().join("1000/старое.md");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "давно").unwrap();

        trash.put(Path::new("план.md")).unwrap();

        let entries = trash.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "план.md");
    }

    #[test]
    fn empty_removes_all_entries() {
        let (_dir, trash) = vault_with(&[("а.md", "а"), ("б.md", "б")]);
        trash.put(Path::new("а.md")).unwrap();
        trash.put(Path::new("б.md")).unwrap();

        assert_eq!(trash.empty().unwrap(), 2);
        assert!(trash.entries().unwrap().is_empty());
        assert_eq!(trash.empty().unwrap(), 0);
    }

    #[test]
    fn remove_drops_one_entry_and_keeps_the_others() {
        let (_dir, trash) = vault_with(&[("а.md", "а"), ("б.md", "б")]);
        trash.put(Path::new("а.md")).unwrap();
        trash.put(Path::new("б.md")).unwrap();
        let first = trash.entries().unwrap()[0].id.clone();

        trash.remove(&first).unwrap();
        let left = trash.entries().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].path, "б.md");
        assert!(matches!(
            trash.remove(&first),
            Err(Error::TrashEntryNotFound(_))
        ));
        assert!(matches!(
            trash.remove(".."),
            Err(Error::TrashEntryNotFound(_))
        ));
    }

    #[test]
    fn entry_names_carry_the_deletion_time() {
        assert_eq!(deletion_time("1700000000"), Some(1_700_000_000));
        assert_eq!(deletion_time("1700000000-2"), Some(1_700_000_000));
        assert_eq!(deletion_time("1700000000-"), None);
        assert_eq!(deletion_time("-2"), None);
        assert_eq!(deletion_time("заметка"), None);
    }
}
