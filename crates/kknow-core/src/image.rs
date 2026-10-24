//! Images next to notes.
//!
//! A note refers to an image by a path from the note (`![](img/diagram.png)`),
//! by a path from the vault root, or by a bare file name, like a wikilink
//! (`![[diagram.png]]`). A bare name is searched in the whole vault. Only
//! images inside the vault can be read: a note can come from another person's
//! vault, and a path like `../../.ssh/id_rsa` in it must open nothing.
//!
//! A pasted or dropped image goes to the `attachments` folder ([`save`]).
//! The window gives only a name and the bytes. The core selects the folder.

use std::fs;
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::vault::is_hidden;

/// Image extensions: the formats the webview can show.
pub const IMAGE_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp", "ico",
];

/// Larger images are not read. The note shows them scaled down anyway, and
/// sending hundreds of megabytes to the window for a preview is a waste.
pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Folder for pasted and dropped images, at the vault root.
pub const ATTACHMENTS: &str = "attachments";

/// These characters break a `![[link]]` or the path cleanup in [`resolve`].
/// A name gets `-` in their place.
const LINK_BREAKERS: [char; 9] = ['[', ']', '|', '#', '^', '?', '%', '<', '>'];

/// Checks only the extension of the link target. It does not touch the disk.
pub fn is_image(target: &str) -> bool {
    has_image_extension(Path::new(&clean(target)))
}

/// Finds an image for a note in `from_dir` inside the vault `root`.
///
/// Order: path from the note, path from the root, then the file name in the
/// whole vault (the one nearest to the root; at equal depth, the first in
/// alphabetical order). Only a target without folders is searched by name:
/// a missing `img/a.png` must not silently become an unrelated `a.png` from
/// another project.
pub fn resolve(root: &Path, from_dir: &Path, target: &str) -> Option<PathBuf> {
    let target = clean(target);
    let relative = Path::new(&target);
    if !has_image_extension(relative) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let inside = |path: PathBuf| {
        let path = path.canonicalize().ok()?;
        (path.starts_with(&root) && path.is_file()).then_some(path)
    };

    // `/img/a.png` is a path from the vault root, not from the disk root.
    let from_root: PathBuf = relative
        .components()
        .filter(|part| !matches!(part, Component::RootDir | Component::Prefix(_)))
        .collect();
    let found = inside(from_dir.join(relative)).or_else(|| inside(root.join(&from_root)));
    if found.is_some() {
        return found;
    }

    let mut parts = relative.components();
    let (Some(Component::Normal(name)), None) = (parts.next(), parts.next()) else {
        return None;
    };
    WalkDir::new(&root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_hidden(entry))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == name)
        .min_by_key(|entry| entry.depth())
        .map(|entry| entry.into_path())
}

/// Finds an image for a loose file. It has no vault, so only a path from the
/// file itself or an absolute path works.
pub fn resolve_near(from_dir: &Path, target: &str) -> Option<PathBuf> {
    let target = clean(target);
    let relative = Path::new(&target);
    if !has_image_extension(relative) {
        return None;
    }
    let path = from_dir.join(relative).canonicalize().ok()?;
    path.is_file().then_some(path)
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let size = fs::metadata(path)
        .map_err(|err| Error::io(path, err))?
        .len();
    if size > MAX_IMAGE_BYTES {
        return Err(Error::TooLarge(path.to_path_buf()));
    }
    fs::read(path).map_err(|err| Error::io(path, err))
}

/// Saves an image into `dir/attachments` under the first free name.
///
/// `name` is a file name without folders: it comes from the window. The
/// function never overwrites a file. It returns the path from `dir` with `/`,
/// for example `attachments/Pasted image 2.png`.
pub fn save(dir: &Path, name: &str, bytes: &[u8]) -> Result<String> {
    let name = attachment_name(name)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(Error::TooLarge(PathBuf::from(name)));
    }
    let folder = dir.join(ATTACHMENTS);
    // A symlink in place of the folder could take the write out of `dir`.
    match fs::symlink_metadata(&folder) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(Error::InvalidFolder(ATTACHMENTS.to_string())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&folder).map_err(|err| Error::io(&folder, err))?;
        }
        Err(err) => return Err(Error::io(&folder, err)),
    }

    let free = free_name(&folder, &name);
    let target = folder.join(&free);
    // A hidden temp file: the vault scan and the name search skip it.
    let temp = folder.join(format!(".{free}.kknow-tmp"));
    fs::write(&temp, bytes).map_err(|err| Error::io(&temp, err))?;
    fs::rename(&temp, &target).map_err(|err| Error::io(&target, err))?;
    Ok(format!("{ATTACHMENTS}/{free}"))
}

/// Copies an image file from the disk into `dir/attachments`, like [`save`].
/// The copy keeps the file name.
pub fn import(dir: &Path, source: &Path) -> Result<String> {
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Check the name before the read: a large file that is not an image must
    // not go into memory.
    attachment_name(&name)?;
    save(dir, &name, &read(source)?)
}

/// Checks a file name from the window and makes it safe for a link.
fn attachment_name(raw: &str) -> Result<String> {
    let invalid = || Error::InvalidImageName(raw.to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('.')
        || trimmed.contains(['/', '\\', '\0'])
        || Path::new(trimmed).is_absolute()
    {
        return Err(invalid());
    }
    let name: String = trimmed
        .chars()
        .map(|ch| {
            if LINK_BREAKERS.contains(&ch) || ch.is_control() {
                '-'
            } else {
                ch
            }
        })
        .collect();
    if !has_image_extension(Path::new(&name)) {
        return Err(invalid());
    }
    Ok(name)
}

/// `name`, then `stem 2.ext`, `stem 3.ext`… The first name not on the disk.
fn free_name(folder: &Path, name: &str) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    (1..)
        .map(|attempt| match attempt {
            1 => name.to_string(),
            n => format!("{stem} {n}.{ext}"),
        })
        .find(|candidate| fs::symlink_metadata(folder.join(candidate)).is_err())
        .expect("бесконечный перебор имён закончился")
}

fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// Target as a path: without `?query` and `#anchor`, with `%20` decoded.
fn clean(target: &str) -> String {
    let end = target.find(['?', '#']).unwrap_or(target.len());
    decode(target[..end].trim())
}

/// Percent-decoding by bytes: `%D0%BA` is one letter, not two.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = raw
            .get(at + 1..at + 3)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for path in [
            "Projects/kknow.md",
            "Projects/img/схема.png",
            "Projects/logo.svg",
            "attachments/deep/shot.png",
            "attachments/shot.png",
            ".kknow/hidden.png",
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"x").unwrap();
        }
        dir
    }

    fn found(dir: &tempfile::TempDir, target: &str) -> Option<String> {
        let root = dir.path().canonicalize().unwrap();
        resolve(&root, &root.join("Projects"), target).map(|path| {
            let relative = path.strip_prefix(&root).unwrap();
            relative.to_string_lossy().replace('\\', "/")
        })
    }

    #[test]
    fn path_is_taken_from_the_note_first() {
        let dir = fixture();

        assert_eq!(
            found(&dir, "img/схема.png").as_deref(),
            Some("Projects/img/схема.png")
        );
        assert_eq!(
            found(&dir, "./logo.svg").as_deref(),
            Some("Projects/logo.svg")
        );
    }

    #[test]
    fn then_from_the_vault_root() {
        let dir = fixture();

        assert_eq!(
            found(&dir, "attachments/shot.png").as_deref(),
            Some("attachments/shot.png")
        );
        assert_eq!(
            found(&dir, "/attachments/shot.png").as_deref(),
            Some("attachments/shot.png")
        );
    }

    #[test]
    fn bare_name_is_searched_across_the_vault() {
        let dir = fixture();

        // Of two `shot.png` files, the one nearest to the root wins.
        assert_eq!(
            found(&dir, "shot.png").as_deref(),
            Some("attachments/shot.png")
        );
    }

    #[test]
    fn missing_folder_is_not_replaced_by_a_namesake() {
        let dir = fixture();

        assert_eq!(found(&dir, "elsewhere/shot.png"), None);
    }

    #[test]
    fn encoded_names_are_decoded() {
        let dir = fixture();

        assert_eq!(
            found(&dir, "img/%D1%81%D1%85%D0%B5%D0%BC%D0%B0.png?raw=1#top").as_deref(),
            Some("Projects/img/схема.png")
        );
    }

    #[test]
    fn nothing_outside_the_vault_or_hidden_is_read() {
        let dir = fixture();
        let outside = dir.path().parent().unwrap().join("kknow-outside-test.png");
        fs::write(&outside, b"x").unwrap();

        assert_eq!(found(&dir, "../../kknow-outside-test.png"), None);
        assert_eq!(found(&dir, outside.to_str().unwrap()), None);
        assert_eq!(found(&dir, "hidden.png"), None);
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn only_images_are_resolved() {
        let dir = fixture();

        assert_eq!(found(&dir, "kknow.md"), None);
        assert!(is_image("a/B.PNG"));
        assert!(!is_image("notes.md"));
    }

    #[test]
    fn loose_file_looks_next_to_itself() {
        let dir = fixture();
        let near = dir.path().join("Projects");

        assert!(resolve_near(&near, "img/схема.png").is_some());
        assert!(resolve_near(&near, "../attachments/shot.png").is_some());
        assert!(resolve_near(&near, "shot.png").is_none());
    }

    #[test]
    fn save_writes_into_attachments_and_returns_the_path() {
        let dir = fixture();

        let path = save(dir.path(), "Pasted image 20261024153012.png", b"png").unwrap();

        assert_eq!(path, "attachments/Pasted image 20261024153012.png");
        assert_eq!(fs::read(dir.path().join(&path)).unwrap(), b"png");
        // The vault resolves the same link from a note in a folder.
        assert_eq!(found(&dir, &path).as_deref(), Some(path.as_str()));
    }

    #[test]
    fn save_never_overwrites_a_file() {
        let dir = fixture();

        assert_eq!(
            save(dir.path(), "shot.png", b"new").unwrap(),
            "attachments/shot 2.png"
        );
        assert_eq!(
            save(dir.path(), "shot.png", b"newer").unwrap(),
            "attachments/shot 3.png"
        );
        assert_eq!(fs::read(dir.path().join("attachments/shot.png")).unwrap(), b"x");
    }

    #[test]
    fn save_creates_the_folder_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();

        save(dir.path(), "схема.webp", b"x").unwrap();

        let names: Vec<_> = fs::read_dir(dir.path().join(ATTACHMENTS))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["схема.webp"]);
    }

    #[test]
    fn save_refuses_paths_and_hidden_names() {
        let dir = fixture();

        for name in [
            "../escape.png",
            "a/b.png",
            "a\\b.png",
            "/etc/x.png",
            "..",
            ".hidden.png",
            "",
            "   ",
        ] {
            assert!(
                matches!(save(dir.path(), name, b"x"), Err(Error::InvalidImageName(_))),
                "{name}"
            );
        }
        assert!(!dir.path().parent().unwrap().join("escape.png").exists());
    }

    #[test]
    fn save_accepts_only_image_extensions() {
        let dir = fixture();

        for name in ["заметка.md", "script.sh", "без расширения"] {
            assert!(
                matches!(save(dir.path(), name, b"x"), Err(Error::InvalidImageName(_))),
                "{name}"
            );
        }
        assert!(save(dir.path(), "Фото.JPG", b"x").is_ok());
    }

    #[test]
    fn save_replaces_characters_that_break_a_link() {
        let dir = fixture();

        assert_eq!(
            save(dir.path(), "план [v2] #1 50%.png", b"x").unwrap(),
            "attachments/план -v2- -1 50-.png"
        );
    }

    #[cfg(unix)]
    #[test]
    fn save_does_not_follow_a_symlinked_folder() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(ATTACHMENTS)).unwrap();

        assert!(save(dir.path(), "a.png", b"x").is_err());
        assert!(!outside.path().join("a.png").exists());
    }

    #[test]
    fn import_copies_a_file_with_its_name() {
        let dir = fixture();
        let source = dir.path().join("Projects/logo.svg");

        assert_eq!(import(dir.path(), &source).unwrap(), "attachments/logo.svg");
        assert!(import(dir.path(), &dir.path().join("Projects/kknow.md")).is_err());
    }
}
