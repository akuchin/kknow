//! User keys: the actions whose keys the user changed.
//!
//! The default keys live in the UI (`ui/src/actions.ts`): the core does not
//! handle key presses. This file keeps only the difference from them, so an
//! action that a new version adds or moves gets its key without a migration:
//!
//! ```json
//! {
//!   "note.new": ["Mod-Shift-n"],
//!   "format.strike": []
//! }
//! ```
//!
//! The key is an action ID, the value is the full new list of its keys, in
//! the CodeMirror format (`Mod-Shift-b`). An empty list removes all keys of
//! the action.
//!
//! The file is global, next to the app settings: keys are a habit of the
//! person, not a property of a vault. A vault from someone else also must not
//! be able to move ⌘S.
//!
//! People edit the file by hand, so one bad entry does not reject the file:
//! the entry is skipped and reported. The core does not know the action list,
//! so it keeps unknown IDs: a newer version may know them, and the UI skips
//! them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const KEYMAP_FILE: &str = "keybindings.json";

/// Modifiers of the CodeMirror format. `Mod` is ⌘ on macOS and Ctrl elsewhere.
const MODIFIERS: [&str; 6] = ["Mod", "Cmd", "Meta", "Ctrl", "Alt", "Shift"];

/// Characters that are written as themselves, the same set as `CODES` in
/// `ui/src/actions.ts`.
const SYMBOLS: &str = "\\,./=-;'`[]";

/// Action ID → its keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Keymap(pub BTreeMap<String, Vec<String>>);

/// Result of reading the file.
#[derive(Debug, Default)]
pub struct Loaded {
    pub keymap: Keymap,
    /// Entries that were skipped, or the whole file if it does not parse.
    pub problems: Vec<String>,
}

impl Keymap {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(KEYMAP_FILE)
    }

    /// Reads the file. No file means no changes.
    ///
    /// A file that does not parse is not moved aside, unlike the settings: the
    /// window reads it again on each focus, and the user may be in the middle
    /// of an edit. It gives no changes and a problem.
    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded::default());
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        let raw = match serde_json::from_slice::<BTreeMap<String, serde_json::Value>>(&bytes) {
            Ok(raw) => raw,
            Err(error) => {
                return Ok(Loaded {
                    keymap: Keymap::default(),
                    problems: vec![format!("{} не разобрать: {error}", path.display())],
                });
            }
        };

        let mut loaded = Loaded::default();
        for (action, value) in raw {
            match keys_of(&value) {
                Ok(keys) => {
                    loaded.keymap.0.insert(action, keys);
                }
                Err(problem) => loaded.problems.push(format!("{action}: {problem}")),
            }
        }
        Ok(loaded)
    }

    /// Writes through a temporary file. A file that does not parse is kept as
    /// `.json.broken` first: it is a hand edit, and the save must not lose it.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        if let Ok(bytes) = fs::read(path) {
            if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                let _ = fs::rename(path, path.with_extension("json.broken"));
            }
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("клавиши сериализуемы");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }
}

/// The keys of one entry: a list of valid keys. One bad key skips the entry,
/// not only the key: half of a list is not what the person wrote.
fn keys_of(value: &serde_json::Value) -> std::result::Result<Vec<String>, String> {
    let list = value
        .as_array()
        .ok_or_else(|| "нужен список клавиш, например [\"Mod-Shift-n\"]".to_string())?;
    list.iter()
        .map(|item| {
            let key = item.as_str().ok_or("клавиша должна быть строкой")?;
            check_key(key)?;
            Ok(key.to_string())
        })
        .collect()
}

/// Checks a key in the CodeMirror format: modifiers, then one key name.
///
/// The UI checks the same rules when it parses the key; the check here gives
/// the person a message instead of a key that silently does nothing.
pub fn check_key(key: &str) -> std::result::Result<(), String> {
    let (modifiers, name) = split(key);
    if name.is_empty() {
        return Err(format!("«{key}»: нет клавиши"));
    }

    let mut seen = Vec::new();
    for modifier in modifiers {
        if !MODIFIERS.contains(&modifier) {
            return Err(format!("«{key}»: неизвестный модификатор «{modifier}»"));
        }
        if seen.contains(&modifier) {
            return Err(format!("«{key}»: модификатор «{modifier}» дважды"));
        }
        seen.push(modifier);
    }

    let mut chars = name.chars();
    let single = chars.next().filter(|_| chars.as_str().is_empty());
    let valid = match single {
        Some(char) => char.is_ascii_alphanumeric() || SYMBOLS.contains(char),
        // A key code: `ArrowLeft`, `F5`, `NumpadAdd`, `Enter`.
        None => {
            name.starts_with(|char: char| char.is_ascii_uppercase())
                && name.chars().all(|char| char.is_ascii_alphanumeric())
        }
    };
    if valid {
        Ok(())
    } else {
        Err(format!("«{key}»: неизвестная клавиша «{name}»"))
    }
}

/// `Mod-Shift-b` → (`[Mod, Shift]`, `b`). A dash at the end is the key: `Mod--`.
fn split(key: &str) -> (Vec<&str>, &str) {
    let (head, name) = match key.strip_suffix('-') {
        Some(head) if head.is_empty() || head.ends_with('-') => {
            (head.strip_suffix('-').unwrap_or(head), "-")
        }
        _ => match key.rsplit_once('-') {
            Some((head, name)) => (head, name),
            None => ("", key),
        },
    };
    let modifiers = if head.is_empty() {
        Vec::new()
    } else {
        head.split('-').collect()
    };
    (modifiers, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn keymap(entries: &[(&str, &[&str])]) -> Keymap {
        Keymap(
            entries
                .iter()
                .map(|(action, keys)| {
                    (
                        action.to_string(),
                        keys.iter().map(|key| key.to_string()).collect(),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn a_missing_file_changes_nothing() {
        let home = dir();

        let loaded = Keymap::load(&Keymap::path(home.path())).unwrap();

        assert_eq!(loaded.keymap, Keymap::default());
        assert!(loaded.problems.is_empty());
    }

    #[test]
    fn round_trip_through_the_file() {
        let home = dir();
        let path = Keymap::path(home.path());
        let keys = keymap(&[
            ("note.new", &["Mod-Shift-n"]),
            ("format.strike", &[]),
            ("scale.smaller", &["Mod--", "Mod-NumpadSubtract"]),
        ]);

        keys.save(&path).unwrap();
        let loaded = Keymap::load(&path).unwrap();

        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.keymap, keys);
        assert!(!home.path().join("keybindings.json.tmp").exists());
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""format.strike": []"#), "{written}");
    }

    #[test]
    fn a_bad_entry_is_skipped_and_the_rest_is_kept() {
        let home = dir();
        let path = Keymap::path(home.path());
        fs::write(
            &path,
            r#"{
                "note.new": ["Mod-Shift-n"],
                "note.save": "Mod-s",
                "note.find": ["Mod-Super-f"],
                "note.mode": ["Mod-e", 5],
                "future.action": ["Mod-j"]
            }"#,
        )
        .unwrap();

        let loaded = Keymap::load(&path).unwrap();

        assert_eq!(
            loaded.keymap,
            keymap(&[
                ("future.action", &["Mod-j"]),
                ("note.new", &["Mod-Shift-n"])
            ]),
            "неизвестное действие остаётся: его знает новая версия"
        );
        assert_eq!(loaded.problems.len(), 3, "{:?}", loaded.problems);
        assert!(loaded.problems[0].starts_with("note.find:"));
        assert!(loaded.problems[0].contains("Super"));
    }

    #[test]
    fn a_broken_file_is_reported_and_left_in_place() {
        let home = dir();
        let path = Keymap::path(home.path());
        fs::write(&path, "{ \"note.new\": [").unwrap();

        let loaded = Keymap::load(&path).unwrap();

        assert_eq!(loaded.keymap, Keymap::default());
        assert_eq!(loaded.problems.len(), 1);
        assert!(path.is_file(), "человек, может быть, ещё правит файл");
    }

    #[test]
    fn a_save_over_a_broken_file_keeps_a_copy() {
        let home = dir();
        let path = Keymap::path(home.path());
        fs::write(&path, "{ правка на середине").unwrap();

        keymap(&[("note.new", &["Mod-Shift-n"])])
            .save(&path)
            .unwrap();

        let broken = home.path().join("keybindings.json.broken");
        assert_eq!(fs::read_to_string(broken).unwrap(), "{ правка на середине");
        assert!(Keymap::load(&path).unwrap().problems.is_empty());
    }

    #[test]
    fn keys_in_the_codemirror_format_are_valid() {
        for key in [
            "Mod-k",
            "Mod-Shift-p",
            "Mod-\\",
            "Mod--",
            "Mod-Shift-=",
            "Mod-Alt-ArrowLeft",
            "Alt-Shift-ArrowUp",
            "Mod-NumpadAdd",
            "Ctrl-Cmd-f",
            "F5",
            "Enter",
            "-",
        ] {
            assert_eq!(check_key(key), Ok(()), "{key}");
        }
    }

    #[test]
    fn wrong_keys_are_rejected() {
        for key in [
            "",
            "Mod-",
            "Super-k",
            "Mod-Mod-k",
            "Mod-ы",
            "Mod-arrowLeft",
            "Mod-Arrow Left",
        ] {
            assert!(check_key(key).is_err(), "{key}");
        }
    }
}
