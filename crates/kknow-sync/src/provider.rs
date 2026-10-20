//! Remote storage as sync sees it.
//!
//! Sync needs only four operations on bytes by key. Thus the sync algorithm
//! does not depend on S3: the same code runs on in-memory storage in tests,
//! and can later run on WebDAV or another disk.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The operation that failed.
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
            Operation::Get => "чтение",
            Operation::Put => "запись",
            Operation::Delete => "удаление",
            Operation::List => "перечисление",
        })
    }
}

/// Storage error.
///
/// The cause is behind `dyn Error`: `SyncProvider` must not expose the error
/// types of a specific backend, or the abstraction leaks.
#[derive(Debug, thiserror::Error)]
#[error("{operation} «{key}» не удалось: {source}")]
pub struct Error {
    pub operation: Operation,
    pub key: String,
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
            source: source.into(),
        }
    }
}

/// Object storage: bytes by string key.
///
/// Implementations must be idempotent where possible: a delete of a missing
/// key is a success, not an error. Sync can stop at any moment and start
/// again, so a repeated action must not break it.
pub trait SyncProvider {
    /// The object contents, or `None` if the key does not exist.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;

    /// Writes the object and replaces the old one.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;

    /// Deletes the object. A missing key is not an error.
    fn delete(&self, key: &str) -> Result<()>;

    /// Keys with the given prefix, including nested "directories".
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

/// A boxed provider is also a provider.
///
/// The storage is chosen at run time: the settings give a folder or S3, but
/// the engine is generic over the provider type.
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

/// In-memory storage.
///
/// A clone shares objects with the original. Tests use this to simulate two
/// devices connected to one bucket.
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
        self.objects.lock().expect("хранилище в памяти испорчено")
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
