//! A folder as note storage.
//!
//! Cloud drive services also use this: they sync a plain directory, so they
//! need no special code. In the UI such a "provider" is only a preset path
//! and name. The actual work happens here.
//!
//! The folder is not a vault: it holds `manifest.json` and `notes/*.md`, the
//! same keys as a bucket. Thus sync stays the same for any disk below.

use std::fs;
use std::path::{Path, PathBuf};

use crate::provider::{Error, Operation, Result, SyncProvider};

/// Suffix that macOS adds to a cloud file that is not downloaded yet.
const ICLOUD_SUFFIX: &str = ".icloud";

pub struct FolderProvider {
    root: PathBuf,
}

impl FolderProvider {
    /// Opens the folder and creates it if necessary.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        fs::create_dir_all(root)
            .map_err(|source| Error::new(Operation::Put, root.to_string_lossy(), source))?;

        // Canonicalize because later checks use this path to make sure a key
        // does not write outside the folder.
        let root = root
            .canonicalize()
            .map_err(|source| Error::new(Operation::Get, root.to_string_lossy(), source))?;

        Ok(FolderProvider { root })
    }

    pub fn location(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// Path for a key.
    ///
    /// Keys come from the manifest, and the manifest comes from remote storage.
    /// So `..` and absolute paths are rejected: otherwise sync could write
    /// files outside the folder. `NoteId` in the core has the same check.
    fn path_for(&self, key: &str, operation: Operation) -> Result<PathBuf> {
        let safe = !key.is_empty()
            && !key.starts_with('/')
            && key
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");

        if !safe {
            return Err(Error::new(operation, key, "недопустимый ключ"));
        }
        Ok(self.root.join(key))
    }

    /// Key for a path inside the folder. `None` means the file is not ours.
    fn key_for(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let mut key = String::new();
        for part in relative.components() {
            let part = part.as_os_str().to_str()?;
            if !key.is_empty() {
                key.push('/');
            }
            key.push_str(part);
        }
        Some(key)
    }
}

impl SyncProvider for FolderProvider {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key, Operation::Get)?;

        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A cloud file that is not downloaded is a placeholder with
                // another name. A silent "no key" is worse than an error here:
                // sync would treat the note as deleted and remove it locally.
                if placeholder(&path).exists() {
                    return Err(Error::new(
                        Operation::Get,
                        key,
                        "файл ещё не скачан облаком — откройте папку и дождитесь загрузки",
                    ));
                }
                Ok(None)
            }
            Err(source) => Err(Error::new(Operation::Get, key, source)),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(key, Operation::Put)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::new(Operation::Put, key, source))?;
        }

        // Write through a temporary file and `rename`, as the core does for
        // notes. Otherwise the cloud could pick up half a file.
        let temp = path.with_extension("kknow-tmp");
        fs::write(&temp, bytes).map_err(|source| Error::new(Operation::Put, key, source))?;
        fs::rename(&temp, &path).map_err(|source| Error::new(Operation::Put, key, source))
    }

    fn delete(&self, key: &str) -> Result<()> {
        let path = self.path_for(key, Operation::Delete)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            // A delete of a missing file is a success: sync can stop and
            // start again, and the second run must not fail.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::new(Operation::Delete, key, source)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let start = self.root.join(prefix.trim_end_matches('/'));
        if !start.exists() {
            return Ok(keys);
        }

        let mut stack = vec![start];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(source) => return Err(Error::new(Operation::List, prefix, source)),
            };

            for entry in entries {
                let entry = entry.map_err(|source| Error::new(Operation::List, prefix, source))?;
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                // An unfinished write of our own sync is not a storage object.
                if path.extension().is_some_and(|ext| ext == "kknow-tmp") {
                    continue;
                }

                // List a cloud placeholder under the real file name: the file
                // is in the storage, only not on this machine yet. A read
                // fails, and `get` reports it, so the list does not hide it.
                let path = restore(&path);
                if let Some(key) = self.key_for(&path) {
                    if key.starts_with(prefix) {
                        keys.push(key);
                    }
                }
            }
        }

        keys.sort();
        Ok(keys)
    }
}

/// Name of the placeholder that macOS uses for a file not downloaded yet:
/// `.name.icloud`.
fn placeholder(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    path.with_file_name(format!(".{name}{ICLOUD_SUFFIX}"))
}

/// The reverse: `.name.icloud` is the file `name`.
fn restore(path: &Path) -> PathBuf {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    match name.strip_prefix('.').and_then(|rest| {
        rest.strip_suffix(ICLOUD_SUFFIX)
            .filter(|real| !real.is_empty())
    }) {
        Some(real) => path.with_file_name(real),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> (tempfile::TempDir, FolderProvider) {
        let dir = tempfile::tempdir().unwrap();
        let provider = FolderProvider::open(dir.path().join("хранилище")).unwrap();
        (dir, provider)
    }

    #[test]
    fn round_trip_through_the_folder() {
        let (_dir, provider) = provider();

        provider
            .put("notes/Проект.md", "# Проект".as_bytes())
            .unwrap();

        assert_eq!(
            provider.get("notes/Проект.md").unwrap().as_deref(),
            Some("# Проект".as_bytes())
        );
        assert_eq!(provider.get("notes/Нет.md").unwrap(), None);
    }

    #[test]
    fn list_walks_subfolders_and_skips_leftovers() {
        let (_dir, provider) = provider();
        provider.put("manifest.json", b"{}").unwrap();
        provider.put("notes/A.md", b"a").unwrap();
        provider.put("notes/Папка/B.md", b"b").unwrap();
        fs::write(provider.root.join("notes/C.kknow-tmp"), "мусор").unwrap();

        assert_eq!(
            provider.list("notes/").unwrap(),
            ["notes/A.md", "notes/Папка/B.md"]
        );
        assert_eq!(provider.list("").unwrap().len(), 3);
    }

    #[test]
    fn deleting_a_missing_key_is_fine() {
        let (_dir, provider) = provider();
        provider.put("notes/A.md", b"a").unwrap();

        provider.delete("notes/A.md").unwrap();
        provider.delete("notes/A.md").unwrap();

        assert_eq!(provider.get("notes/A.md").unwrap(), None);
    }

    #[test]
    fn a_key_cannot_escape_the_folder() {
        let (dir, provider) = provider();

        assert!(provider.put("../снаружи.md", "нет".as_bytes()).is_err());
        assert!(provider.get("/etc/passwd").is_err());
        assert!(provider.put("", "нет".as_bytes()).is_err());
        assert!(!dir.path().join("снаружи.md").exists());
    }

    #[test]
    fn an_undownloaded_icloud_file_is_an_error_not_an_absence() {
        let (_dir, provider) = provider();
        fs::create_dir_all(provider.root.join("notes")).unwrap();
        // This is how macOS marks a file that is not on this machine yet.
        fs::write(provider.root.join("notes/.Проект.md.icloud"), b"").unwrap();

        // The list has the file: it is in the storage, only not here.
        assert_eq!(provider.list("notes/").unwrap(), ["notes/Проект.md"]);

        let error = provider.get("notes/Проект.md").unwrap_err();
        assert!(error.to_string().contains("не скачан"), "{error}");
    }

    #[test]
    fn writing_leaves_no_temporary_files_behind() {
        let (_dir, provider) = provider();

        provider.put("notes/A.md", b"a").unwrap();

        let leftovers: Vec<_> = fs::read_dir(provider.root.join("notes"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
