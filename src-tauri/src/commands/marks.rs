use kknow_app::view::NoteView;

use kknow_app::bookmarks::Mark;
use kknow_core::NoteId;
use serde::Serialize;
use tauri::State;

use super::note_id;
use crate::state::{AppState, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookmarkView {
    pub(super) note: String,
    pub(super) title: String,
    pub(super) folder: Option<String>,
    pub(super) line: usize,
    pub(super) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(super) name: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TaskView {
    pub(super) note: String,
    pub(super) title: String,
    pub(super) folder: Option<String>,
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) done: bool,
}

#[tauri::command(async)]
pub fn set_bookmarks(
    state: State<'_, AppState>,
    id: String,
    lines: Vec<Mark>,
    content: String,
    vault: Option<String>,
) -> Result<Vec<Mark>> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.set_bookmarks(&id, &lines, &content)?;
        Ok(workspace.note_bookmarks(&id, &content))
    })
}

#[tauri::command(async)]
pub fn list_bookmarks(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Vec<BookmarkView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .bookmarks()
            .into_iter()
            .filter_map(|bookmark| {
                let id = NoteId::parse(&bookmark.note)?;
                Some(BookmarkView {
                    title: workspace.note_title(&id),
                    folder: id.folder().map(str::to_string),
                    note: bookmark.note.clone(),
                    line: bookmark.line,
                    text: bookmark.text.clone(),
                    name: bookmark.name.clone(),
                })
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn list_tasks(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<TaskView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .tasks()
            .into_iter()
            .map(|(id, task)| TaskView {
                note: id.as_str().to_string(),
                title: workspace.note_title(id),
                folder: id.folder().map(str::to_string),
                line: task.line,
                text: task.text.clone(),
                done: task.done,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn set_task(
    state: State<'_, AppState>,
    id: String,
    line: usize,
    done: bool,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        let content = workspace.set_task_unlocked(&id, line, done)?;
        Ok(workspace.note_view(&id, content))
    })
}
