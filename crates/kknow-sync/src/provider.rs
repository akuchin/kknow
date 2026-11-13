//! Remote storage as sync sees it.
//!
//! Sync needs only four operations on bytes by key. Thus the sync algorithm
//! does not depend on S3: the same code runs on in-memory storage in tests,
//! and can later run on WebDAV or another disk.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
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
            Operation::Get => "read",
            Operation::Put => "write",
            Operation::Delete => "delete",
            Operation::List => "list",
        })
    }
}

/// What went wrong, for the common problems that a person can fix.
///
/// A new user meets these first: a wrong key, a typo in the bucket name, no
/// network, a cloud folder of a disk that is not connected. The text of the
/// library says nothing to them ("PermissionDenied (permanent) at list,
/// context: …"), so a backend names the cause, and the CLI and the window
/// give their own words for it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cause {
    /// Anything else: the text of the backend.
    #[default]
    Other,
    /// The storage refused the access keys. The code of the storage, for
    /// example `InvalidAccessKeyId` or `SignatureDoesNotMatch`.
    KeysRefused(String),
    /// The bucket does not exist.
    NoBucket(String),
    /// Nothing answers at the address: no network, a wrong host or port.
    Unreachable(String),
    /// The app cannot read or write the folder.
    FolderDenied(PathBuf),
    /// The folder and its parent do not exist: a disk that is not
    /// connected, or a cloud drive that is not installed.
    FolderMissing(PathBuf),
    /// A cloud drive keeps the file in the cloud only: it is not on the disk
    /// yet.
    NotDownloaded,
}

/// Storage error.
///
/// The source is behind `dyn Error`: `SyncProvider` must not expose the error
/// types of a specific backend, or the abstraction leaks. The [`Cause`] says
/// what it means for a person.
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

    /// Work before a sync. Gives the keys that it changed: the remote
    /// manifest can have an old hash for them.
    ///
    /// A plain storage only checks that it is not encrypted: a device without
    /// the key must not write plaintext notes into it. [`crate::Encrypted`]
    /// also encrypts the plaintext objects that other programs put there.
    fn prepare(&self) -> crate::Result<Vec<String>> {
        if self.get(crate::crypto::KEY_FILE)?.is_some() {
            return Err(crate::Error::Locked);
        }
        Ok(Vec::new())
    }
}

/// A reference to a provider is also a provider: the encryption operations
/// wrap a storage that they do not own.
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

    fn prepare(&self) -> crate::Result<Vec<String>> {
        (**self).prepare()
    }
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

    fn prepare(&self) -> crate::Result<Vec<String>> {
        (**self).prepare()
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
