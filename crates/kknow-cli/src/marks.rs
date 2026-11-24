use kknow_app::Workspace;
use kknow_app::bookmarks::clean_name;
use kknow_app::text::task_row;
use kknow_core::NoteId;
use kknow_index::Task;

use crate::error::{CliError, Result};
use crate::notes::*;

pub(crate) fn bookmarks(workspace: &Workspace) {
    let bookmarks = workspace.bookmarks();
    if bookmarks.is_empty() {
        println!("no bookmarks");
        return;
    }
    for bookmark in bookmarks {
        let place = format!("{}:{}", bookmark.note, bookmark.line);
        let text = bookmark.text.trim();
        match &bookmark.name {
            Some(name) => println!("{place}  «{name}»  {text}"),
            None => println!("{place}  {text}"),
        }
    }
}

pub(crate) fn tasks(workspace: &Workspace, all: bool) {
    let tasks: Vec<(&NoteId, &Task)> = workspace
        .tasks()
        .into_iter()
        .filter(|(_, task)| all || !task.done)
        .collect();
    if tasks.is_empty() {
        let empty = if all { "no tasks" } else { "no open tasks" };
        println!("{empty}");
        return;
    }
    for (id, task) in tasks {
        println!("{}", task_row(id, task, all));
    }
}

pub(crate) fn bookmark(
    workspace: &mut Workspace,
    note: &str,
    line: usize,
    name: Option<&str>,
) -> Result<()> {
    let id = find(workspace, note)?.clone();
    let lines = workspace.read_plain(&id)?.lines().count();
    if line == 0 || line > lines {
        return Err(CliError::NoSuchLine(id, line, lines));
    }
    if let Some(name) = name {
        return name_bookmark(workspace, &id, line, name);
    }
    let set = workspace.toggle_bookmark(&id, line)?;
    if set {
        println!("bookmark added: {id}:{line}");
    } else {
        println!("bookmark removed: {id}:{line}");
    }
    Ok(())
}

pub(crate) fn name_bookmark(
    workspace: &mut Workspace,
    id: &NoteId,
    line: usize,
    name: &str,
) -> Result<()> {
    if !workspace.name_bookmark(id, line, Some(name))? {
        workspace.toggle_bookmark(id, line)?;
        workspace.name_bookmark(id, line, Some(name))?;
    }
    match clean_name(Some(name)) {
        Some(name) => println!("bookmark {id}:{line} is named \"{name}\""),
        None => println!("bookmark {id}:{line} no longer has a name"),
    }
    Ok(())
}
