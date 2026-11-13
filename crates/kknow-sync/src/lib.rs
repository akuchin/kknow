//! Sync of a vault with remote storage.
//!
//! The rule is the same as in all of kknow: the files on disk are the source
//! of truth. The cloud keeps a copy of the notes and a manifest, that is, a
//! list of what this copy must contain. Everything else (what is new here,
//! what was deleted there, what diverged) comes from three manifests: the
//! local one, the remote one, and the one both sides agreed on last time.
//!
//! ```
//! use kknow_core::Vault;
//! use kknow_sync::{MemoryProvider, SyncEngine};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let dir = tempfile::tempdir()?;
//! # std::fs::write(dir.path().join("Index.md"), "# Contents")?;
//! let mut vault = Vault::open(dir.path())?;
//! let engine = SyncEngine::new(MemoryProvider::new());
//!
//! let report = engine.run(&mut vault)?;
//!
//! assert_eq!(report.uploaded.len(), 1);
//! // The second run does nothing.
//! assert!(engine.run(&mut vault)?.is_empty());
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod crypto;
pub mod encrypted;
mod engine;
pub mod folder;
pub mod git;
pub mod locked;
pub mod manifest;
mod plan;
pub mod provider;
pub mod shared;

#[cfg(feature = "s3")]
pub mod s3;

use std::path::{Path, PathBuf};

use kknow_core::NoteId;

pub use config::{GitAuth, GitSettings, Provider, RemoteConfig, S3Settings};
pub use crypto::{DataKey, RecoveryCode};
pub use encrypted::Encrypted;
pub use engine::{
    Conflict, Fetched, MANIFEST_KEY, Progress, Report, Staged, SyncEngine, note_key, note_of_key,
};
pub use folder::FolderProvider;
pub use git::{ChangeKind, Git, GitChange, GitConflict, GitReport, GitStatus, Merged};
pub use manifest::Manifest;
pub use plan::{Action, CONFLICT_MARK, ConflictPolicy, FileAction, Plan, is_conflict_copy};
pub use provider::{MemoryProvider, SyncProvider};

#[cfg(feature = "s3")]
pub use s3::{S3Config, S3Provider};

/// A sync engine for the saved settings: the provider and the conflict policy.
///
/// Both frontends build the engine here. Before, each one called `connect`
/// and then `with_policy`, and a frontend that forgot the second call would
/// silently sync with the default policy.
pub fn engine(
    config: &RemoteConfig,
    secret: Option<String>,
    key: Option<&DataKey>,
) -> Result<SyncEngine<Box<dyn SyncProvider>>> {
    Ok(SyncEngine::new(connect(config, secret, key)?).with_policy(config.policy))
}

/// Connects to storage from the saved settings.
///
/// This is one entry point for both frontends: the window and the terminal
/// must read the settings the same way. The secret comes as a separate
/// argument because the settings file never contains it. So does the data
/// key of an encrypted storage: without it, an encrypted storage is
/// [`Error::Locked`].
pub fn connect(
    config: &RemoteConfig,
    secret: Option<String>,
    key: Option<&DataKey>,
) -> Result<Box<dyn SyncProvider>> {
    let storage = storage(config, secret)?;
    if !config.encrypted {
        return Ok(storage);
    }
    let key = key.ok_or(Error::Locked)?;
    Ok(Box::new(Encrypted::new(storage, key)))
}

/// The storage under the encryption, if there is encryption. The encryption
/// operations ([`encrypted::init`], [`encrypted::unlock`]) work on it.
pub fn storage(config: &RemoteConfig, secret: Option<String>) -> Result<Box<dyn SyncProvider>> {
    if config.encrypted && config.is_git() {
        return Err(Error::GitEncrypted);
    }
    match &config.provider {
        Provider::Folder { path } => Ok(Box::new(FolderProvider::open(path)?)),

        #[cfg(feature = "s3")]
        Provider::S3(settings) => {
            let s3 = S3Config {
                bucket: settings.bucket.clone(),
                endpoint: settings.endpoint.clone(),
                region: settings.region.clone(),
                root: settings.prefix.clone(),
                access_key_id: settings.access_key_id.clone(),
                secret_access_key: secret,
            };
            Ok(Box::new(S3Provider::connect(&s3)?))
        }

        #[cfg(not(feature = "s3"))]
        Provider::S3(_) => Err(Error::NoS3),

        Provider::Git(_) => Err(Error::GitIsNotAProvider),
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Vault(#[from] kknow_core::Error),

    #[error(transparent)]
    Provider(#[from] provider::Error),

    #[error("cannot parse the manifest: {0}")]
    Manifest(#[from] serde_json::Error),

    #[error("sync settings: no bucket is set")]
    NoBucket,

    #[error("sync settings: no folder is set")]
    NoFolder,

    #[error("this build has no S3 support")]
    NoS3,

    #[error("sync settings: no repository address is set")]
    NoGitUrl,

    #[error("git syncs the vault with its own command, not through the manifest")]
    GitIsNotAProvider,

    #[error("git is not found: {}. Set the path to git in the settings", .0.display())]
    GitMissing(PathBuf),

    #[error("git: {0}")]
    Git(String),

    #[error(
        "the vault folder is not empty and is not a git repository. Connect an empty folder or a folder with a repository"
    )]
    GitNotEmpty,

    #[error("the vault is not the root of a git repository (the root is {}). One vault is one repository", .0.display())]
    GitNotRoot(PathBuf),

    #[error("the repository has no remote origin: set the address")]
    GitNoOrigin,

    #[error("the git server does not answer ({0}). Check the address and the network connection")]
    GitUnreachable(String),

    #[error("the git server refused the sign-in ({0}). Check the token or the SSH key")]
    GitDenied(String),

    #[error("the repository is not found ({0}). Check its address")]
    GitRepoMissing(String),

    #[error(
        "the repository address contains a login or a token. Remove it from the address and give the token separately"
    )]
    GitUrlCredentials,

    #[error("HEAD is not on a branch: switch to a branch")]
    GitDetached,

    #[error("the commit message is empty")]
    GitEmptyMessage,

    #[error("commit your changes first: the merge would overwrite them ({})", .0.join(", "))]
    GitLocalChanges(Vec<String>),

    #[error("the manifest has note {0}, but the vault does not")]
    MissingRemote(NoteId),

    #[error("note {0} in the vault is not UTF-8")]
    NotUtf8(NoteId),

    #[error("the vault changed during sync: run it again")]
    RemoteChanged,

    #[error("encryption: {0}")]
    Crypto(String),

    #[error("wrong passphrase or recovery code")]
    WrongPassphrase,

    #[error("the passphrase must have at least {0} characters")]
    PassphraseTooShort(usize),

    #[error(
        "the object {0} does not open: it is damaged, or it was encrypted with a different key"
    )]
    Damaged(String),

    #[error("the note was locked with another password: this vault has a different lock key")]
    OtherNotesKey,

    #[error("the path {0} is too long for an encrypted storage")]
    KeyTooLong(String),

    #[error("the storage is encrypted: enter the passphrase or the recovery code")]
    Locked,

    #[error("the storage is not encrypted")]
    NotEncrypted,

    #[error("the storage is already encrypted")]
    AlreadyEncrypted,

    #[error(
        "the key of this device does not open the storage: encryption in the storage was turned on again. Type the passphrase again"
    )]
    WrongKey,

    #[error(
        "a device without encryption wrote into this encrypted storage. Update kknow on all devices and check the storage"
    )]
    MixedStorage,

    #[error("git storage cannot be encrypted")]
    GitEncrypted,

    #[error("{}", kknow_core::io_text(path, source))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}
