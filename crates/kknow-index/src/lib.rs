//! Markdown note parsing and the link graph between notes.
//!
//! The crate works with note text and ids but does not read the disk.
//! [`kknow_core::Vault`] supplies the files.

pub mod agent_note;
pub mod graph;
pub mod parse;
pub mod relink;
pub mod render;
pub mod search;
pub mod tag;
pub mod wikilink;

pub use graph::{BrokenLink, Edge, EdgeKind, LinkGraph, Resolver};
pub use parse::{Frontmatter, Heading, MarkdownLink, ParsedNote, parse};
pub use relink::Relink;
pub use render::to_html;
pub use search::{SearchHit, SearchIndex};
pub use tag::Tag;
pub use wikilink::{Subpath, WikiKind, WikiLink};
