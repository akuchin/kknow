use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Vaults,
    Settings,
    OpenFiles,
    Bookmarks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadProblem {
    pub kind: FileKind,
    pub moved_to: PathBuf,
    pub reason: String,
}

impl LoadProblem {
    pub fn new(kind: FileKind, moved_to: PathBuf, reason: impl ToString) -> Self {
        LoadProblem {
            kind,
            moved_to,
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for LoadProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (what, moved) = match self.kind {
            FileKind::Vaults => ("the vault registry", "it is moved to"),
            FileKind::Settings => ("the settings", "they are moved to"),
            FileKind::OpenFiles => ("the list of open files", "it is moved to"),
            FileKind::Bookmarks => ("the bookmarks", "the file is moved to"),
        };
        write!(
            formatter,
            "cannot parse {what} ({}); {moved} {}",
            self.reason,
            self.moved_to.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(kind: FileKind) -> String {
        LoadProblem::new(kind, PathBuf::from("/x/a.broken"), "bad json").to_string()
    }

    #[test]
    fn each_kind_keeps_its_text() {
        assert_eq!(
            text(FileKind::Vaults),
            "cannot parse the vault registry (bad json); it is moved to /x/a.broken"
        );
        assert_eq!(
            text(FileKind::Settings),
            "cannot parse the settings (bad json); they are moved to /x/a.broken"
        );
        assert_eq!(
            text(FileKind::OpenFiles),
            "cannot parse the list of open files (bad json); it is moved to /x/a.broken"
        );
        assert_eq!(
            text(FileKind::Bookmarks),
            "cannot parse the bookmarks (bad json); the file is moved to /x/a.broken"
        );
    }
}
