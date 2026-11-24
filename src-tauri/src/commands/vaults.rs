use kknow_app::Workspace;
use kknow_app::vaults::VaultEntry;
use kknow_sync::RemoteConfig;
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct VaultInfo {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) root: String,
    pub(super) color: String,
    pub(super) note_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct VaultView {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) path: String,
    pub(super) color: String,
    pub(super) labels: Vec<String>,
    pub(super) active: bool,
    pub(super) available: bool,
    pub(super) git: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "VaultStats"))]
pub struct VaultStatsView {
    pub(super) notes: usize,
    pub(super) folders: usize,
    pub(super) bytes: u64,
}

pub(super) fn info(entry: &VaultEntry, workspace: &Workspace) -> VaultInfo {
    VaultInfo {
        id: entry.id.clone(),
        name: entry.name.clone(),
        color: entry.color.as_str().to_string(),
        root: workspace.root().to_string_lossy().into_owned(),
        note_count: workspace.len(),
    }
}

pub(super) fn view(entry: &VaultEntry, active: Option<&str>) -> VaultView {
    VaultView {
        active: active == Some(entry.id.as_str()),
        available: entry.available(),
        id: entry.id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().into_owned(),
        color: entry.color.as_str().to_string(),
        labels: entry.labels.clone(),
        git: RemoteConfig::load(&RemoteConfig::path(&entry.path))
            .ok()
            .flatten()
            .is_some_and(|config| config.is_git()),
    }
}

#[tauri::command(async)]
pub fn list_vaults(state: State<'_, AppState>) -> Vec<VaultView> {
    let active = state.active().map(|entry| entry.id);
    state
        .entries()
        .iter()
        .map(|entry| view(entry, active.as_deref()))
        .collect()
}

#[tauri::command(async)]
pub fn add_vault(state: State<'_, AppState>, path: String) -> Result<VaultInfo> {
    let entry = state.add(path.into(), None)?;
    activate_vault(state, entry.id)
}

#[tauri::command(async)]
pub fn remove_vault(state: State<'_, AppState>, id: String) -> Result<Vec<VaultView>> {
    state.remove(&id)?;
    Ok(list_vaults(state))
}

#[tauri::command(async)]
pub fn activate_vault(state: State<'_, AppState>, id: String) -> Result<VaultInfo> {
    let entry = state.set_active(&id)?;
    let slot = state.slot(Some(&entry.id))?;
    slot.read(|workspace| Ok(info(&entry, workspace)))
}

#[tauri::command(async)]
pub fn rename_vault(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<Vec<VaultView>> {
    state.rename(&id, &name)?;
    Ok(list_vaults(state))
}

#[tauri::command(async)]
pub fn set_vault_color(
    state: State<'_, AppState>,
    id: String,
    color: String,
) -> Result<Vec<VaultView>> {
    state.set_color(&id, &color)?;
    Ok(list_vaults(state))
}

#[tauri::command(async)]
pub fn set_vault_labels(
    state: State<'_, AppState>,
    id: String,
    labels: Vec<String>,
) -> Result<Vec<VaultView>> {
    state.set_labels(&id, &labels)?;
    Ok(list_vaults(state))
}

#[tauri::command(async)]
pub fn vault_stats(state: State<'_, AppState>, vault: Option<String>) -> Result<VaultStatsView> {
    let slot = state.slot(vault.as_deref())?;
    slot.read(|workspace| {
        let stats = workspace.stats();
        Ok(VaultStatsView {
            notes: stats.notes,
            folders: stats.folders,
            bytes: stats.bytes,
        })
    })
}

#[tauri::command]
pub fn vault_by_hotkey(state: State<'_, AppState>, number: usize) -> Option<String> {
    state.by_hotkey(number)
}

#[tauri::command(async)]
pub fn current_vault(state: State<'_, AppState>) -> Option<VaultInfo> {
    let entry = state.active()?;
    let slot = state.slot(Some(&entry.id)).ok()?;
    slot.read(|workspace| Ok(info(&entry, workspace))).ok()
}

#[tauri::command(async)]
pub fn reload_vault(state: State<'_, AppState>, vault: Option<String>) -> Result<VaultInfo> {
    let slot = state.slot(vault.as_deref())?;
    slot.write(|workspace| {
        workspace.reload()?;
        Ok(info(&slot.entry, workspace))
    })
}
