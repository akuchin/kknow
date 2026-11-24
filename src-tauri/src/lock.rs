use kknow_app::lock::Unlocked;
use serde::Serialize;
use tauri::State;

use crate::commands::note_id;
use crate::state::{AppState, Result};
use kknow_app::view::NoteView;

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LockStateView {
    password: bool,
    unlocked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum UnlockedBy {
    Password,
    RecoveryCode,
}

#[tauri::command(async)]
pub fn lock_state(state: State<'_, AppState>, vault: Option<String>) -> Result<LockStateView> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let lock = workspace.lock_state();
        Ok(LockStateView {
            password: lock.password,
            unlocked: lock.unlocked,
        })
    })
}

#[tauri::command(async)]
pub fn set_lock_password(
    state: State<'_, AppState>,
    password: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let code = workspace.set_lock_password(&password)?;
        Ok(code.display().to_string())
    })
}

#[tauri::command(async)]
pub fn unlock_notes(
    state: State<'_, AppState>,
    secret: String,
    vault: Option<String>,
) -> Result<UnlockedBy> {
    state.slot(vault.as_deref())?.write(|workspace| {
        Ok(match workspace.unlock_notes(&secret)? {
            Unlocked::Password => UnlockedBy::Password,
            Unlocked::RecoveryCode => UnlockedBy::RecoveryCode,
        })
    })
}

#[tauri::command(async)]
pub fn change_lock_password(
    state: State<'_, AppState>,
    password: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.change_lock_password(&password)?))
}

#[tauri::command(async)]
pub fn lock_all(state: State<'_, AppState>, vault: Option<String>) -> Result<()> {
    state.slot(vault.as_deref())?.write(|workspace| {
        workspace.lock_all();
        Ok(())
    })
}

#[tauri::command(async)]
pub fn lock_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.lock_note(&id)?;
        Ok(workspace.open_view(&id)?)
    })
}

#[tauri::command(async)]
pub fn remove_lock(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.remove_lock(&id)?;
        Ok(workspace.open_view(&id)?)
    })
}
