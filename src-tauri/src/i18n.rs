//! UI texts on the Rust side: the menu and the window titles.
//!
//! They come from the same files as the window (`locales/*.json`), so a
//! translation has one place. The window language is in the settings.

use std::collections::HashMap;

/// The translation files, the same ones the window uses (`locales/*.json`).
/// A new language needs a line here too; a test checks that none is missing.
const LOCALES: &[(&str, &str)] = &[
    ("ru", include_str!("../../locales/ru.json")),
    ("en", include_str!("../../locales/en.json")),
];

/// The reference language: an unknown language or a missing key falls back
/// to it, as in the window.
const FALLBACK: &str = "ru";

/// The key of the settings window title.
pub const SETTINGS_TITLE: &str = "settings.windowTitle";

/// The text for `key` in the UI language.
pub fn label(locale: &str, key: &str) -> String {
    lookup(locale, key)
        .or_else(|| lookup(FALLBACK, key))
        .unwrap_or_else(|| key.to_string())
}

fn lookup(locale: &str, key: &str) -> Option<String> {
    let (_, source) = LOCALES.iter().find(|(name, _)| *name == locale)?;
    // The files are small, and the menu and the window titles ask for a few
    // keys, only when they appear and on a language change: parsing on each
    // call is cheaper than a cache.
    let dict: HashMap<String, String> = serde_json::from_str(source).ok()?;
    dict.get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_follows_the_language() {
        assert_eq!(label("ru", "appMenu.settings"), "Настройки…");
        assert_eq!(label("en", "appMenu.settings"), "Settings…");
        assert_eq!(label("ru", "appMenu.updates"), "Проверить обновления…");
        assert_eq!(label("en", "appMenu.updates"), "Check for Updates…");
    }

    #[test]
    fn the_settings_window_has_a_title_in_each_language() {
        assert_eq!(label("ru", SETTINGS_TITLE), "Настройки — kknow");
        assert_eq!(label("en", SETTINGS_TITLE), "Settings — kknow");
    }

    /// The window shows a shipped command in its own language while its name
    /// is still the English one from `locales/en.json`: the two must match.
    #[test]
    fn shipped_commands_have_their_names_in_the_locales() {
        for spec in kknow_run::spec::defaults().commands {
            let key = format!("command.shipped.{}", spec.id);
            assert_eq!(lookup("en", &key), Some(spec.name.clone()), "{key}");
            assert!(lookup("ru", &key).is_some(), "{key}");
        }
    }

    #[test]
    fn built_in_themes_have_their_names_in_the_locales() {
        let set = kknow_app::theme::load(None, None);
        let built_in = set
            .themes
            .iter()
            .filter(|theme| theme.source == kknow_app::theme::Source::BuiltIn);
        for theme in built_in {
            let key = format!("theme.builtIn.{}", theme.id);
            assert_eq!(lookup("en", &key), Some(theme.name.clone()), "{key}");
            assert!(lookup("ru", &key).is_some(), "{key}");
        }
    }

    #[test]
    fn an_unknown_language_falls_back_to_russian() {
        assert_eq!(label("xx", "appMenu.settings"), "Настройки…");
        assert_eq!(label("en", "no.such.key"), "no.such.key");
    }

    #[test]
    fn every_translation_file_is_listed() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../locales");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        files.sort();
        let mut listed: Vec<String> = LOCALES.iter().map(|(name, _)| name.to_string()).collect();
        listed.sort();
        assert_eq!(files, listed, "every file in locales/ is in LOCALES");
    }
}
