//! A part of a note for a hover preview or an embed: the section of a
//! heading, the block of a `^id`, or the whole body.

use kknow_core::NoteId;
use kknow_index::{EmbedSource, ParsedNote};

use crate::{Result, Workspace, subpath_line};

/// A rendered part of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// HTML without line marks; images name their note (`data-note`).
    pub html: String,
    /// 1-based line where the part starts; `None` for the whole note.
    pub line: Option<usize>,
}

impl Workspace {
    /// Renders the part of note `id` that `subpath` names: `Heading`,
    /// `Part#Heading` or `^block`, as after `#` in a link. Without a subpath,
    /// the whole note without its frontmatter.
    ///
    /// `Ok(None)`: the note does not exist, or it has no such heading or
    /// block. The text comes from the disk, as the other notes of a preview
    /// are not open.
    pub fn render_section(&self, id: &NoteId, subpath: Option<&str>) -> Result<Option<Section>> {
        let Some(parsed) = self.parsed(id) else {
            return Ok(None);
        };
        let source = self.read(id)?;
        let subpath = subpath.map(str::trim).filter(|sub| !sub.is_empty());
        let Some((text, line)) = section_text(&source, parsed, subpath) else {
            return Ok(None);
        };
        Ok(Some(Section {
            html: kknow_index::fragment_to_html_with_embeds(&text, id, &self.resolver, self),
            line,
        }))
    }
}

/// `![[note#Heading]]` reads the section from the disk, the same part that a
/// hover preview shows.
impl EmbedSource for Workspace {
    fn embed_text(&self, id: &NoteId, subpath: Option<&str>) -> Option<String> {
        let parsed = self.parsed(id)?;
        let source = self.read(id).ok()?;
        section_text(&source, parsed, subpath).map(|(text, _)| text)
    }
}

/// The text of the part and the line where it starts.
fn section_text(
    source: &str,
    parsed: &ParsedNote,
    subpath: Option<&str>,
) -> Option<(String, Option<usize>)> {
    let Some(sub) = subpath else {
        let body = parsed.frontmatter.as_ref().map_or(0, |f| f.span.end);
        return Some((source[body..].to_string(), None));
    };
    let start = subpath_line(parsed, sub)?;
    let lines: Vec<&str> = source.split_inclusive('\n').collect();

    let end = match parsed.headings.iter().find(|h| h.line == start) {
        // A heading: up to the next heading of the same or a higher level.
        Some(heading) => parsed
            .headings
            .iter()
            .find(|next| next.line > start && next.level <= heading.level)
            .map_or(lines.len(), |next| next.line - 1),
        // A block: its paragraph or list item, the lines around it up to an
        // empty line or a heading.
        None => {
            let blank = |line: usize| lines.get(line - 1).is_none_or(|text| text.trim().is_empty());
            let heading = |line: usize| parsed.headings.iter().any(|h| h.line == line);
            let mut first = start;
            while first > 1 && !blank(first - 1) && !heading(first - 1) {
                first -= 1;
            }
            let mut last = start;
            while last < lines.len() && !blank(last + 1) && !heading(last + 1) {
                last += 1;
            }
            return Some((lines[first - 1..last].concat(), Some(first)));
        }
    };
    Some((lines[start - 1..end.max(start)].concat(), Some(start)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const PLAN: &str = "---\naliases: [Roadmap]\n---\n# Plan\n\nIntro.\n\n## Goals\n\nShip it.\n\n### Risks\n\nLate. ^late\nStill late.\n\n## Done\n\nNothing.\n";

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Plan.md"), PLAN).unwrap();
        fs::write(dir.path().join("Setext.md"), "Title\n=====\n\ntext\n\nPart\n----\n\nmore\n").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn section(workspace: &Workspace, id: &str, sub: Option<&str>) -> Option<Section> {
        workspace
            .render_section(&NoteId::parse(id).unwrap(), sub)
            .unwrap()
    }

    #[test]
    fn a_heading_section_keeps_its_subheadings_and_stops_at_its_level() {
        let (_dir, workspace) = workspace();
        let goals = section(&workspace, "Plan", Some("Goals")).unwrap();

        assert_eq!(goals.line, Some(8));
        assert!(goals.html.contains("Ship it."), "{}", goals.html);
        assert!(goals.html.contains("Risks"), "{}", goals.html);
        assert!(!goals.html.contains("Done"), "{}", goals.html);
        assert!(!goals.html.contains("data-line"), "{}", goals.html);
    }

    #[test]
    fn the_last_section_goes_to_the_end() {
        let (_dir, workspace) = workspace();
        let done = section(&workspace, "Plan", Some("Plan#Done")).unwrap();
        assert!(done.html.contains("Nothing."), "{}", done.html);
        assert!(!done.html.contains("Ship it"), "{}", done.html);
    }

    #[test]
    fn a_block_is_its_paragraph() {
        let (_dir, workspace) = workspace();
        let block = section(&workspace, "Plan", Some("^late")).unwrap();

        assert_eq!(block.line, Some(14));
        assert!(block.html.contains("Still late."), "{}", block.html);
        assert!(!block.html.contains("Risks"), "{}", block.html);
    }

    #[test]
    fn the_whole_note_has_no_frontmatter() {
        let (_dir, workspace) = workspace();
        let whole = section(&workspace, "Plan", None).unwrap();

        assert_eq!(whole.line, None);
        assert!(whole.html.contains("Nothing."), "{}", whole.html);
        assert!(!whole.html.contains("Roadmap"), "{}", whole.html);
    }

    #[test]
    fn setext_headings_make_sections_too() {
        let (_dir, workspace) = workspace();
        let part = section(&workspace, "Setext", Some("Part")).unwrap();
        assert!(part.html.contains("more"), "{}", part.html);
        assert!(!part.html.contains("text</p>"), "{}", part.html);
    }

    #[test]
    fn the_preview_shows_embeds_and_a_section_preview_too() {
        let (dir, mut workspace) = workspace();
        fs::write(dir.path().join("Home.md"), "# Home\n\n![[Plan#Risks]]\n").unwrap();
        workspace.save(&NoteId::parse("Home").unwrap(), "# Home\n\n![[Plan#Risks]]\n").unwrap();
        let home = NoteId::parse("Home").unwrap();

        let html = workspace.render(&home, "# Home\n\n![[Plan#Risks]]\n");
        assert!(html.contains(r#"class="embed" data-note="Plan" data-sub="Risks""#), "{html}");
        assert!(html.contains("Still late."), "{html}");
        assert!(!html.contains("Ship it."), "{html}");

        let section = section(&workspace, "Home", None).unwrap();
        assert!(section.html.contains("Still late."), "{}", section.html);
    }

    #[test]
    fn a_missing_note_or_heading_is_none() {
        let (_dir, workspace) = workspace();
        assert!(section(&workspace, "Plan", Some("Ghost")).is_none());
        assert!(section(&workspace, "Ghost", None).is_none());
    }
}
