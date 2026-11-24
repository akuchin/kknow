use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{Error, Result};

pub const SETTINGS_FILE: &str = "settings.json";

const DEFAULT_LOCALE: &str = "ru";
const DEFAULT_THEME: &str = "light";

const NOTE_SCALE: RangeInclusive<u32> = 70..=200;
const NOTE_SCALE_STEP: u32 = 10;
const DEFAULT_NOTE_SCALE: u32 = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ViewMode {
    Edit,
    #[default]
    Split,
    Read,
}

fn lenient_mode<'de, D: Deserializer<'de>>(raw: D) -> std::result::Result<ViewMode, D::Error> {
    Ok(match String::deserialize(raw)?.as_str() {
        "edit" => ViewMode::Edit,
        "read" => ViewMode::Read,
        _ => ViewMode::Split,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum NoteLabel {
    #[default]
    Title,
    File,
}

fn lenient_label<'de, D: Deserializer<'de>>(raw: D) -> std::result::Result<NoteLabel, D::Error> {
    Ok(match String::deserialize(raw)?.as_str() {
        "file" => NoteLabel::File,
        _ => NoteLabel::Title,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AppSettings {
    pub locale: String,
    pub theme: String,
    #[serde(deserialize_with = "lenient_label")]
    pub note_label: NoteLabel,
    #[serde(deserialize_with = "lenient_mode")]
    pub view_mode: ViewMode,
    pub open_last_note: bool,
    pub line_numbers: bool,
    pub note_scale: u32,
    pub auto_update: bool,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub git_path: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            locale: DEFAULT_LOCALE.to_string(),
            theme: DEFAULT_THEME.to_string(),
            note_label: NoteLabel::Title,
            view_mode: ViewMode::Split,
            open_last_note: true,
            line_numbers: true,
            note_scale: DEFAULT_NOTE_SCALE,
            auto_update: true,
            git_path: None,
        }
    }
}

#[derive(Debug)]
pub struct Loaded {
    pub settings: AppSettings,
    pub problem: Option<String>,
}

impl AppSettings {
    pub fn path(config_dir: impl AsRef<Path>) -> PathBuf {
        config_dir.as_ref().join(SETTINGS_FILE)
    }

    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
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
                        "cannot parse the settings ({error}); they are moved to {}",
                        broken.display()
                    )),
                })
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("settings serialize");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

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

    fn tidy(&mut self) {
        if self.locale.trim().is_empty() {
            self.locale = DEFAULT_LOCALE.to_string();
        }
        if self.theme.trim().is_empty() {
            self.theme = DEFAULT_THEME.to_string();
        }
        let stepped = (self.note_scale + NOTE_SCALE_STEP / 2) / NOTE_SCALE_STEP * NOTE_SCALE_STEP;
        self.note_scale = stepped.clamp(*NOTE_SCALE.start(), *NOTE_SCALE.end());
        self.git_path = self
            .git_path
            .take()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty());
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
        assert_eq!(settings.note_label, NoteLabel::Title);
        assert_eq!(settings.view_mode, ViewMode::Split);
        assert!(settings.open_last_note);
        assert!(settings.line_numbers);
        assert_eq!(settings.note_scale, 100);
        assert!(settings.auto_update);
    }

    #[test]
    fn round_trip_through_the_file() {
        let home = dir();
        let path = AppSettings::path(home.path());
        let settings = AppSettings {
            locale: "en".to_string(),
            theme: "night".to_string(),
            note_label: NoteLabel::File,
            view_mode: ViewMode::Read,
            open_last_note: false,
            line_numbers: false,
            note_scale: 130,
            auto_update: false,
            git_path: Some("/opt/homebrew/bin/git".to_string()),
        };

        settings.save(&path).unwrap();
        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings, settings);
        assert!(!home.path().join("settings.json.tmp").exists());

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(r#""viewMode": "read""#), "{written}");
        assert!(written.contains(r#""lineNumbers": false"#), "{written}");
        assert!(written.contains(r#""autoUpdate": false"#), "{written}");
        assert!(written.contains(r#""noteScale": 130"#), "{written}");
        assert!(written.contains(r#""noteLabel": "file""#), "{written}");
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
        assert_eq!(
            loaded.settings.view_mode,
            ViewMode::Edit,
            "это выбор человека"
        );
        assert!(
            loaded.settings.open_last_note,
            "пропущенное поле берёт умолчание, а не `false`"
        );
        assert!(
            loaded.settings.auto_update,
            "старый файл без поля не выключает обновления"
        );
    }

    #[test]
    fn an_old_autosave_delay_in_the_file_is_ignored() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"locale":"en","autosaveMs":1200}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings.locale, "en");
        loaded.settings.save(&path).unwrap();
        let resaved = fs::read_to_string(&path).unwrap();
        assert!(!resaved.contains("autosaveMs"), "{resaved}");
    }

    #[test]
    fn a_wild_note_scale_is_clamped_and_snapped_to_a_step() {
        let home = dir();
        let path = AppSettings::path(home.path());

        for (raw, expected) in [(5, 70), (1000, 200), (124, 120), (125, 130)] {
            fs::write(&path, format!(r#"{{"noteScale":{raw}}}"#)).unwrap();
            let loaded = AppSettings::load(&path).unwrap();
            assert_eq!(loaded.settings.note_scale, expected, "из {raw}");
        }
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
    fn an_unknown_note_label_falls_back_to_the_title() {
        let home = dir();
        let path = AppSettings::path(home.path());
        fs::write(&path, r#"{"locale":"en","noteLabel":"path"}"#).unwrap();

        let loaded = AppSettings::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(loaded.settings.locale, "en", "остальное прочитано");
        assert_eq!(loaded.settings.note_label, NoteLabel::Title);
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
