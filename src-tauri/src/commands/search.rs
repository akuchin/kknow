use serde::Serialize;
use tauri::State;

use crate::state::{AppState, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct GlobalResult {
    pub(super) vault: String,
    pub(super) vault_name: String,
    pub(super) color: String,
    pub(super) id: String,
    pub(super) title: String,
    pub(super) folder: Option<String>,
    pub(super) snippet: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SearchResult {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) folder: Option<String>,
    pub(super) snippet: String,
}

#[tauri::command(async)]
pub fn search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    vault: Option<String>,
) -> Result<Vec<SearchResult>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let hits = workspace.search(&query, limit.unwrap_or(50))?;

        Ok(hits
            .into_iter()
            .map(|hit| SearchResult {
                folder: hit.id.folder().map(str::to_string),
                id: hit.id.as_str().to_string(),
                title: hit.title,
                snippet: hit.snippet,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn search_all(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    vaults: Option<Vec<String>>,
) -> Result<Vec<GlobalResult>> {
    let limit = limit.unwrap_or(20);
    let mut found = Vec::new();

    for entry in state.search_order() {
        if !entry.available() {
            continue;
        }
        if vaults.as_ref().is_some_and(|ids| !ids.contains(&entry.id)) {
            continue;
        }

        let slot = match state.slot(Some(&entry.id)) {
            Ok(slot) => slot,
            Err(error) => {
                eprintln!("vault {} did not open: {error}", entry.name);
                continue;
            }
        };

        let hits = slot.read(|workspace| Ok(workspace.search(&query, limit)?))?;
        found.extend(hits.into_iter().map(|hit| GlobalResult {
            vault: entry.id.clone(),
            vault_name: entry.name.clone(),
            color: entry.color.as_str().to_string(),
            folder: hit.id.folder().map(str::to_string),
            id: hit.id.as_str().to_string(),
            title: hit.title,
            snippet: hit.snippet,
        }));
    }

    Ok(found)
}
