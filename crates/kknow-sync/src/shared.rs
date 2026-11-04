//! Shared files: the parts of `.kknow` that sync carries with the notes.
//!
//! Most of `.kknow` belongs to one machine: the sync state, the storage
//! address, the trash. But some files belong to the notes: bookmarks,
//! proposals of agents and comment threads. Without them, a comment that an
//! agent answered on the laptop would have no answer on the desktop.
//!
//! The list is closed on purpose. A new file in `.kknow` does not sync until
//! someone adds it here, so a file with a secret or a machine path cannot
//! leave by accident.
//!
//! A shared file has no conflict copy, as a note has. A copy of
//! `bookmarks.json` next to it would do nothing: no part of the app reads it.
//! Instead, the engine gives both versions to a merge function from the app,
//! which knows the format ([`Merge`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::{Error, Result};

/// Single shared files, relative to the vault root.
pub const SHARED_FILES: &[&str] = &[".kknow/bookmarks.json"];

/// Folders of shared records: each `<id>.json` inside syncs.
pub const SHARED_DIRS: &[&str] = &[".kknow/proposals", ".kknow/threads"];

/// Joins two versions of a shared file: the path, the local bytes and the
/// remote bytes. `None` means the function cannot join them; then the local
/// version wins.
pub type Merge = fn(&str, &[u8], &[u8]) -> Option<Vec<u8>>;

/// True for a key that sync carries as a shared file.
///
/// A key comes from the storage, so this is also the check that a remote
/// object cannot write outside these folders: no `/` and no `..` in the name.
pub fn is_shared(key: &str) -> bool {
    if SHARED_FILES.contains(&key) {
        return true;
    }
    SHARED_DIRS.iter().any(|dir| {
        key.strip_prefix(dir)
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|name| name.strip_suffix(".json"))
            .is_some_and(|id| {
                !id.is_empty()
                    && id.len() <= 32
                    && id
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            })
    })
}

/// All shared files of the vault with their bytes.
pub fn read_all(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for key in SHARED_FILES {
        if let Some(bytes) = read(root, key)? {
            files.insert(key.to_string(), bytes);
        }
    }
    for dir in SHARED_DIRS {
        let path = root.join(dir);
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(Error::io(&path, source)),
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let key = format!("{dir}/{}", name.to_string_lossy());
            if is_shared(&key) {
                if let Some(bytes) = read(root, &key)? {
                    files.insert(key, bytes);
                }
            }
        }
    }
    Ok(files)
}

/// The bytes of one shared file, or `None` if it is not there.
pub fn read(root: &Path, key: &str) -> Result<Option<Vec<u8>>> {
    let path = root.join(key);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::io(&path, source)),
    }
}

/// Writes a shared file through a temporary one, as the app does.
pub fn write(root: &Path, key: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
    fs::rename(&temp, &path).map_err(|source| Error::io(&path, source))
}

pub fn remove(root: &Path, key: &str) -> Result<()> {
    let path = root.join(key);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::io(&path, source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_files_are_shared() {
        assert!(is_shared(".kknow/bookmarks.json"));
        assert!(is_shared(".kknow/threads/k3f.json"));
        assert!(is_shared(".kknow/proposals/p_1-a.json"));
        assert!(!is_shared(".kknow/sync.json"));
        assert!(!is_shared(".kknow/remote.json"));
        assert!(!is_shared(".kknow/threads/../remote.json"));
        assert!(!is_shared(".kknow/threads/a/b.json"));
        assert!(!is_shared(".kknow/threads/.json"));
        assert!(!is_shared(".kknow/trash/x.json"));
    }

    #[test]
    fn read_all_skips_temporary_and_foreign_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".kknow/threads/a.json", b"{}").unwrap();
        write(dir.path(), ".kknow/bookmarks.json", b"[]").unwrap();
        fs::write(dir.path().join(".kknow/threads/b.json.tmp"), "x").unwrap();
        fs::write(dir.path().join(".kknow/sync.json"), "{}").unwrap();

        let keys: Vec<String> = read_all(dir.path()).unwrap().into_keys().collect();

        assert_eq!(keys, [".kknow/bookmarks.json", ".kknow/threads/a.json"]);
    }
}
