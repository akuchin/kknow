mod error;
pub mod image;
mod note;
pub mod trash;
mod vault;

pub use error::{Error, Result, io_text};
pub use note::{MARKDOWN_EXTENSIONS, Note, NoteId, is_markdown_extension};
pub use trash::{Trash, TrashEntry};
pub use vault::{Duplicate, Vault};
