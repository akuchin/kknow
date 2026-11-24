use kknow_app::Workspace;
use kknow_core::NoteId;

use crate::state::{Error, Result};

pub mod attachments;
pub mod links;
pub mod marks;
pub mod notes;
pub mod proposals;
pub mod search;
#[cfg(test)]
mod tests;
pub mod trash;
pub mod vaults;
pub mod windows;

pub(crate) fn note_id(workspace: &Workspace, raw: &str) -> Result<NoteId> {
    workspace
        .find(raw)
        .cloned()
        .ok_or_else(|| Error::NotFound(raw.to_string()))
}
