use kknow_app::view::{LinkPreview, LinkRef};

use kknow_core::NoteId;
use serde::Serialize;
use tauri::State;

use super::note_id;
use crate::state::{AppState, Error, Result};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LinkTargetView {
    pub(super) id: String,
    pub(super) line: Option<usize>,
    pub(super) headings: Vec<HeadingView>,
    pub(super) blocks: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BacklinkLineView {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) line: usize,
    pub(super) before: String,
    pub(super) text: String,
    pub(super) after: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MentionView {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) line: usize,
    pub(super) before: String,
    pub(super) text: String,
    pub(super) after: String,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct HeadingView {
    pub(super) level: u8,
    pub(super) text: String,
    pub(super) line: usize,
}

#[tauri::command(async)]
pub fn link_target(
    state: State<'_, AppState>,
    from: String,
    target: String,
    vault: Option<String>,
) -> Result<Option<LinkTargetView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let from = NoteId::parse(&from).ok_or_else(|| Error::NotFound(from.clone()))?;
        Ok(workspace
            .link_target(&from, &target)
            .map(|found| LinkTargetView {
                id: found.id.as_str().to_string(),
                line: found.line,
                headings: found
                    .headings
                    .iter()
                    .map(|heading| HeadingView {
                        level: heading.level,
                        text: heading.text.clone(),
                        line: heading.line,
                    })
                    .collect(),
                blocks: found.blocks.iter().map(|block| block.id.clone()).collect(),
            }))
    })
}

#[tauri::command(async)]
pub fn note_backlinks(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<Vec<BacklinkLineView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace
            .backlink_lines(&id)
            .into_iter()
            .map(|link| BacklinkLineView {
                id: link.edge.from.as_str().to_string(),
                title: workspace.note_title(&link.edge.from),
                line: link.context.line,
                before: link.context.before,
                text: link.context.text,
                after: link.context.after,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn unlinked_mentions(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<Vec<MentionView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace
            .unlinked_mentions(&id)?
            .into_iter()
            .map(|mention| MentionView {
                id: mention.from.as_str().to_string(),
                title: workspace.note_title(&mention.from),
                line: mention.context.line,
                before: mention.context.before,
                text: mention.context.text,
                after: mention.context.after,
                start: mention.span.start,
                end: mention.span.end,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn link_mention(
    state: State<'_, AppState>,
    from: String,
    start: usize,
    end: usize,
    text: String,
    target: String,
    vault: Option<String>,
) -> Result<Vec<LinkRef>> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let from = note_id(workspace, &from)?;
        let target = note_id(workspace, &target)?;
        workspace.link_mention(&from, start..end, &text, &target)?;
        Ok(workspace.link_refs(workspace.backlinks(&target), |edge| &edge.from))
    })
}

#[tauri::command(async)]
pub fn link_preview(
    state: State<'_, AppState>,
    from: String,
    target: String,
    vault: Option<String>,
) -> Result<Option<LinkPreview>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let from = NoteId::parse(&from).ok_or_else(|| Error::NotFound(from.clone()))?;
        Ok(workspace.link_preview(&from, &target)?)
    })
}
