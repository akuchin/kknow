//! The app menu in the macOS menu bar.
//!
//! We take the system default menu (Edit with copy and paste, Window, Quit)
//! and add "Settings…" to the app submenu, where macOS users look for it.
//! The item opens the same settings window as ⌘, in the main window.
//! The menu takes ⌘, before the webview, so the key works in every window.

use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

/// Menu item id. The menu event handler finds the item by it.
pub const SETTINGS_ITEM: &str = "settings";

/// The item text in the UI language. Any language other than English is
/// Russian, as in the settings file.
pub fn settings_label(locale: &str) -> &'static str {
    if locale == "en" {
        "Settings…"
    } else {
        "Настройки…"
    }
}

/// The default menu with "Settings…" after "About" in the app submenu.
pub fn build<R: Runtime>(app: &AppHandle<R>, locale: &str) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        let settings = MenuItem::with_id(
            app,
            SETTINGS_ITEM,
            settings_label(locale),
            true,
            Some("CmdOrCtrl+,"),
        )?;
        // About is at 0 and a separator at 1: the macOS order is About,
        // Settings, then Services.
        app_menu.insert_items(&[&settings, &PredefinedMenuItem::separator(app)?], 2)?;
    }
    Ok(menu)
}

/// Changes the item text when the user changes the language.
pub fn relabel<R: Runtime>(app: &AppHandle<R>, locale: &str) {
    let Some(menu) = app.menu() else { return };
    let Ok(items) = menu.items() else { return };
    let Some(MenuItemKind::Submenu(app_menu)) = items.into_iter().next() else {
        return;
    };
    if let Some(MenuItemKind::MenuItem(item)) = app_menu.get(SETTINGS_ITEM) {
        let _ = item.set_text(settings_label(locale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_follows_the_language() {
        assert_eq!(settings_label("ru"), "Настройки…");
        assert_eq!(settings_label("en"), "Settings…");
    }
}
