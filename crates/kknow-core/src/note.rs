use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// File extensions that the vault treats as notes.
pub const MARKDOWN_EXTENSIONS: [&str; 3] = ["md", "markdown", "mdown"];

pub fn is_markdown_extension(ext: &str) -> bool {
    MARKDOWN_EXTENSIONS
        .iter()
        .any(|known| ext.eq_ignore_ascii_case(known))
}

/// Note id: the path relative to the vault root, without the extension,
/// always with `/` as the separator.
///
/// `Projects/Rust/kknow.md` -> `Projects/Rust/kknow`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(String);

impl NoteId {
    /// Returns `None` for paths that cannot be inside the vault: absolute,
    /// with `..`, empty or not UTF-8.
    pub fn from_relative_path(path: &Path) -> Option<Self> {
        let mut parts: Vec<&str> = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_str()?),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }

        let (file, folders) = parts.split_last()?;
        let file_path = Path::new(file);
        let stem = match file_path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if is_markdown_extension(ext) => file_path.file_stem()?.to_str()?,
            _ => *file,
        };
        if stem.is_empty() {
            return None;
        }

        let mut id = String::with_capacity(path.as_os_str().len());
        for folder in folders {
            if folder.is_empty() {
                return None;
            }
            id.push_str(folder);
            id.push('/');
        }
        id.push_str(stem);
        Some(NoteId(id))
    }

    /// Parses user input: `folder/note`, `folder/note.md` and `folder\note`
    /// give the same id.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.replace('\\', "/");
        Self::from_relative_path(Path::new(normalized.trim_matches('/')))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Note name without folders. A short `[[link]]` refers to this name.
    pub fn name(&self) -> &str {
        match self.0.rsplit_once('/') {
            Some((_, name)) => name,
            None => &self.0,
        }
    }

    /// Folder relative to the vault root. `None` for notes in the root.
    pub fn folder(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(folder, _)| folder)
    }

    pub fn to_relative_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.md", self.0))
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Note metadata. The content is not kept in memory: [`crate::Vault::read`]
/// reads it, because the files on disk are the source of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: NoteId,
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_extension_and_normalizes_separators() {
        let id = NoteId::from_relative_path(Path::new("Projects/Rust/kknow.md")).unwrap();
        assert_eq!(id.as_str(), "Projects/Rust/kknow");
        assert_eq!(id.name(), "kknow");
        assert_eq!(id.folder(), Some("Projects/Rust"));
        assert_eq!(
            id.to_relative_path(),
            PathBuf::from("Projects/Rust/kknow.md")
        );
    }

    #[test]
    fn keeps_dots_inside_the_file_name() {
        let id = NoteId::from_relative_path(Path::new("note.v2.md")).unwrap();
        assert_eq!(id.as_str(), "note.v2");
    }

    #[test]
    fn rejects_paths_that_escape_the_vault() {
        assert!(NoteId::from_relative_path(Path::new("../secrets.md")).is_none());
        assert!(NoteId::from_relative_path(Path::new("/etc/passwd.md")).is_none());
        assert!(NoteId::from_relative_path(Path::new("")).is_none());
    }

    #[test]
    fn parse_accepts_user_input_in_any_shape() {
        let expected = NoteId::parse("folder/note").unwrap();
        assert_eq!(NoteId::parse("folder/note.md").unwrap(), expected);
        assert_eq!(NoteId::parse("folder\\note").unwrap(), expected);
        assert_eq!(NoteId::parse("/folder/note/").unwrap(), expected);
    }

    #[test]
    fn markdown_extension_check_is_case_insensitive() {
        assert!(is_markdown_extension("MD"));
        assert!(is_markdown_extension("markdown"));
        assert!(!is_markdown_extension("txt"));
    }
}
