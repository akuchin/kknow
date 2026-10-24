//! The app menu in the macOS menu bar.
//!
//! We take the system default menu (Edit with copy and paste, Window, Quit)
//! and add "Settings…" and "Check for Updates…" to the app submenu, where
//! macOS users look for them. "Settings…" opens the same settings window as ⌘,
//! in the main window. The menu takes ⌘, before the webview, so the key works
//! in every window. "Check for Updates…" opens the same window and starts a
//! check: the status shows in "General", not in a dialog.

use std::collections::HashMap;

use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

/// Menu item ids. The menu event handler finds the items by them.
pub const SETTINGS_ITEM: &str = "settings";
pub const UPDATES_ITEM: &str = "updates";

/// The translation files, the same ones the window uses (`locales/*.json`).
/// A new language needs a line here too; a test checks that none is missing.
const LOCALES: &[(&str, &str)] = &[
    ("ru", include_str!("../../locales/ru.json")),
    ("en", include_str!("../../locales/en.json")),
];

/// The reference language: an unknown language or a missing key falls back
/// to it, as in the window.
const FALLBACK: &str = "ru";

/// The text for `key` in the UI language.
pub fn label(locale: &str, key: &str) -> String {
    lookup(locale, key)
        .or_else(|| lookup(FALLBACK, key))
        .unwrap_or_else(|| key.to_string())
}

fn lookup(locale: &str, key: &str) -> Option<String> {
    let (_, source) = LOCALES.iter().find(|(name, _)| *name == locale)?;
    // The files are small and the menu asks for two keys, only at start and
    // on a language change: parsing on each call is cheaper than a cache.
    let dict: HashMap<String, String> = serde_json::from_str(source).ok()?;
    dict.get(key).cloned()
}

/// The default menu with "Settings…" and "Check for Updates…" after "About"
/// in the app submenu.
pub fn build<R: Runtime>(app: &AppHandle<R>, locale: &str) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        let settings = MenuItem::with_id(
            app,
            SETTINGS_ITEM,
            label(locale, "appMenu.settings"),
            true,
            Some("CmdOrCtrl+,"),
        )?;
        let updates = MenuItem::with_id(
            app,
            UPDATES_ITEM,
            label(locale, "appMenu.updates"),
            true,
            None::<&str>,
        )?;
        // About is at 0 and a separator at 1: the macOS order is About,
        // Settings, then Services. Zed and Sublime put the update item next
        // to Settings too.
        app_menu.insert_items(
            &[&settings, &updates, &PredefinedMenuItem::separator(app)?],
            2,
        )?;
    }
    Ok(menu)
}

/// Changes the item texts when the user changes the language.
pub fn relabel<R: Runtime>(app: &AppHandle<R>, locale: &str) {
    let Some(menu) = app.menu() else { return };
    let Ok(items) = menu.items() else { return };
    let Some(MenuItemKind::Submenu(app_menu)) = items.into_iter().next() else {
        return;
    };
    if let Some(MenuItemKind::MenuItem(item)) = app_menu.get(SETTINGS_ITEM) {
        let _ = item.set_text(label(locale, "appMenu.settings"));
    }
    if let Some(MenuItemKind::MenuItem(item)) = app_menu.get(UPDATES_ITEM) {
        let _ = item.set_text(label(locale, "appMenu.updates"));
    }
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
    fn an_unknown_language_falls_back_to_russian() {
        assert_eq!(label("xx", "appMenu.settings"), "Настройки…");
        assert_eq!(label("en", "нет.такого"), "нет.такого");
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
        assert_eq!(files, listed, "каждый файл из locales/ есть в LOCALES");
    }
}
