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
//! # std::fs::write(dir.path().join("Index.md"), "# Оглавление")?;
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
mod engine;
pub mod folder;
pub mod git;
pub mod manifest;
mod plan;
pub mod provider;

#[cfg(feature = "s3")]
pub mod s3;

use std::path::{Path, PathBuf};

use kknow_core::NoteId;

pub use config::{GitAuth, GitSettings, Provider, RemoteConfig, S3Settings};
pub use engine::{
    Conflict, Fetched, MANIFEST_KEY, NOTES_PREFIX, Progress, Report, Staged, SyncEngine, note_key,
};
pub use folder::FolderProvider;
pub use git::{ChangeKind, Git, GitChange, GitConflict, GitReport, GitStatus, Merged};
pub use manifest::Manifest;
pub use plan::{Action, CONFLICT_MARK, ConflictPolicy, Plan, is_conflict_copy};
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
) -> Result<SyncEngine<Box<dyn SyncProvider>>> {
    Ok(SyncEngine::new(connect(config, secret)?).with_policy(config.policy))
}

/// Connects to storage from the saved settings.
///
/// This is one entry point for both frontends: the window and the terminal
/// must read the settings the same way. The secret comes as a separate
/// argument because the settings file never contains it.
pub fn connect(config: &RemoteConfig, secret: Option<String>) -> Result<Box<dyn SyncProvider>> {
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

    #[error("манифест не разобрать: {0}")]
    Manifest(#[from] serde_json::Error),

    #[error("настройка синхронизации: не задан бакет")]
    NoBucket,

    #[error("настройка синхронизации: не задана папка")]
    NoFolder,

    #[error("эта сборка собрана без поддержки S3")]
    NoS3,

    #[error("настройка синхронизации: не задан адрес репозитория")]
    NoGitUrl,

    #[error("git синхронизирует хранилище своей командой, а не через манифест")]
    GitIsNotAProvider,

    #[error("git не найден: {}. Укажите путь к git в настройках", .0.display())]
    GitMissing(PathBuf),

    #[error("git: {0}")]
    Git(String),

    #[error(
        "папка хранилища не пустая и не репозиторий git. Подключите пустую папку или папку, где уже есть репозиторий"
    )]
    GitNotEmpty,

    #[error("хранилище — не корень репозитория git (корень: {}). Одно хранилище — один репозиторий", .0.display())]
    GitNotRoot(PathBuf),

    #[error("в репозитории нет remote origin: укажите адрес")]
    GitNoOrigin,

    #[error(
        "в адресе репозитория есть логин или токен. Уберите его из адреса и укажите токен отдельно"
    )]
    GitUrlCredentials,

    #[error("HEAD не на ветке: переключитесь на ветку")]
    GitDetached,

    #[error("пустое сообщение коммита")]
    GitEmptyMessage,

    #[error("сначала закоммитьте изменения: их перезапишет слияние ({})", .0.join(", "))]
    GitLocalChanges(Vec<String>),

    #[error("манифест обещает заметку {0}, но в хранилище её нет")]
    MissingRemote(NoteId),

    #[error("заметка {0} в хранилище не в UTF-8")]
    NotUtf8(NoteId),

    #[error("хранилище изменилось во время синхронизации — запустите её ещё раз")]
    RemoteChanged,

    #[error("ошибка ввода-вывода в {path}: {source}")]
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
