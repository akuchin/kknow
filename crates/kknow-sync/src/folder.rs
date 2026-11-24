use std::fs;
use std::path::{Path, PathBuf};

use crate::provider::{Cause, Error, Operation, Result, SyncProvider};

const ICLOUD_SUFFIX: &str = ".icloud";

fn folder_error(root: &Path, operation: Operation, key: &str, source: std::io::Error) -> Error {
    use std::io::ErrorKind;
    let cause = match source.kind() {
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => {
            Cause::FolderDenied(root.to_path_buf())
        }
        _ => Cause::Other,
    };
    Error::new(operation, key, source).because(cause)
}

pub struct FolderProvider {
    root: PathBuf,
}

impl FolderProvider {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if !root.exists()
            && root
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty() && !parent.exists())
        {
            return Err(
                Error::new(Operation::Get, root.to_string_lossy(), "no folder")
                    .because(Cause::FolderMissing(root.to_path_buf())),
            );
        }
        fs::create_dir_all(root).map_err(|source| {
            folder_error(root, Operation::Put, &root.to_string_lossy(), source)
        })?;

        let root = root.canonicalize().map_err(|source| {
            folder_error(root, Operation::Get, &root.to_string_lossy(), source)
        })?;

        Ok(FolderProvider { root })
    }

    fn failure(&self, operation: Operation, key: &str, source: std::io::Error) -> Error {
        folder_error(&self.root, operation, key, source)
    }

    pub fn location(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn path_for(&self, key: &str, operation: Operation) -> Result<PathBuf> {
        let safe = !key.is_empty()
            && !key.starts_with('/')
            && key
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..");

        if !safe {
            return Err(Error::new(operation, key, "invalid key"));
        }
        Ok(self.root.join(key))
    }

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
                if placeholder(&path).exists() {
                    return Err(Error::new(Operation::Get, key, "not downloaded")
                        .because(Cause::NotDownloaded));
                }
                Ok(None)
            }
            Err(source) => Err(self.failure(Operation::Get, key, source)),
        }
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path_for(key, Operation::Put)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| self.failure(Operation::Put, key, source))?;
        }

        let temp = path.with_extension("kknow-tmp");
        kknow_core::fsx::write_atomic_via(&path, &temp, bytes)
            .map_err(|source| self.failure(Operation::Put, key, source))
    }

    fn delete(&self, key: &str) -> Result<()> {
        let path = self.path_for(key, Operation::Delete)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(self.failure(Operation::Delete, key, source)),
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
                Err(source) => return Err(self.failure(Operation::List, prefix, source)),
            };

            for entry in entries {
                let entry =
                    entry.map_err(|source| self.failure(Operation::List, prefix, source))?;
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_some_and(|ext| ext == "kknow-tmp") {
                    continue;
                }

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

fn placeholder(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    path.with_file_name(format!(".{name}{ICLOUD_SUFFIX}"))
}

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
        fs::write(provider.root.join("notes/.Проект.md.icloud"), b"").unwrap();

        assert_eq!(provider.list("notes/").unwrap(), ["notes/Проект.md"]);

        let error = provider.get("notes/Проект.md").unwrap_err();
        assert_eq!(error.cause, Cause::NotDownloaded, "{error}");
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

    #[test]
    fn a_folder_on_a_missing_disk_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("NoDisk/kknow");

        let error = FolderProvider::open(&target).err().unwrap();
        assert_eq!(error.cause, Cause::FolderMissing(target.clone()));
        assert!(error.to_string().contains("does not exist"), "{error}");
        assert!(!dir.path().join("NoDisk").exists());
    }

    #[test]
    fn the_last_level_of_the_folder_is_still_created() {
        let dir = tempfile::tempdir().unwrap();
        assert!(FolderProvider::open(dir.path().join("kknow")).is_ok());
        assert!(dir.path().join("kknow").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_without_write_access_is_named() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, provider) = provider();
        let root = provider.root.clone();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();

        let error = provider.put("a.md", b"# A").unwrap_err();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(error.cause, Cause::FolderDenied(root));
        assert!(
            error.to_string().starts_with("no access to the folder"),
            "{error}"
        );
        drop(dir);
    }
}
