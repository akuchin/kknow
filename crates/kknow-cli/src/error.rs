use std::path::PathBuf;

use kknow_app::git_client::GitClientError;
use kknow_core::NoteId;

pub type Result<T, E = CliError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    App(#[from] kknow_app::Error),

    #[error(transparent)]
    Core(#[from] kknow_core::Error),

    #[error(transparent)]
    Sync(#[from] kknow_sync::Error),

    #[error(transparent)]
    Run(#[from] kknow_run::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("note is not found: {0}")]
    NoteNotFound(String),

    #[error("not a folder and not a vault from the list: {0}")]
    NotAVault(String),

    #[error("no vaults with label {0}: kknow vaults label <vault> <label>")]
    NoVaultWithLabel(String),

    #[error("cannot find the settings folder: HOME is not set")]
    NoConfigFolder,

    #[error("no storage is set: use --bucket, --folder or the folder settings")]
    NoStorage,

    #[error("{0} has no line {1}: it has {2} lines")]
    NoSuchLine(NoteId, usize, usize),

    #[error("the file already exists: {}", .0.display())]
    FileExists(PathBuf),

    #[error("command is not found: {0}; `kknow commands` lists them")]
    CommandNotFound(String),

    #[error("the command was stopped by a signal")]
    StoppedBySignal,

    #[error("{reason}: set {variable} or run the command in a terminal")]
    NoTerminal {
        reason: &'static str,
        variable: String,
    },

    #[error("the two {0}s are different")]
    SecretsDiffer(String),

    #[error("no token is set: KKNOW_GIT_TOKEN")]
    NoGitToken,

    #[error("variable {0} is not set")]
    GitVariableNotSet(String),

    #[error("{vault}: {source}")]
    Vault {
        vault: String,
        source: kknow_app::Error,
    },
}

impl From<GitClientError> for CliError {
    fn from(error: GitClientError) -> Self {
        match error {
            GitClientError::NoToken => CliError::NoGitToken,
            GitClientError::VariableNotSet(name) => CliError::GitVariableNotSet(name),
        }
    }
}
