//! Text rows that the CLI and the MCP server print in the same form.
//!
//! An agent reads the same rows as a person in the terminal: one format is
//! less to learn for both, and a test of one checks the other.

use kknow_core::NoteId;
use kknow_index::{Edge, EdgeKind, Subpath, Task};

/// `note:line  text`. With `all`, the box shows which tasks are done.
pub fn task_row(id: &NoteId, task: &Task, all: bool) -> String {
    let place = format!("{id}:{}", task.line);
    if !all {
        return format!("{place}  {}", task.text);
    }
    let mark = if task.done { 'x' } else { ' ' };
    format!("{place}  [{mark}] {}", task.text)
}

/// A link row: the other note, its heading or block, the label and the kind.
pub fn describe(edge: &Edge, other: &NoteId) -> String {
    let mut line = other.to_string();

    match &edge.subpath {
        Some(Subpath::Heading(heading)) => line.push_str(&format!("#{heading}")),
        Some(Subpath::Block(block)) => line.push_str(&format!("#^{block}")),
        None => {}
    }
    if let Some(label) = &edge.label {
        line.push_str(&format!("  «{label}»"));
    }
    match edge.kind {
        EdgeKind::Embed => line.push_str("  [embed]"),
        EdgeKind::Markdown => line.push_str("  [md]"),
        EdgeKind::Wiki => {}
    }

    line
}

/// A search snippet for text output, on one line.
///
/// The snippet has HTML highlighting, which a text reader does not need.
pub fn plain_snippet(snippet: &str) -> String {
    snippet
        .replace("<b>", "«")
        .replace("</b>", "»")
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_row_shows_the_box_only_with_all() {
        let id = NoteId::parse("Проекты/План").unwrap();
        let note = kknow_index::parse("# План\n\n- [x] начать\n");
        let task = &note.tasks[0];

        assert_eq!(task_row(&id, task, false), "Проекты/План:3  начать");
        assert_eq!(task_row(&id, task, true), "Проекты/План:3  [x] начать");
    }
}
