use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct RecordDir {
    dir: PathBuf,
}

impl RecordDir {
    pub fn new(vault_root: &Path, relative: &str) -> Self {
        RecordDir {
            dir: vault_root.join(relative),
        }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn all<T: DeserializeOwned>(&self) -> Result<Vec<T>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::io(&self.dir, source)),
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        Ok(paths
            .iter()
            .filter_map(|path| fs::read(path).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect())
    }

    pub fn get<T: DeserializeOwned>(&self, id: &str) -> Result<Option<T>> {
        if !valid_id(id) {
            return Ok(None);
        }
        let path = self.file(id);
        match fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::io(&path, source)),
        }
    }

    pub fn put<T: Serialize>(&self, id: &str, record: &T) -> Result<()> {
        fs::create_dir_all(&self.dir).map_err(|source| Error::io(&self.dir, source))?;
        let path = self.file(id);
        let mut bytes = serde_json::to_vec_pretty(record).expect("the record serializes");
        bytes.push(b'\n');
        kknow_core::fsx::write_atomic(&path, &bytes).map_err(|source| Error::io(&path, source))
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Ok(());
        }
        let path = self.file(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::io(&path, source)),
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

pub fn fresh_id() -> String {
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    let mut value = hasher.finish();
    let mut id = String::new();
    for _ in 0..6 {
        let digit = (value % 36) as u32;
        id.push(char::from_digit(digit, 36).expect("a base-36 digit"));
        value /= 36;
    }
    id
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_that_leave_the_folder_are_refused() {
        assert!(valid_id("p7f3a_x-1"));
        assert!(!valid_id("../secrets"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(""));
    }

    #[test]
    fn fresh_ids_are_valid_and_differ() {
        let first = fresh_id();
        let second = fresh_id();

        assert!(valid_id(&first));
        assert_eq!(first.len(), 6);
        assert_ne!(first, second);
    }

    #[test]
    fn a_broken_record_does_not_hide_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let records = RecordDir::new(dir.path(), ".kknow/test");
        records.put("a", &serde_json::json!({ "x": 1 })).unwrap();
        fs::write(dir.path().join(".kknow/test/b.json"), "{не json").unwrap();

        let all: Vec<serde_json::Value> = records.all().unwrap();

        assert_eq!(all.len(), 1);
    }
}
