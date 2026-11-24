use kknow_app::view::NoteView;

use kknow_app::bookmarks::Mark;
use kknow_app::{Saved, Workspace};
use kknow_core::NoteId;
use serde::Serialize;
use tauri::State;

use super::note_id;
use crate::state::{AppState, Result};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct NoteSummary {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) folder: Option<String>,
    pub(super) tags: Vec<String>,
    pub(super) locked: bool,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RenamedNote {
    pub(super) id: String,
    pub(super) relinked: usize,
}

#[tauri::command(async)]
pub fn list_notes(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<NoteSummary>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .notes()
            .map(|(id, parsed)| NoteSummary {
                id: id.as_str().to_string(),
                title: parsed.title().unwrap_or_else(|| id.name()).to_string(),
                folder: id.folder().map(str::to_string),
                tags: parsed.tags.iter().map(|tag| tag.name.clone()).collect(),
                locked: workspace.is_locked(id),
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn load_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    let slot = state.slot(vault.as_deref())?;
    let view = slot.read(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.open_view(&id)?)
    })?;
    if !view.has_agent_answers() {
        return Ok(view);
    }
    slot.write(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.load_note(&id)?)
    })
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SaveOutcome {
    Saved { note: NoteView },
    Conflict { note: NoteView, copy: String },
}

#[tauri::command(async)]
pub fn save_note(
    state: State<'_, AppState>,
    id: String,
    content: String,
    base: Option<String>,
    bookmarks: Option<Vec<Mark>>,
    vault: Option<String>,
) -> Result<SaveOutcome> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        let written = |workspace: &mut Workspace, content: String| -> Result<SaveOutcome> {
            if let Some(marks) = &bookmarks {
                workspace.set_bookmarks(&id, marks, &content)?;
            }
            Ok(SaveOutcome::Saved {
                note: workspace.note_view(&id, content),
            })
        };
        let Some(base) = base else {
            workspace.save(&id, &content)?;
            return written(workspace, content);
        };
        Ok(match workspace.save_if_unchanged(&id, &content, &base)? {
            Saved::Written => written(workspace, content)?,
            Saved::Conflict { copy, disk } => SaveOutcome::Conflict {
                note: workspace.note_view(&id, disk),
                copy: copy.as_str().to_string(),
            },
        })
    })
}

#[tauri::command(async)]
pub fn preview(
    state: State<'_, AppState>,
    id: String,
    content: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.render(&id, &content))
    })
}

#[tauri::command(async)]
pub fn create_note(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = workspace.create(&name)?;
        let content = workspace.read(&id)?;
        Ok(workspace.note_view(&id, content))
    })
}

#[tauri::command(async)]
pub fn rename_note(
    state: State<'_, AppState>,
    id: String,
    to: String,
    vault: Option<String>,
) -> Result<RenamedNote> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let from = note_id(workspace, &id)?;
        let to = NoteId::parse(&to).ok_or_else(|| kknow_app::Error::InvalidName(to.clone()))?;
        let relinked = workspace.rename(&from, &to)?;
        Ok(RenamedNote {
            id: to.as_str().to_string(),
            relinked: relinked.notes,
        })
    })
}

#[tauri::command(async)]
pub fn duplicate_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.duplicate(&id)?.as_str().to_string())
    })
}

#[tauri::command(async)]
pub fn delete_note(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.delete(&id)?;
        Ok(())
    })
}
