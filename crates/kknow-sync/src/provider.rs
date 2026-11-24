use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Get,
    Put,
    Delete,
    List,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Operation::Get => "read",
            Operation::Put => "write",
            Operation::Delete => "delete",
            Operation::List => "list",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cause {
    #[default]
    Other,
    KeysRefused(String),
    NoBucket(String),
    Unreachable(String),
    FolderDenied(PathBuf),
    FolderMissing(PathBuf),
    NotDownloaded,
}

#[derive(Debug, thiserror::Error)]
#[error("{}", self.text())]
pub struct Error {
    pub operation: Operation,
    pub key: String,
    pub cause: Cause,
    #[source]
    pub source: Box<dyn std::error::Error + Send + Sync>,
}

impl Error {
    pub fn new(
        operation: Operation,
        key: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Error {
            operation,
            key: key.into(),
            cause: Cause::Other,
            source: source.into(),
        }
    }

    pub fn because(mut self, cause: Cause) -> Self {
        self.cause = cause;
        self
    }

    fn text(&self) -> String {
        match &self.cause {
            Cause::Other => format!(
                "{} \"{}\" failed: {}",
                self.operation, self.key, self.source
            ),
            Cause::KeysRefused(code) => format!(
                "the storage refused the access keys ({code}). Check the access key and the secret key"
            ),
            Cause::NoBucket(bucket) => {
                format!("the bucket {bucket} does not exist. Check its name and the region")
            }
            Cause::Unreachable(address) => {
                format!("{address} does not answer. Check the address and the network connection")
            }
            Cause::NotDownloaded => format!(
                "the cloud has not downloaded \"{}\" yet. Open the folder and wait for the download",
                self.key
            ),
            Cause::FolderDenied(path) => format!(
                "no access to the folder {}. Check its permissions; on macOS, also System Settings → Privacy & Security → Files and Folders",
                path.display()
            ),
            Cause::FolderMissing(path) => format!(
                "the folder {} does not exist. Connect its disk or pick another folder",
                path.display()
            ),
        }
    }
}

pub trait SyncProvider {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;

    fn delete(&self, key: &str) -> Result<()>;

    fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

impl<P: SyncProvider + ?Sized> SyncProvider for &P {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        (**self).get(key)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        (**self).put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<()> {
        (**self).delete(key)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        (**self).list(prefix)
    }
}

impl<P: SyncProvider + ?Sized> SyncProvider for Box<P> {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        (**self).get(key)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        (**self).put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<()> {
        (**self).delete(key)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        (**self).list(prefix)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryProvider {
    objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl MemoryProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn keys(&self) -> Vec<String> {
        self.objects().keys().cloned().collect()
    }

    fn objects(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>> {
        self.objects.lock().expect("the memory storage is poisoned")
    }
}

impl SyncProvider for MemoryProvider {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.objects().get(key).cloned())
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.objects().insert(key.to_string(), bytes.to_vec());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.objects().remove(key);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self
            .objects()
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_lists_by_prefix() {
        let provider = MemoryProvider::new();
        provider.put("notes/Index.md", b"# Index").unwrap();
        provider.put("manifest.json", b"{}").unwrap();

        assert_eq!(provider.get("notes/Index.md").unwrap().unwrap(), b"# Index");
        assert_eq!(provider.list("notes/").unwrap(), ["notes/Index.md"]);
    }

    #[test]
    fn missing_key_is_none_and_delete_is_idempotent() {
        let provider = MemoryProvider::new();

        assert!(provider.get("notes/Нет.md").unwrap().is_none());
        assert!(provider.delete("notes/Нет.md").is_ok());
    }

    #[test]
    fn a_clone_shares_the_storage() {
        let first = MemoryProvider::new();
        let second = first.clone();

        first.put("notes/Index.md", b"# Index").unwrap();

        assert!(second.get("notes/Index.md").unwrap().is_some());
    }
}
