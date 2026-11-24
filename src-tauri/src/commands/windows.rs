use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kknow_app::Workspace;
use kknow_app::theme::{self, ThemeSet};
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};

use super::note_id;
use crate::state::{AppState, Error, Result};

#[tauri::command(async)]
pub fn open_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: State<'_, crate::settings::SettingsState>,
    section: Option<String>,
) -> Result<()> {
    let section = section.unwrap_or_else(|| "general".to_string());

    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.eval(format!("location.hash = '{section}'"));
        let _ = window.set_focus();
        return Ok(());
    }

    let url = WebviewUrl::App(format!("settings.html#{section}").into());
    WebviewWindowBuilder::new(&app, SETTINGS_WINDOW, url)
        .title(crate::i18n::label(
            &settings.get().locale,
            crate::i18n::SETTINGS_TITLE,
        ))
        .inner_size(880.0, 580.0)
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|error| Error::Window(error.to_string()))?;

    Ok(())
}

pub const MAIN_WINDOW: &str = "main";

pub const SETTINGS_WINDOW: &str = "settings";

#[tauri::command(async)]
pub fn list_themes(state: State<'_, AppState>, vault: Option<String>) -> Result<ThemeSet> {
    let global = state.themes_path();
    match state.slot(vault.as_deref()) {
        Ok(slot) => slot.read(|workspace| Ok(workspace.themes(global.as_deref()))),
        Err(Error::NoVault) if vault.is_none() => Ok(theme::load(None, global.as_deref())),
        Err(error) => Err(error),
    }
}

#[tauri::command(async)]
pub fn share_theme(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    let global = state.themes_path().ok_or(Error::NoConfig)?;
    let root = state
        .slot(vault.as_deref())?
        .read(|workspace| Ok(workspace.root().to_path_buf()))?;
    theme::share(&root, &id, &global).map_err(|error| Error::ShareTheme(error.to_string()))?;
    Ok(())
}

#[tauri::command(async)]
pub fn reveal_themes(state: State<'_, AppState>) -> Result<()> {
    let dir = state.themes_path().ok_or(Error::NoConfig)?;
    fs::create_dir_all(&dir).map_err(|error| Error::Reveal(error.to_string()))?;
    open_default(&dir).map_err(|error| Error::Reveal(error.to_string()))
}

#[tauri::command(async)]
pub fn reveal_note(
    state: State<'_, AppState>,
    id: Option<String>,
    vault: Option<String>,
) -> Result<()> {
    let path = state
        .slot(vault.as_deref())?
        .read(|workspace| reveal_target(workspace, id.as_deref()))?;
    reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

pub(super) fn reveal_target(workspace: &Workspace, id: Option<&str>) -> Result<PathBuf> {
    match id {
        Some(raw) => Ok(workspace.path_for(&note_id(workspace, raw)?)),
        None => Ok(workspace.root().to_path_buf()),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    run(Command::new("open").arg("-R").arg(path))
}

#[cfg(target_os = "windows")]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    run(Command::new("explorer").arg(format!("/select,{}", path.display())))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    run(Command::new("xdg-open").arg(path.parent().unwrap_or(path)))
}

pub(super) fn run(command: &mut Command) -> std::io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(status.to_string()))
    }
}

#[tauri::command(async)]
pub fn open_note(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    let path = state
        .slot(vault.as_deref())?
        .read(|workspace| reveal_target(workspace, Some(&id)))?;
    open_default(&path).map_err(|error| Error::Open(error.to_string()))
}

#[tauri::command(async)]
pub fn reveal_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    let path = state.slot(vault.as_deref())?.read(|workspace| {
        if !workspace.folders()?.contains(&name) {
            return Err(Error::NotFound(name.clone()));
        }
        Ok(workspace.root().join(&name))
    })?;
    reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

#[cfg(target_os = "macos")]
pub(super) fn open_default(path: &Path) -> std::io::Result<()> {
    run(Command::new("open").arg(path))
}

#[cfg(target_os = "windows")]
pub(super) fn open_default(path: &Path) -> std::io::Result<()> {
    run(Command::new("cmd").args(["/C", "start", ""]).arg(path))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn open_default(path: &Path) -> std::io::Result<()> {
    run(Command::new("xdg-open").arg(path))
}
