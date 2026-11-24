pub mod config;
pub mod encrypted;
mod engine;
pub mod folder;
pub mod git;
pub mod manifest;
mod plan;
pub mod provider;
pub mod shared;

#[cfg(feature = "s3")]
pub mod s3;

use std::path::{Path, PathBuf};

use kknow_core::NoteId;

pub use config::{GitAuth, GitSettings, Provider, RemoteConfig, S3Settings};
pub use encrypted::{Encrypted, Storage};
pub use engine::{
    Conflict, Fetched, MANIFEST_KEY, Progress, Report, Staged, SyncEngine, note_key, note_of_key,
};
pub use folder::FolderProvider;
pub use git::{ChangeKind, Git, GitChange, GitConflict, GitReport, GitStatus, Merged, Side};
pub use manifest::Manifest;
pub use plan::{Action, CONFLICT_MARK, ConflictPolicy, FileAction, Plan, is_conflict_copy};
pub use provider::{MemoryProvider, SyncProvider};

#[cfg(feature = "s3")]
pub use s3::{S3Config, S3Provider};

pub fn engine(
    config: &RemoteConfig,
    secret: Option<String>,
    key: Option<&kknow_crypto::DataKey>,
) -> Result<SyncEngine<Box<dyn SyncProvider>>> {
    Ok(SyncEngine::over(connect(config, secret, key)?).with_policy(config.policy))
}

pub fn connect(
    config: &RemoteConfig,
    secret: Option<String>,
    key: Option<&kknow_crypto::DataKey>,
) -> Result<Storage<Box<dyn SyncProvider>>> {
    let storage = storage(config, secret)?;
    if !config.encrypted {
        return Ok(Storage::Plain(storage));
    }
    let key = key.ok_or(encrypted::Error::Locked)?;
    Ok(Storage::Encrypted(Encrypted::new(storage, key)))
}

pub fn storage(config: &RemoteConfig, secret: Option<String>) -> Result<Box<dyn SyncProvider>> {
    if config.encrypted && config.is_git() {
        return Err(config::Error::GitEncrypted.into());
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
        Provider::S3(_) => Err(config::Error::NoS3.into()),

        Provider::Git(_) => Err(config::Error::GitIsNotAProvider.into()),
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

    #[error("the note {0} was not staged for upload")]
    NotStaged(NoteId),

    #[error("the manifest has note {0}, but the vault does not")]
    MissingRemote(NoteId),

    #[error("note {0} in the vault is not UTF-8")]
    NotUtf8(NoteId),

    #[error("the vault changed during sync: run it again")]
    RemoteChanged,

    #[error(transparent)]
    Git(#[from] git::Error),

    #[error(transparent)]
    Config(#[from] config::Error),

    #[error(transparent)]
    Encryption(#[from] encrypted::Error),

    #[error(transparent)]
    Crypto(#[from] kknow_crypto::Error),

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn text(error: impl Into<Error>) -> String {
        error.into().to_string()
    }

    #[test]
    fn git_errors_keep_their_texts() {
        let cases = [
            (
                git::Error::Missing(PathBuf::from("/usr/bin/git")),
                "git is not found: /usr/bin/git. Set the path to git in the settings",
            ),
            (git::Error::Command("boom".into()), "git: boom"),
            (
                git::Error::NotEmpty,
                "the vault folder is not empty and is not a git repository. Connect an empty folder or a folder with a repository",
            ),
            (
                git::Error::NotRoot(PathBuf::from("/vaults")),
                "the vault is not the root of a git repository (the root is /vaults). One vault is one repository",
            ),
            (
                git::Error::NoOrigin,
                "the repository has no remote origin: set the address",
            ),
            (
                git::Error::Unreachable("no host".into()),
                "the git server does not answer (no host). Check the address and the network connection",
            ),
            (
                git::Error::Denied("bad token".into()),
                "the git server refused the sign-in (bad token). Check the token or the SSH key",
            ),
            (
                git::Error::RepoMissing("nope".into()),
                "the repository is not found (nope). Check its address",
            ),
            (
                git::Error::UrlCredentials,
                "the repository address contains a login or a token. Remove it from the address and give the token separately",
            ),
            (
                git::Error::Detached,
                "HEAD is not on a branch: switch to a branch",
            ),
            (git::Error::EmptyMessage, "the commit message is empty"),
            (
                git::Error::LocalChanges(vec!["a.md".into(), "b.md".into()]),
                "commit your changes first: the merge would overwrite them (a.md, b.md)",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(text(error), expected);
        }
    }

    #[test]
    fn config_errors_keep_their_texts() {
        let cases = [
            (config::Error::NoBucket, "sync settings: no bucket is set"),
            (config::Error::NoFolder, "sync settings: no folder is set"),
            (config::Error::NoS3, "this build has no S3 support"),
            (
                config::Error::NoGitUrl,
                "sync settings: no repository address is set",
            ),
            (
                config::Error::GitIsNotAProvider,
                "git syncs the vault with its own command, not through the manifest",
            ),
            (
                config::Error::GitEncrypted,
                "git storage cannot be encrypted",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(text(error), expected);
        }
    }

    #[test]
    fn encryption_errors_keep_their_texts() {
        let cases = [
            (
                encrypted::Error::Locked,
                "the storage is encrypted: enter the passphrase or the recovery code",
            ),
            (
                encrypted::Error::NotEncrypted,
                "the storage is not encrypted",
            ),
            (
                encrypted::Error::AlreadyEncrypted,
                "the storage is already encrypted",
            ),
            (
                encrypted::Error::WrongKey,
                "the key of this device does not open the storage: encryption in the storage was turned on again. Type the passphrase again",
            ),
            (
                encrypted::Error::MixedStorage,
                "a device without encryption wrote into this encrypted storage. Update kknow on all devices and check the storage",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(text(error), expected);
        }
    }

    #[test]
    fn crypto_errors_keep_their_texts() {
        let cases = [
            (
                kknow_crypto::Error::Cipher("boom".into()),
                "encryption: boom",
            ),
            (
                kknow_crypto::Error::WrongPassphrase,
                "wrong passphrase or recovery code",
            ),
            (
                kknow_crypto::Error::PassphraseTooShort(8),
                "the passphrase must have at least 8 characters",
            ),
            (
                kknow_crypto::Error::Damaged("a.md".into()),
                "the object a.md does not open: it is damaged, or it was encrypted with a different key",
            ),
            (
                kknow_crypto::Error::OtherNotesKey,
                "the note was locked with another password: this vault has a different lock key",
            ),
            (
                kknow_crypto::Error::KeyTooLong("a/b".into()),
                "the path a/b is too long for an encrypted storage",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(text(error), expected);
        }
    }
}
