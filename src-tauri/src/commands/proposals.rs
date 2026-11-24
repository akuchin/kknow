use kknow_app::proposals::Review;
use kknow_app::threads::{CommentRef, USER_AUTHOR};
use kknow_app::view::NoteView;

use tauri::State;

use super::note_id;
use crate::state::{AppState, Error, Result};

#[tauri::command(async)]
pub fn review_proposal(
    state: State<'_, AppState>,
    proposal: String,
    review: Review,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let (reviewed, _) = workspace.review_proposal(&proposal, &review)?;
        let id = note_id(workspace, &reviewed.note)?;
        let content = workspace.read_plain(&id)?;
        Ok(workspace.note_view(&id, content))
    })
}

#[tauri::command(async)]
pub fn dismiss_proposal(
    state: State<'_, AppState>,
    proposal: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.remove_proposal(&proposal)?))
}

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn reply_thread(
    state: State<'_, AppState>,
    note: String,
    thread: Option<String>,
    line: Option<usize>,
    text: Option<String>,
    message: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &note)?;
        let target = match (thread, line, text) {
            (Some(key), _, _) => CommentRef::Id(key),
            (None, Some(line), Some(text)) => CommentRef::Place {
                note: id.clone(),
                line,
                text,
            },
            _ => return Err(Error::NotFound(note.clone())),
        };
        workspace.reply_comment(&target, USER_AUTHOR, &message, false)?;
        let content = workspace.read_plain(&id)?;
        Ok(workspace.note_view(&id, content))
    })
}
