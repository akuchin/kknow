use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use tauri::State;
use tauri::ipc::{InvokeBody, Request, Response};

use super::note_id;
use crate::state::{AppState, Error, Result};

#[tauri::command(async)]
pub fn note_image(
    state: State<'_, AppState>,
    id: String,
    target: String,
    vault: Option<String>,
) -> Result<Response> {
    let path = state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace
            .image(&id, &target)
            .ok_or(Error::ImageNotFound(target))
    })?;
    let bytes = kknow_core::image::read(&path).map_err(kknow_app::Error::from)?;
    Ok(Response::new(bytes))
}

#[tauri::command(async)]
pub fn save_attachment(state: State<'_, AppState>, request: Request<'_>) -> Result<String> {
    let header = |key: &str| {
        let value = request.headers().get(key)?.to_str().ok()?;
        Some(percent_decode_str(value).decode_utf8_lossy().into_owned())
    };
    let dir = attachment_dir(&state, header("kknow-file").as_deref())?;
    let name = header("kknow-name").unwrap_or_default();
    let saved = match request.body() {
        InvokeBody::Raw(bytes) => kknow_core::image::save(&dir, &name, bytes),
        InvokeBody::Json(value) => {
            let bytes: Vec<u8> =
                serde_json::from_value(value.clone()).map_err(|_| Error::NoImageBytes)?;
            kknow_core::image::save(&dir, &name, &bytes)
        }
    };
    Ok(saved.map_err(kknow_app::Error::from)?)
}

#[tauri::command(async)]
pub fn attach_file(
    state: State<'_, AppState>,
    source: String,
    file: Option<String>,
) -> Result<String> {
    let dir = attachment_dir(&state, file.as_deref())?;
    Ok(kknow_core::image::import(&dir, Path::new(&source)).map_err(kknow_app::Error::from)?)
}

pub(super) fn attachment_dir(state: &AppState, file: Option<&str>) -> Result<PathBuf> {
    match file {
        Some(file) => {
            let path = state.known_file(file)?;
            let dir = path
                .parent()
                .ok_or_else(|| Error::UnknownFile(file.to_string()))?;
            Ok(dir.to_path_buf())
        }
        None => state
            .slot(None)?
            .read(|workspace| Ok(workspace.root().to_path_buf())),
    }
}
