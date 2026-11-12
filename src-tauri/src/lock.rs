//! Commands of locked notes. The session and the crypto live in
//! `kknow_app::lock`; this module only translates them for the window.
//!
//! The notes key stays in the `Workspace` of the vault slot, so it lives as
//! long as the process and never reaches the keychain.

use kknow_app::lock::Unlocked;
use serde::Serialize;
use tauri::State;

use crate::commands::{NoteView, note_id, open_view};
use crate::state::{AppState, Result};

/// The lock state of a vault.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LockStateView {
    /// The vault has a password for locked notes.
    password: bool,
    /// Locked notes open: the key is in memory.
    unlocked: bool,
}

/// What opened the session. After the recovery code, the window asks for a
/// new password.
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

/// Sets the first password of the vault and unlocks the session. The answer
/// is the recovery code: the window shows it once.
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

/// Opens all locked notes of the vault with the password or the recovery
/// code.
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

/// A new password in an unlocked session. Only `lock.json` changes.
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

/// Drops the key from memory: all locked notes close.
#[tauri::command(async)]
pub fn lock_all(state: State<'_, AppState>, vault: Option<String>) -> Result<()> {
    state.slot(vault.as_deref())?.write(|workspace| {
        workspace.lock_all();
        Ok(())
    })
}

/// Locks a note and returns its view: the note stays open.
#[tauri::command(async)]
pub fn lock_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.lock_note(&id)?;
        open_view(workspace, &id)
    })
}

/// Writes a locked note back as plain text and returns its view.
#[tauri::command(async)]
pub fn remove_lock(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.remove_lock(&id)?;
        open_view(workspace, &id)
    })
}
