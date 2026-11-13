//! The app menu in the macOS menu bar.
//!
//! We take the system default menu (Edit with copy and paste, Window, Quit)
//! and add "Settings…" and "Check for Updates…" to the app submenu, where
//! macOS users look for them. "Settings…" opens the same settings window as ⌘,
//! in the main window. The menu takes ⌘, before the webview, so the key works
//! in every window. "Check for Updates…" opens the same window and starts a
//! check: the status shows in "General", not in a dialog.

use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use crate::i18n::label;

/// Menu item ids. The menu event handler finds the items by them.
pub const SETTINGS_ITEM: &str = "settings";
pub const UPDATES_ITEM: &str = "updates";

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
