use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::{Error, Result};

pub const SHARED_FILES: &[&str] = &[".kknow/bookmarks.json", kknow_crypto::locked::LOCK_FILE];

pub const SHARED_DIRS: &[&str] = &[".kknow/proposals", ".kknow/threads"];

pub type Merge = fn(&str, &[u8], &[u8]) -> Option<Vec<u8>>;

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

pub fn read(root: &Path, key: &str) -> Result<Option<Vec<u8>>> {
    let path = root.join(key);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::io(&path, source)),
    }
}

pub fn write(root: &Path, key: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    kknow_core::fsx::write_atomic(&path, bytes).map_err(|source| Error::io(&path, source))
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
