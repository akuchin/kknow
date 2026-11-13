use std::path::PathBuf;

use crate::note::NoteId;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the vault root is not a folder: {0}")]
    NotADirectory(PathBuf),

    #[error("the vault has no such note: {0}")]
    NoteNotFound(NoteId),

    #[error("the path goes outside the vault root: {0}")]
    OutsideVault(PathBuf),

    #[error("invalid folder path: {0}")]
    InvalidFolder(String),

    #[error("already exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("trash entry is not found: {0}")]
    TrashEntryNotFound(String),

    #[error("the file is too large: {0}")]
    TooLarge(PathBuf),

    #[error("invalid image name: {0}")]
    InvalidImageName(String),

    #[error("{}", io_text(path, source))]
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

/// The text of an input or output error, by its kind.
///
/// A new user meets these first: a vault in a folder that macOS does not let
/// the app read, a cloud folder of a disk that is not connected. "os error
/// 13" says nothing to them, so the common kinds get their own words. The
/// window has its own texts for the same kinds (`src-tauri/src/errors.rs`).
pub fn io_text(path: &std::path::Path, source: &std::io::Error) -> String {
    use std::io::ErrorKind;
    let path = path.display();
    match source.kind() {
        ErrorKind::PermissionDenied => format!(
            "no access to {path}. Check the permissions of this folder; on macOS, also System Settings → Privacy & Security → Files and Folders"
        ),
        ErrorKind::NotFound => format!("{path} does not exist"),
        ErrorKind::ReadOnlyFilesystem => format!("{path} is on a read-only disk"),
        ErrorKind::StorageFull => format!("the disk is full: {path}"),
        _ => format!("I/O error in {path}: {source}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Error as IoError, ErrorKind};
    use std::path::Path;

    use super::*;

    #[test]
    fn common_io_errors_have_their_own_words() {
        let text = |kind| io_text(Path::new("/notes"), &IoError::from(kind));
        assert!(text(ErrorKind::PermissionDenied).starts_with("no access to /notes."));
        assert_eq!(text(ErrorKind::NotFound), "/notes does not exist");
        assert_eq!(
            text(ErrorKind::ReadOnlyFilesystem),
            "/notes is on a read-only disk"
        );
        assert!(text(ErrorKind::Interrupted).contains("/notes"));
    }
}
