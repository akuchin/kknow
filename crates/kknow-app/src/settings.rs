//! App settings: language, theme and editor behaviour.
//!
//! These settings were in the window `localStorage` before. That worked with one
//! window. Now there are two windows, the main one and settings, and each has
//! its own `localStorage`: a language change in settings would not reach the
//! main window. So the source of truth is a file next to the vault registry,
//! and the windows only show it.
//!
//! Settings are global for the app, not per notes directory: the UI language is
//! not a vault property. The path comes as an argument: the core must not know
//! about Tauri or where the platform keeps settings (see
//! [`crate::vaults::config_dir`]).

use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{Error, Result};

pub const SETTINGS_FILE: &str = "settings.json";

const DEFAULT_LOCALE: &str = "ru";
const DEFAULT_THEME: &str = "light";
const DEFAULT_AUTOSAVE_MS: u32 = 600;

/// Autosave delay limits.
///
/// The lower limit stops each key press from going to disk. The upper limit
/// keeps "saves by itself" true, not a promise for half a minute.
const AUTOSAVE_MS: RangeInclusive<u32> = 100..=5000;

/// How a note is shown: where the markup is drawn and if it can be edited.
///
/// The window remembers the mode per note. This value is for notes that do not
/// have a mode yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// The markup is drawn in the text; no preview.
    Edit,
    /// Plain markdown with the preview next to it.
    #[default]
    Split,
    /// Preview only: read, not edit.
    Read,
}

/// An unknown mode in the file must not mark all settings as broken. It becomes
/// the default mode, the same way an empty language becomes Russian.
fn lenient_mode<'de, D: Deserializer<'de>>(raw: D) -> std::result::Result<ViewMode, D::Error> {
    Ok(match String::deserialize(raw)?.as_str() {
        "edit" => ViewMode::Edit,
        "read" => ViewMode::Read,
        _ => ViewMode::Split,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// `#[serde(default)]` is on the struct, not on the fields: a missing field then
// takes its value from the manual `Default`, not the zero of its type.
// Otherwise a file without `openLastNote` would mean "do not open", but the
// default is the opposite.
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// UI language.
    pub locale: String,
    /// Theme ID: built-in `light`/`dark` or a theme from the vault.
    pub theme: String,
    /// Mode for a note that does not have its own mode yet.
    #[serde(deserialize_with = "lenient_mode")]
    pub view_mode: ViewMode,
    /// Autosave delay in milliseconds.
    pub autosave_ms: u32,
    /// Open the last note at startup.
    pub open_last_note: bool,
    /// Line numbers in the editor gutter.
    pub line_numbers: bool,
}

/// Defaults are written by hand: `#[derive(Default)]` gives an empty language,
/// a zero delay and no last note. With such settings the app needs repair at
/// startup.
impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            locale: DEFAULT_LOCALE.to_string(),
            theme: DEFAULT_THEME.to_string(),
            view_mode: ViewMode::Split,
            autosave_ms: DEFAULT_AUTOSAVE_MS,
            open_last_note: true,
            line_numbers: true,
        }
    }
}

/// Result of reading settings.
#[derive(Debug)]
pub struct Loaded {
    pub settings: AppSettings,
    /// The file did not parse: it is moved aside and settings start from
    /// defaults. A silent reset looks like lost settings, so the problem goes
    /// up to the status bar.
    pub problem: Option<String>,
}

impl AppSettings {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(SETTINGS_FILE)
    }

    /// Reads settings. No file gives defaults. A broken file also gives
    /// defaults, plus a problem message and a backup copy next to it.
    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            // First start is normal: the defaults are the settings.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    settings: AppSettings::default(),
                    problem: None,
                });
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        match serde_json::from_slice::<AppSettings>(&bytes) {
            Ok(mut settings) => {
                settings.adopt_legacy(&bytes);
                settings.tidy();
                Ok(Loaded {
                    settings,
                    problem: None,
                })
            }
            Err(error) => {
                let broken = path.with_extension("json.broken");
                let _ = fs::rename(path, &broken);
                Ok(Loaded {
                    settings: AppSettings::default(),
                    problem: Some(format!(
                        "настройки не разобрать ({error}); они отложены в {}",
                        broken.display()
                    )),
                })
            }
        }
    }

    /// Writes through a temporary file, like the registry and the manifest.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("настройки сериализуемы");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    /// A file from before view modes knows only "show preview". A hidden
    /// preview is the "Edit" mode, so keep that choice instead of losing it.
    fn adopt_legacy(&mut self, bytes: &[u8]) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy {
            view_mode: Option<serde::de::IgnoredAny>,
            preview_visible: Option<bool>,
        }

        if let Ok(Legacy {
            view_mode: None,
            preview_visible: Some(false),
        }) = serde_json::from_slice(bytes)
        {
            self.view_mode = ViewMode::Edit;
        }
    }

    /// Repairs the values that were read.
    ///
    /// People edit the file by hand, so repair it instead of failing to start.
    /// An empty language leaves the UI without labels, and `autosave_ms: 0`
    /// writes to disk on each key press.
    fn tidy(&mut self) {
        if self.locale.trim().is_empty() {
            self.locale = DEFAULT_LOCALE.to_string();
        }
        if self.theme.trim().is_empty() {
            self.theme = DEFAULT_THEME.to_string();
        }
        self.autosave_ms = self
            .autosave_ms
            .clamp(*AUTOSAVE_MS.start(), *AUTOSAVE_MS.end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn defaults_are_a_working_application() {
        let settings = AppSettings::default();

        assert_eq!(settings.locale, "ru");
        assert_eq!(settings.theme, "light");
        assert_eq!(settings.view_mode, ViewMode::Split);
        assert_eq!(settings.autosave_ms, 600);
        assert!(settings.open_last_note);
        assert!(settings.line_numbers);
    }

    #[test]
    fn round_trip_through_the_file() {
        let home = dir();
        let path = AppSettings::path(home.path());
        let settings = AppSettings {
            locale: "en".to_string(),
            theme: "night".to_string(),
            view_mode: ViewMode::Read,
            autosave_ms: 1200,
            open_last_note: false,
            line_numbers: false,
        };

        settings.save(&path).unwrap();
        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings, settings);
        assert!(!home.path().join("settings.json.tmp").exists());

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""autosaveMs": 1200"#), "{written}");
        assert!(written.contains(r#""viewMode": "read""#), "{written}");
        assert!(written.contains(r#""lineNumbers": false"#), "{written}");
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let home = dir();

        let loaded = AppSettings::load(&AppSettings::path(home.path())).unwrap();

        assert_eq!(loaded.settings, AppSettings::default());
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn a_broken_file_is_set_aside_and_reported() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, "{ это не json").unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert_eq!(loaded.settings, AppSettings::default());
        assert!(loaded.problem.is_some(), "о сбросе настроек надо сказать");
        assert!(home.path().join("settings.json.broken").is_file());
    }

    #[test]
    fn a_hand_edited_file_is_repaired_not_trusted() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(
            &path,
            r#"{"locale":"  ","theme":"","autosaveMs":5,"viewMode":"edit"}"#,
        )
        .unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none(), "разобрался — претензий нет");
        assert_eq!(loaded.settings.locale, "ru");
        assert_eq!(loaded.settings.theme, "light");
        assert_eq!(loaded.settings.autosave_ms, 100, "меньше границы");
        assert_eq!(loaded.settings.view_mode, ViewMode::Edit, "это выбор человека");
        assert!(
            loaded.settings.open_last_note,
            "пропущенное поле берёт умолчание, а не `false`"
        );
    }

    #[test]
    fn a_wild_autosave_delay_is_clamped_from_above() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"autosaveMs":600000}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert_eq!(loaded.settings.autosave_ms, 5000);
    }

    #[test]
    fn an_unknown_mode_falls_back_instead_of_breaking_the_file() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"locale":"en","viewMode":"grid"}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings.locale, "en", "остальное прочитано");
        assert_eq!(loaded.settings.view_mode, ViewMode::Split);
    }

    #[test]
    fn a_hidden_preview_from_an_old_file_becomes_the_edit_mode() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"previewVisible":false}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert_eq!(loaded.settings.view_mode, ViewMode::Edit);
    }

    #[test]
    fn a_new_mode_wins_over_the_old_flag() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"previewVisible":false,"viewMode":"read"}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert_eq!(loaded.settings.view_mode, ViewMode::Read);
    }
}
