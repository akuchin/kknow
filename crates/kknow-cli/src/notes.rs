use std::collections::BTreeMap;
use std::fmt::Write;

use kknow_app::Workspace;
use kknow_app::text::{backlink_row, describe, plain_snippet};
use kknow_core::NoteId;

use crate::error::{CliError, Result};

pub(crate) fn warn_about_duplicates(workspace: &Workspace) {
    let root = workspace.root();
    let shown = |path: &std::path::Path| {
        path.strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    for duplicate in workspace.duplicates() {
        eprintln!(
            "warning: two files for the note {}: kept {}, skipped {}",
            duplicate.id,
            shown(&duplicate.kept),
            shown(&duplicate.skipped)
        );
    }
}

pub(crate) fn find<'a>(workspace: &'a Workspace, raw: &str) -> Result<&'a NoteId> {
    workspace
        .find(raw)
        .ok_or_else(|| CliError::NoteNotFound(raw.to_string()))
}

pub(crate) fn list(workspace: &Workspace) {
    for (id, parsed) in workspace.notes() {
        match parsed.title() {
            Some(title) if title != id.name() => println!("{id}  ({title})"),
            _ => println!("{id}"),
        }
    }
}

pub(crate) fn stats(workspace: &Workspace) {
    let graph = workspace.graph();
    let tag_count: usize = workspace.notes().map(|(_, parsed)| parsed.tags.len()).sum();
    let stats = workspace.stats();

    println!("vault:          {}", workspace.root().display());
    println!("notes:            {}", graph.note_count());
    println!("folders:          {}", stats.folders);
    println!("links:            {}", graph.edge_count());
    println!("broken links:     {}", graph.broken().len());
    println!("tags:             {tag_count}");
    println!("no backlinks:     {}", graph.orphans().count());
    println!("size:             {}", human_size(stats.bytes));

    let mut popular: Vec<(&NoteId, usize)> = workspace
        .notes()
        .map(|(id, _)| (id, graph.backlinks(id).len()))
        .filter(|(_, count)| *count > 0)
        .collect();
    popular.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    if !popular.is_empty() {
        println!("\nmost linked:");
        for (id, count) in popular.iter().take(5) {
            println!("  {count:>3}  {id}");
        }
    }
}

pub(crate) fn human_size(bytes: u64) -> String {
    const STEP: f64 = 1024.0;
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];

    if (bytes as f64) < STEP {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64 / STEP;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < STEP {
            break;
        }
        value /= STEP;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

pub(crate) fn links(workspace: &Workspace, note: &str) -> Result<()> {
    let id = find(workspace, note)?;

    let edges = workspace.outgoing(id);
    if edges.is_empty() {
        println!("{id}: no outgoing links");
        return Ok(());
    }

    println!("{id} links to:");
    for edge in edges {
        println!("  {}", describe(edge, &edge.to));
    }
    Ok(())
}

pub(crate) fn backlinks(workspace: &Workspace, note: &str) -> Result<()> {
    let id = find(workspace, note)?;

    let lines = workspace.backlink_lines(id);
    if lines.is_empty() {
        println!("{id}: no notes link here");
        return Ok(());
    }

    println!("notes that link to {id}:");
    for line in &lines {
        println!("  {}", backlink_row(line));
    }
    Ok(())
}

pub(crate) fn tags(workspace: &Workspace) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, parsed) in workspace.notes() {
        for tag in &parsed.tags {
            *counts.entry(tag.name.as_str()).or_default() += 1;
        }
    }

    let mut sorted: Vec<(&str, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    for (name, count) in sorted {
        println!("{count:>3}  #{name}");
    }
}

pub(crate) fn broken(workspace: &Workspace) {
    for link in workspace.graph().broken() {
        println!("{}  ->  {}", link.from, link.target);
    }
}

pub(crate) fn orphans(workspace: &Workspace) {
    for id in workspace.graph().orphans() {
        println!("{id}");
    }
}

pub(crate) fn show(workspace: &mut Workspace, note: &str) -> Result<()> {
    print!("{}", show_text(workspace, note)?);
    Ok(())
}

pub(crate) fn show_text(workspace: &mut Workspace, note: &str) -> Result<String> {
    let mut out = String::new();
    let id = find(workspace, note)?.clone();
    let view = workspace.load_note(&id)?;
    let parsed = workspace.parsed(&id).expect("the note is parsed on open");

    writeln!(out, "id:        {id}").expect("a string takes text");
    writeln!(out, "title: {}", parsed.title().unwrap_or("—")).expect("a string takes text");
    writeln!(out, "file:  {}", workspace.path_for(&id).display()).expect("a string takes text");

    if view.locked {
        writeln!(out, "locked: yes").expect("a string takes text");
        return Ok(out);
    }

    if parsed.frontmatter.is_some() {
        writeln!(out, "frontmatter: yes").expect("a string takes text");
    }

    if !parsed.headings.is_empty() {
        writeln!(out, "\noutline:").expect("a string takes text");
        for heading in &parsed.headings {
            let indent = "  ".repeat(heading.level.saturating_sub(1) as usize);
            writeln!(out, "  {indent}{}", heading.text).expect("a string takes text");
        }
    }

    if !parsed.wiki_links.is_empty() {
        writeln!(out, "\nlinks:").expect("a string takes text");
        for link in &parsed.wiki_links {
            writeln!(out, "  [[{}]]", link.display_text()).expect("a string takes text");
        }
    }

    if !view.tags.is_empty() {
        writeln!(out, "\ntags:").expect("a string takes text");
        for tag in &view.tags {
            writeln!(out, "  #{tag}").expect("a string takes text");
        }
    }

    Ok(out)
}

pub(crate) fn render(workspace: &Workspace, note: &str) -> Result<()> {
    let id = find(workspace, note)?;
    let view = workspace.open_view(id)?;
    if view.locked {
        return Err(kknow_app::Error::NoteLocked(id.clone()).into());
    }

    println!("{}", view.html);
    Ok(())
}

pub(crate) fn search(workspace: &Workspace, query: &str, limit: usize) -> Result<()> {
    let hits = workspace.search(query, limit)?;

    if hits.is_empty() {
        println!("nothing found");
        return Ok(());
    }

    for hit in hits {
        println!("{:.2}  {}  ({})", hit.score, hit.title, hit.id);
        let snippet = plain_snippet(&hit.snippet);
        if !snippet.is_empty() {
            println!("      {snippet}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kknow_crypto::KdfParams;
    use std::fs;

    #[test]
    fn sizes_are_written_the_way_a_file_manager_writes_them() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(12_900_000), "12.3 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    fn open(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    #[test]
    fn show_of_a_locked_note_says_so_and_has_no_outline() {
        let (dir, mut workspace) =
            open(&[("Diary.md", "# Diary\n\n## Part\n\n#health [[Anna]]\n")]);
        let weak = KdfParams {
            memory_kib: 64,
            passes: 1,
            lanes: 1,
        };
        workspace
            .set_lock_password_with("correct horse", weak)
            .unwrap();
        let id = NoteId::parse("Diary").unwrap();
        workspace.lock_note(&id).unwrap();
        let mut closed = Workspace::open(dir.path()).unwrap();

        let text = show_text(&mut closed, "Diary").unwrap();

        assert!(text.contains("locked: yes"), "{text}");
        assert!(!text.contains("outline"), "{text}");
        assert!(!text.contains("tags"), "{text}");
        assert!(!text.contains("links"), "{text}");
    }

    #[test]
    fn show_adopts_the_agent_answers() {
        let text = "Demo zero<!-- @agent \u{ab}zero\u{bb}: start -->\n<!-- @agent-note: VARIANTS: 1) a -->\nDemo one #idea\n";
        let (_dir, mut workspace) = open(&[("Demo.md", text)]);

        let shown = show_text(&mut workspace, "Demo").unwrap();

        assert!(shown.contains("#idea"), "{shown}");
        assert!(!shown.contains("locked"), "{shown}");
        let id = NoteId::parse("Demo").unwrap();
        assert!(!workspace.read_plain(&id).unwrap().contains("@agent-note"));
        assert_eq!(workspace.note_threads(&id).unwrap().len(), 1);
    }

    #[test]
    fn render_of_a_locked_note_fails_with_the_shared_error() {
        let (dir, mut workspace) = open(&[("Diary.md", "# Diary\n")]);
        let weak = KdfParams {
            memory_kib: 64,
            passes: 1,
            lanes: 1,
        };
        workspace
            .set_lock_password_with("correct horse", weak)
            .unwrap();
        workspace
            .lock_note(&NoteId::parse("Diary").unwrap())
            .unwrap();
        let closed = Workspace::open(dir.path()).unwrap();

        let error = render(&closed, "Diary").unwrap_err();

        assert_eq!(error.to_string(), "the note Diary is locked");
    }
}
