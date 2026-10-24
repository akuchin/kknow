//! kknow core: the model of a note vault.
//!
//! This layer knows nothing about the UI, search or sync. It answers one
//! question: which notes are in the vault and how to read them.

mod error;
pub mod image;
mod note;
pub mod trash;
mod vault;

pub use error::{Error, Result};
pub use note::{MARKDOWN_EXTENSIONS, Note, NoteId, is_markdown_extension};
pub use trash::{Trash, TrashEntry};
pub use vault::Vault;
