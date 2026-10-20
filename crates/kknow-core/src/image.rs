//! Images next to notes.
//!
//! A note refers to an image by a path from the note (`![](img/diagram.png)`),
//! by a path from the vault root, or by a bare file name, like a wikilink
//! (`![[diagram.png]]`). A bare name is searched in the whole vault. Only
//! images inside the vault can be read: a note can come from another person's
//! vault, and a path like `../../.ssh/id_rsa` in it must open nothing.

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
}
