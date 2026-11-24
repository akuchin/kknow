use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use crate::i18n::label;

pub const SETTINGS_ITEM: &str = "settings";
pub const UPDATES_ITEM: &str = "updates";

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
        app_menu.insert_items(
            &[&settings, &updates, &PredefinedMenuItem::separator(app)?],
            2,
        )?;
    }
    Ok(menu)
}

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
