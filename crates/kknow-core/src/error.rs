use std::path::PathBuf;

use crate::note::NoteId;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("vault root is not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("note not found in vault: {0}")]
    NoteNotFound(NoteId),

    #[error("path escapes the vault root: {0}")]
    OutsideVault(PathBuf),

    #[error("invalid folder path: {0}")]
    InvalidFolder(String),

    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("file is too large: {0}")]
    TooLarge(PathBuf),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}
