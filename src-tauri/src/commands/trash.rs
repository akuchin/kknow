use std::path::Path;
use std::time::UNIX_EPOCH;

use kknow_core::NoteId;
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, Error, Result};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TrashEntryView {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) deleted: u64,
    pub(super) notes: Vec<String>,
}

#[tauri::command(async)]
pub fn list_trash(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Vec<TrashEntryView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let entries = workspace
            .trash()
            .entries()
            .map_err(kknow_app::Error::from)?;
        Ok(entries
            .into_iter()
            .rev()
            .map(|entry| TrashEntryView {
                deleted: entry
                    .deleted
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |age| age.as_secs()),
                id: entry.id,
                path: entry.path,
                notes: entry.notes,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn restore_trash(
    state: State<'_, AppState>,
    entry: String,
    vault: Option<String>,
) -> Result<Vec<String>> {
    state.slot(vault.as_deref())?.write(|workspace| {
        Ok(workspace
            .restore(&entry)?
            .iter()
            .map(|id| id.as_str().to_string())
            .collect())
    })
}

#[tauri::command(async)]
pub fn trash_note(
    state: State<'_, AppState>,
    entry: String,
    note: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let content = workspace
            .trash()
            .read(&entry, &note)
            .map_err(kknow_app::Error::from)?;
        let id = NoteId::from_relative_path(Path::new(&note))
            .ok_or_else(|| Error::NotFound(note.clone()))?;
        Ok(workspace.render(&id, &content))
    })
}

#[tauri::command(async)]
pub fn delete_trash_entry(
    state: State<'_, AppState>,
    entry: String,
    vault: Option<String>,
) -> Result<()> {
    state.slot(vault.as_deref())?.write(|workspace| {
        Ok(workspace
            .trash()
            .remove(&entry)
            .map_err(kknow_app::Error::from)?)
    })
}

#[tauri::command(async)]
pub fn empty_trash(state: State<'_, AppState>, vault: Option<String>) -> Result<usize> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.trash().empty().map_err(kknow_app::Error::from)?))
}

#[tauri::command(async)]
pub fn rename_folder(
    state: State<'_, AppState>,
    name: String,
    to: String,
    vault: Option<String>,
) -> Result<usize> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.rename_folder(&name, &to)?.notes))
}

#[tauri::command(async)]
pub fn list_folders(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<String>> {
    state
        .slot(vault.as_deref())?
        .read(|workspace| Ok(workspace.folders()?))
}

#[tauri::command(async)]
pub fn create_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.create_folder(&name)?))
}

#[tauri::command(async)]
pub fn delete_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.delete_folder(&name)?))
}
