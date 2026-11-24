use kknow_core::NoteId;
use kknow_index::{Edge, EdgeKind, Subpath, Task};

use crate::BacklinkLine;

pub fn task_row(id: &NoteId, task: &Task, all: bool) -> String {
    let place = format!("{id}:{}", task.line);
    if !all {
        return format!("{place}  {}", task.text);
    }
    let mark = if task.done { 'x' } else { ' ' };
    format!("{place}  [{mark}] {}", task.text)
}

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

pub fn backlink_row(link: &BacklinkLine) -> String {
    let context = &link.context;
    format!(
        "{}:{}  {}{}{}",
        link.edge.from, context.line, context.before, context.text, context.after
    )
}

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
