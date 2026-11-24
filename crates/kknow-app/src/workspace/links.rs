use std::path::PathBuf;

use kknow_core::NoteId;
use kknow_index::Edge;

use crate::lines::LineContext;
use crate::*;

impl Workspace {
    pub fn backlinks(&self, id: &NoteId) -> &[Edge] {
        self.graph.backlinks(id)
    }

    pub fn backlink_lines(&self, id: &NoteId) -> Vec<BacklinkLine<'_>> {
        let mut edges: Vec<&Edge> = self.backlinks(id).iter().collect();
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.span.start.cmp(&b.span.start)));

        let mut lines = Vec::with_capacity(edges.len());
        let mut source: Option<(&NoteId, String)> = None;
        for edge in edges {
            if source.as_ref().is_none_or(|(from, _)| *from != &edge.from) {
                source = Some((&edge.from, self.read_plain(&edge.from).unwrap_or_default()));
            }
            let Some((_, text)) = &source else { continue };
            if let Some(context) = LineContext::around(text, &edge.span) {
                lines.push(BacklinkLine { edge, context });
            }
        }
        lines
    }

    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.graph.outgoing(id)
    }

    pub fn render(&self, id: &NoteId, content: &str) -> String {
        kknow_index::to_html_with_embeds(content, id, &self.resolver, self)
    }

    pub fn image(&self, from: &NoteId, target: &str) -> Option<PathBuf> {
        let note = self.path_for(from);
        let dir = note.parent().unwrap_or(self.root());
        kknow_core::image::resolve(self.root(), dir, target)
    }

    pub fn resolve(&self, target: &str, from: &NoteId) -> Option<NoteId> {
        self.resolver.resolve(target, from)
    }

    pub fn link_target(&self, from: &NoteId, link: &str) -> Option<LinkTarget<'_>> {
        let link = link.split('|').next().unwrap_or_default();
        let (target, subpath) = match link.split_once('#') {
            Some((target, subpath)) => (target, Some(subpath.trim())),
            None => (link, None),
        };
        let id = if target.trim().is_empty() {
            self.parsed.get_key_value(from)?.0.clone()
        } else {
            self.resolve(target, from)?
        };
        let parsed = self.parsed.get(&id)?;
        let line = subpath
            .filter(|sub| !sub.is_empty())
            .and_then(|sub| subpath_line(parsed, sub));
        Some(LinkTarget {
            id,
            line,
            headings: &parsed.headings,
            blocks: &parsed.blocks,
        })
    }
}
