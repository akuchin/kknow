use kknow_core::fsx;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const KEYMAP_FILE: &str = "keybindings.json";

const MODIFIERS: [&str; 6] = ["Mod", "Cmd", "Meta", "Ctrl", "Alt", "Shift"];

const SYMBOLS: &str = "\\,./=-;'`[]";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Keymap(pub BTreeMap<String, Vec<String>>);

#[derive(Debug, Default)]
pub struct Loaded {
    pub keymap: Keymap,
    pub problems: Vec<Problem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "KeyProblem"))]
pub enum Problem {
    File {
        path: String,
        detail: String,
    },
    NotAList {
        action: String,
    },
    NotAString {
        action: String,
    },
    NoKey {
        action: String,
        key: String,
    },
    UnknownModifier {
        action: String,
        key: String,
        modifier: String,
    },
    RepeatedModifier {
        action: String,
        key: String,
        modifier: String,
    },
    UnknownKey {
        action: String,
        key: String,
        name: String,
    },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::File { path, detail } => write!(f, "{path}: {detail}"),
            Problem::NotAList { action } => {
                write!(
                    f,
                    "{action}: needs a list of keys, for example [\"Mod-Shift-n\"]"
                )
            }
            Problem::NotAString { action } => write!(f, "{action}: a key must be a string"),
            Problem::NoKey { action, key } => write!(f, "{action}: \"{key}\" has no key"),
            Problem::UnknownModifier {
                action,
                key,
                modifier,
            } => write!(f, "{action}: \"{key}\": unknown modifier \"{modifier}\""),
            Problem::RepeatedModifier {
                action,
                key,
                modifier,
            } => write!(
                f,
                "{action}: \"{key}\": the modifier \"{modifier}\" is twice"
            ),
            Problem::UnknownKey { action, key, name } => {
                write!(f, "{action}: \"{key}\": unknown key \"{name}\"")
            }
        }
    }
}

impl Keymap {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(KEYMAP_FILE)
    }

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
                    problems: vec![Problem::File {
                        path: path.display().to_string(),
                        detail: error.to_string(),
                    }],
                });
            }
        };

        let mut loaded = Loaded::default();
        for (action, value) in raw {
            match keys_of(&action, &value) {
                Ok(keys) => {
                    loaded.keymap.0.insert(action, keys);
                }
                Err(problem) => loaded.problems.push(problem),
            }
        }
        Ok(loaded)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }
        if let Ok(bytes) = fs::read(path) {
            if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                let _ = fsx::set_aside(path);
            }
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("keys serialize");
        bytes.push(b'\n');

        fsx::write_atomic(path, &bytes).map_err(|source| Error::io(path, source))
    }
}

fn keys_of(action: &str, value: &serde_json::Value) -> std::result::Result<Vec<String>, Problem> {
    let action = || action.to_string();
    let list = value
        .as_array()
        .ok_or_else(|| Problem::NotAList { action: action() })?;
    list.iter()
        .map(|item| {
            let key = item
                .as_str()
                .ok_or_else(|| Problem::NotAString { action: action() })?;
            check_key(&action(), key)?;
            Ok(key.to_string())
        })
        .collect()
}

pub fn check_key(action: &str, key: &str) -> std::result::Result<(), Problem> {
    let (modifiers, name) = split(key);
    let action = action.to_string();
    let key_text = key.to_string();
    if name.is_empty() {
        return Err(Problem::NoKey {
            action,
            key: key_text,
        });
    }

    let mut seen = Vec::new();
    for modifier in modifiers {
        if !MODIFIERS.contains(&modifier) {
            return Err(Problem::UnknownModifier {
                action,
                key: key_text,
                modifier: modifier.to_string(),
            });
        }
        if seen.contains(&modifier) {
            return Err(Problem::RepeatedModifier {
                action,
                key: key_text,
                modifier: modifier.to_string(),
            });
        }
        seen.push(modifier);
    }

    let mut chars = name.chars();
    let single = chars.next().filter(|_| chars.as_str().is_empty());
    let valid = match single {
        Some(char) => char.is_ascii_alphanumeric() || SYMBOLS.contains(char),
        None => {
            name.starts_with(|char: char| char.is_ascii_uppercase())
                && name.chars().all(|char| char.is_ascii_alphanumeric())
        }
    };
    if valid {
        Ok(())
    } else {
        Err(Problem::UnknownKey {
            action,
            key: key_text,
            name: name.to_string(),
        })
    }
}

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
        assert!(
            matches!(
                &loaded.problems[0],
                Problem::UnknownModifier { action, modifier, .. }
                    if action == "note.find" && modifier == "Super"
            ),
            "{:?}",
            loaded.problems
        );
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
            assert_eq!(check_key("note.new", key), Ok(()), "{key}");
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
            assert!(check_key("note.new", key).is_err(), "{key}");
        }
    }
}
