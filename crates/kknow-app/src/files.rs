//! Single files: markdown opened without a vault.
//!
//! A double click on `README.md` in a repository must not make the repository a
//! vault: a vault has a watcher, an index and sync, but the user needs one file.
//! So such files have their own list next to the vault registry (`files.json`),
//! without directory scan, graph or search.
//!
//! The list is the window's memory of "what was open", not storage. A file that
//! is no longer on disk is dropped from it. A missing vault can have an outside
//! cause (a disk is unplugged), so we keep it. A single file is usually deleted
//! or moved by hand, and a row that leads nowhere only gets in the way.
//!
//! The list path comes as an argument for the same reason as the registry path:
//! the core does not know about Tauri or where the platform keeps settings.

use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::{NoteId, is_markdown_extension};
use kknow_index::Resolver;
use kknow_sync::CONFLICT_MARK;
use serde::{Deserialize, Serialize};

use crate::{Error, Result, hash};

pub const FILES_FILE: &str = "files.json";

/// Current list format version.
pub const VERSION: u32 = 1;

/// Single files opened in the window, in the order they were opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFiles {
    #[serde(default = "current_version")]
    version: u32,
    #[serde(default)]
    files: Vec<PathBuf>,
}

fn current_version() -> u32 {
    VERSION
}

impl Default for OpenFiles {
    fn default() -> Self {
        OpenFiles {
            version: VERSION,
            files: Vec::new(),
        }
    }
}

/// Result of reading the list.
#[derive(Debug)]
pub struct Loaded {
    pub files: OpenFiles,
    /// The list file did not parse: it is moved aside and the list starts empty.
    pub problem: Option<String>,
    /// Entries dropped on load because the files are no longer on disk.
    pub forgotten: Vec<PathBuf>,
}

impl OpenFiles {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(FILES_FILE)
    }

    /// Reads the list and drops missing files. No list file gives an empty list.
    /// A broken one also does, plus a problem message and a copy next to it.
    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    files: OpenFiles::default(),
                    problem: None,
                    forgotten: Vec::new(),
                });
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        match serde_json::from_slice::<OpenFiles>(&bytes) {
            Ok(mut files) => {
                files.dedup();
                let forgotten = files.prune();
                Ok(Loaded {
                    files,
                    problem: None,
                    forgotten,
                })
            }
            Err(error) => {
                let broken = path.with_extension("json.broken");
                let _ = fs::rename(path, &broken);
                Ok(Loaded {
                    files: OpenFiles::default(),
                    problem: Some(format!(
                        "список открытых файлов не разобрать ({error}); он отложен в {}",
                        broken.display()
                    )),
                    forgotten: Vec::new(),
                })
            }
        }
    }

    /// Writes through a temporary file, like the vault registry.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("список сериализуем");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.files
    }

    /// Compares the canonical path: the window can send a path through a
    /// symlink.
    pub fn contains(&self, path: &Path) -> bool {
        let canonical = path.canonicalize();
        let path = canonical.as_deref().unwrap_or(path);
        self.files.iter().any(|known| known == path)
    }

    /// Adds the file and returns its canonical path.
    ///
    /// A known file gets no second row and keeps its place. Otherwise rows would
    /// jump under the mouse on each open.
    pub fn add(&mut self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = markdown_file(path.as_ref())?;
        if !self.files.contains(&path) {
            self.files.push(path.clone());
        }
        Ok(path)
    }

    /// Removes the entry; the file stays on disk. `false` means it was not in
    /// the list.
    pub fn remove(&mut self, path: &Path) -> bool {
        let canonical = path.canonicalize();
        let path = canonical.as_deref().unwrap_or(path);
        let before = self.files.len();
        // A missing file cannot be canonicalized, so also compare it as is.
        self.files.retain(|known| known != path);
        before != self.files.len()
    }

    /// Removes entries whose files are gone from disk and returns them.
    pub fn prune(&mut self) -> Vec<PathBuf> {
        let (kept, gone): (Vec<PathBuf>, Vec<PathBuf>) =
            self.files.drain(..).partition(|path| path.is_file());
        self.files = kept;
        gone
    }

    /// People edit the file by hand: a repeated entry must not give two rows.
    fn dedup(&mut self) {
        let mut seen: Vec<PathBuf> = Vec::new();
        self.files.retain(|path| {
            let fresh = !seen.contains(path);
            if fresh {
                seen.push(path.clone());
            }
            fresh
        });
    }
}

/// Checks that the path is a markdown file and canonicalizes it.
///
/// Uses the same extension list as the vault uses for notes: the window shows
/// markdown, so there is no reason to open a `.png` in it.
pub fn markdown_file(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(Error::NotAMarkdownFile(path.to_path_buf()));
    }
    let markdown = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_markdown_extension);
    if !markdown {
        return Err(Error::NotAMarkdownFile(path.to_path_buf()));
    }
    path.canonicalize()
        .map_err(|source| Error::io(path, source))
}

pub fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::io(path, source))
}

/// Result of saving a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSaved {
    Written,
    /// The file changed outside: it stays as is, and the window text goes to
    /// `copy` next to it, the same as for a vault note.
    Conflict {
        copy: PathBuf,
        disk: String,
    },
}

/// Writes the file only if the disk still has `base`, the text the edit started
/// from. The reason is the same as for [`crate::Workspace::save_if_unchanged`],
/// and it is even stronger here: a single file is usually a `README.md` in a
/// repository that other tools also edit.
pub fn save_if_unchanged(path: &Path, content: &str, base: &str) -> Result<FileSaved> {
    let disk = match fs::read_to_string(path) {
        Ok(disk) => Some(disk),
        // Deleted outside: the write brings the file back and keeps the edits.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => return Err(Error::io(path, source)),
    };

    match disk {
        Some(disk) if disk != base && disk != content => {
            let copy = conflict_copy(path, content);
            write(&copy, content)?;
            Ok(FileSaved::Conflict { copy, disk })
        }
        _ => {
            write(path, content)?;
            Ok(FileSaved::Written)
        }
    }
}

/// Writes through a temporary file next to it and `rename`, like the vault: an
/// interrupted write does not leave half the text.
///
/// Permissions are copied from the old file: `rename` puts a new file in place.
/// Otherwise an executable script or a private file would silently become a
/// plain one.
pub fn write(path: &Path, content: &str) -> Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".kknow-tmp");
    let temp = PathBuf::from(temp);

    fs::write(&temp, content).map_err(|source| Error::io(&temp, source))?;
    if let Ok(metadata) = fs::metadata(path) {
        let _ = fs::set_permissions(&temp, metadata.permissions());
    }
    fs::rename(&temp, path).map_err(|source| Error::io(path, source))
}

/// Name of the copy for text that did not go into the file: the file stem,
/// [`CONFLICT_MARK`], a short hash and the old extension.
fn conflict_copy(path: &Path, content: &str) -> PathBuf {
    let short = format!("{:016x}", hash(content));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
        .unwrap_or_else(|| "md".to_string());
    path.with_file_name(format!("{stem}{CONFLICT_MARK}{})", &short[..8]))
        .with_extension(extension)
}

/// The first heading of the text, or the file name if there is none.
pub fn title(path: &Path, content: &str) -> String {
    kknow_index::parse(content)
        .title()
        .map(str::to_string)
        .unwrap_or_else(|| stem(path))
}

/// HTML for the preview.
///
/// The file has no vault, so wikilinks cannot resolve and show as broken. This
/// is more honest than a search in nearby files: they are not parsed, and a match
/// would be random.
pub fn render(path: &Path, content: &str) -> String {
    let id = NoteId::parse(&stem(path))
        .unwrap_or_else(|| NoteId::parse("note").expect("простое имя — годный идентификатор"));
    kknow_index::to_html(content, &id, &Resolver::new([]))
}

/// Image that a single file refers to: a path relative to it or an absolute one.
pub fn image(path: &Path, target: &str) -> Option<PathBuf> {
    kknow_core::image::resolve_near(path.parent()?, target)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markdown(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_file_is_remembered_once_and_in_place() {
        let home = tempfile::tempdir().unwrap();
        let first = markdown(home.path(), "README.md", "# Проект\n");
        let second = markdown(home.path(), "docs/план.md", "план\n");
        let mut files = OpenFiles::default();

        files.add(&first).unwrap();
        files.add(&second).unwrap();
        // The same file by another path: canonicalization removes the `.`.
        files.add(home.path().join(".").join("README.md")).unwrap();

        assert_eq!(files.entries().len(), 2);
        assert_eq!(files.entries()[0], first.canonicalize().unwrap());
    }

    #[test]
    fn only_markdown_files_are_accepted() {
        let home = tempfile::tempdir().unwrap();
        let picture = markdown(home.path(), "схема.png", "не текст");
        let mut files = OpenFiles::default();

        assert!(matches!(
            files.add(&picture),
            Err(Error::NotAMarkdownFile(_))
        ));
        assert!(matches!(
            files.add(home.path()),
            Err(Error::NotAMarkdownFile(_))
        ));
        assert!(files.add(home.path().join("нет.md")).is_err());
        assert!(files.entries().is_empty());
    }

    #[test]
    fn missing_files_are_forgotten_on_load() {
        let home = tempfile::tempdir().unwrap();
        let kept = markdown(home.path(), "остался.md", "текст\n");
        let gone = markdown(home.path(), "удалён.md", "текст\n");
        let list = home.path().join("files.json");

        let mut files = OpenFiles::default();
        files.add(&kept).unwrap();
        files.add(&gone).unwrap();
        files.save(&list).unwrap();
        fs::remove_file(&gone).unwrap();

        let loaded = OpenFiles::load(&list).unwrap();

        assert_eq!(loaded.files.entries(), [kept.canonicalize().unwrap()]);
        assert_eq!(loaded.forgotten.len(), 1);
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn a_broken_list_is_set_aside() {
        let home = tempfile::tempdir().unwrap();
        let list = home.path().join("files.json");
        fs::write(&list, "{ не json").unwrap();

        let loaded = OpenFiles::load(&list).unwrap();

        assert!(loaded.files.entries().is_empty());
        assert!(loaded.problem.is_some());
        assert!(home.path().join("files.json.broken").exists());
    }

    #[test]
    fn forgetting_keeps_the_file_on_disk() {
        let home = tempfile::tempdir().unwrap();
        let path = markdown(home.path(), "README.md", "# Проект\n");
        let mut files = OpenFiles::default();
        files.add(&path).unwrap();

        assert!(files.remove(&path));
        assert!(!files.remove(&path), "второй раз забывать нечего");
        assert!(files.entries().is_empty());
        assert!(path.exists());
    }

    #[test]
    fn a_vanished_file_can_still_be_forgotten() {
        let home = tempfile::tempdir().unwrap();
        let path = markdown(home.path(), "README.md", "# Проект\n");
        let mut files = OpenFiles::default();
        let canonical = files.add(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(files.remove(&canonical));
    }

    #[test]
    fn saving_over_an_outside_edit_leaves_a_copy() {
        let home = tempfile::tempdir().unwrap();
        let path = markdown(home.path(), "README.md", "было\n");

        // Someone else edited the file while it was open.
        fs::write(&path, "чужая правка\n").unwrap();
        let saved = save_if_unchanged(&path, "моя правка\n", "было\n").unwrap();

        let FileSaved::Conflict { copy, disk } = saved else {
            panic!("ожидался конфликт");
        };
        assert_eq!(disk, "чужая правка\n");
        assert_eq!(read(&path).unwrap(), "чужая правка\n");
        assert_eq!(read(&copy).unwrap(), "моя правка\n");
        let name = copy.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("README (конфликт "), "{name}");
        assert!(name.ends_with(").md"), "{name}");
    }

    #[test]
    fn saving_an_unchanged_file_writes_it() {
        let home = tempfile::tempdir().unwrap();
        let path = markdown(home.path(), "README.md", "было\n");

        let saved = save_if_unchanged(&path, "стало\n", "было\n").unwrap();

        assert_eq!(saved, FileSaved::Written);
        assert_eq!(read(&path).unwrap(), "стало\n");
        assert!(
            !home.path().join("README.md.kknow-tmp").exists(),
            "временный файл не остался"
        );
    }

    #[cfg(unix)]
    #[test]
    fn writing_keeps_the_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let path = markdown(home.path(), "секрет.md", "было\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        write(&path, "стало\n").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn title_falls_back_to_the_file_name() {
        let path = Path::new("/tmp/README.md");

        assert_eq!(title(path, "# Проект\n\nтекст"), "Проект");
        assert_eq!(title(path, "просто текст"), "README");
    }

    #[test]
    fn wiki_links_render_without_a_vault() {
        let html = render(Path::new("/tmp/README.md"), "См. [[Другая]] и **жирный**.");

        assert!(html.contains("<strong>жирный</strong>"));
        assert!(html.contains("Другая"));
    }
}
