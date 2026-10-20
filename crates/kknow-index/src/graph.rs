use std::collections::{BTreeMap, BTreeSet, HashMap};

use kknow_core::{NoteId, is_markdown_extension};

use crate::parse::ParsedNote;
use crate::wikilink::{Subpath, WikiKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// `[[note]]`
    Wiki,
    /// `![[note]]`
    Embed,
    /// `[text](note.md)`
    Markdown,
}

/// A link from one note to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NoteId,
    pub to: NoteId,
    pub kind: EdgeKind,
    /// Alias or link text, if it differs from the target.
    pub label: Option<String>,
    pub subpath: Option<Subpath>,
}

/// A link with no matching note. The UI usually shows it grey, and a click
/// creates the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    pub from: NoteId,
    pub target: String,
    pub kind: EdgeKind,
}

/// Link graph of the whole vault.
#[derive(Debug, Default)]
pub struct LinkGraph {
    notes: BTreeSet<NoteId>,
    outgoing: BTreeMap<NoteId, Vec<Edge>>,
    incoming: BTreeMap<NoteId, Vec<Edge>>,
    broken: Vec<BrokenLink>,
}

impl LinkGraph {
    /// Link resolution needs the full note list, so the graph is built at
    /// once, not one note at a time.
    pub fn build<'a>(notes: impl IntoIterator<Item = (NoteId, &'a ParsedNote)>) -> Self {
        let notes: Vec<(NoteId, &ParsedNote)> = notes.into_iter().collect();
        let resolver = Resolver::new(notes.iter().map(|(id, _)| id));

        let mut graph = LinkGraph {
            notes: notes.iter().map(|(id, _)| id.clone()).collect(),
            ..Default::default()
        };

        for (from, parsed) in &notes {
            for link in &parsed.wiki_links {
                if link.is_internal() {
                    continue;
                }
                let kind = match link.kind {
                    WikiKind::Link => EdgeKind::Wiki,
                    WikiKind::Embed => EdgeKind::Embed,
                };
                graph.add(
                    &resolver,
                    from,
                    &link.target,
                    kind,
                    link.alias.clone(),
                    link.subpath.clone(),
                );
            }

            for link in &parsed.markdown_links {
                let label = (!link.label.is_empty()).then(|| link.label.clone());
                graph.add(
                    &resolver,
                    from,
                    &link.target,
                    EdgeKind::Markdown,
                    label,
                    None,
                );
            }
        }

        graph
    }

    fn add(
        &mut self,
        resolver: &Resolver,
        from: &NoteId,
        target: &str,
        kind: EdgeKind,
        label: Option<String>,
        subpath: Option<Subpath>,
    ) {
        match resolver.resolve(target, from) {
            Some(to) => {
                let edge = Edge {
                    from: from.clone(),
                    to: to.clone(),
                    kind,
                    label,
                    subpath,
                };
                self.outgoing
                    .entry(from.clone())
                    .or_default()
                    .push(edge.clone());
                self.incoming.entry(to).or_default().push(edge);
            }
            None => self.broken.push(BrokenLink {
                from: from.clone(),
                target: target.to_string(),
                kind,
            }),
        }
    }

    /// Outgoing links in the order they appear in the text.
    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.outgoing.get(id).map_or(&[], Vec::as_slice)
    }

    pub fn backlinks(&self, id: &NoteId) -> &[Edge] {
        self.incoming.get(id).map_or(&[], Vec::as_slice)
    }

    pub fn broken(&self) -> &[BrokenLink] {
        &self.broken
    }

    /// Notes that no note links to.
    pub fn orphans(&self) -> impl Iterator<Item = &NoteId> {
        self.notes
            .iter()
            .filter(|id| !self.incoming.contains_key(*id))
    }

    pub fn edge_count(&self) -> usize {
        self.outgoing.values().map(Vec::len).sum()
    }

    pub fn note_count(&self) -> usize {
        self.notes.len()
    }
}

/// Matches link text to a vault note.
pub struct Resolver {
    by_path: HashMap<String, NoteId>,
    by_name: HashMap<String, Vec<NoteId>>,
}

impl Resolver {
    pub fn new<'a>(ids: impl IntoIterator<Item = &'a NoteId>) -> Self {
        let mut by_path = HashMap::new();
        let mut by_name: HashMap<String, Vec<NoteId>> = HashMap::new();

        for id in ids {
            by_path.insert(id.as_str().to_lowercase(), id.clone());
            by_name
                .entry(id.name().to_lowercase())
                .or_default()
                .push(id.clone());
        }

        Resolver { by_path, by_name }
    }

    /// Standard wikilink rules: full path first, then file name, then path
    /// suffix. Of several candidates, the one nearest to the linking note wins.
    pub fn resolve(&self, target: &str, from: &NoteId) -> Option<NoteId> {
        let target = normalize(target);
        if target.is_empty() {
            return None;
        }

        if let Some(id) = self.by_path.get(&target) {
            return Some(id.clone());
        }

        if let Some(candidates) = self.by_name.get(&target) {
            return closest(candidates, from);
        }

        // Partial path `Rust/kknow` for the note `Projects/Rust/kknow`.
        // Find candidates by file name, not by a scan of the whole vault:
        // a link with no match must not cost a walk over all notes.
        let name = target.rsplit('/').next()?;
        let suffix = format!("/{target}");
        let matches: Vec<NoteId> = self
            .by_name
            .get(name)?
            .iter()
            .filter(|id| id.as_str().to_lowercase().ends_with(&suffix))
            .cloned()
            .collect();
        closest(&matches, from)
    }
}

/// Converts a link target to note id form.
fn normalize(target: &str) -> String {
    let target = target.trim().replace('\\', "/");
    let target = target.trim_start_matches("./").trim_matches('/');

    let without_extension = match target.rsplit_once('.') {
        Some((stem, ext)) if is_markdown_extension(ext) && !stem.is_empty() => stem,
        _ => target,
    };

    without_extension.to_lowercase()
}

/// Of several notes with the same name, picks the one nearest to `from`:
/// longest common path first, then the least deep, then alphabetical order.
fn closest(candidates: &[NoteId], from: &NoteId) -> Option<NoteId> {
    candidates
        .iter()
        .max_by_key(|candidate| {
            let shared = shared_folders(candidate, from);
            let depth = candidate.as_str().matches('/').count();
            (
                shared,
                std::cmp::Reverse(depth),
                std::cmp::Reverse(candidate.as_str()),
            )
        })
        .cloned()
}

fn shared_folders(a: &NoteId, b: &NoteId) -> usize {
    let a = a.folder().unwrap_or_default().split('/');
    let b = b.folder().unwrap_or_default().split('/');
    a.zip(b).take_while(|(a, b)| a == b).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn graph(notes: &[(&str, &str)]) -> LinkGraph {
        let parsed: Vec<(NoteId, ParsedNote)> = notes
            .iter()
            .map(|(id, source)| (NoteId::parse(id).unwrap(), parse(source)))
            .collect();
        LinkGraph::build(parsed.iter().map(|(id, note)| (id.clone(), note)))
    }

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    #[test]
    fn resolves_short_link_by_file_name() {
        let graph = graph(&[
            ("Index", "смотри [[kknow]]"),
            ("Projects/Rust/kknow", "# kknow"),
        ]);

        let backlinks = graph.backlinks(&id("Projects/Rust/kknow"));
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].from, id("Index"));
    }

    #[test]
    fn resolves_full_path_and_ignores_extension_and_case() {
        let graph = graph(&[
            ("Index", "[[Projects/Rust/KKNOW.md]]"),
            ("Projects/Rust/kknow", "# kknow"),
        ]);

        assert_eq!(
            graph.outgoing(&id("Index"))[0].to,
            id("Projects/Rust/kknow")
        );
        assert!(graph.broken().is_empty());
    }

    #[test]
    fn resolves_partial_path_suffix() {
        let graph = graph(&[
            ("Index", "[[Rust/kknow]]"),
            ("Projects/Rust/kknow", "# kknow"),
        ]);

        assert_eq!(
            graph.outgoing(&id("Index"))[0].to,
            id("Projects/Rust/kknow")
        );
    }

    #[test]
    fn prefers_the_note_closest_to_the_source() {
        let graph = graph(&[
            ("Work/Daily/Monday", "[[Notes]]"),
            ("Work/Daily/Notes", "рядом"),
            ("Personal/Notes", "далеко"),
        ]);

        assert_eq!(
            graph.outgoing(&id("Work/Daily/Monday"))[0].to,
            id("Work/Daily/Notes")
        );
    }

    #[test]
    fn keeps_alias_and_subpath_on_the_edge() {
        let graph = graph(&[("A", "[[B#Установка|как ставить]]"), ("B", "# B")]);

        let edge = &graph.outgoing(&id("A"))[0];
        assert_eq!(edge.label.as_deref(), Some("как ставить"));
        assert_eq!(edge.subpath, Some(Subpath::Heading("Установка".into())));
    }

    #[test]
    fn embeds_and_markdown_links_are_separate_kinds() {
        let graph = graph(&[("A", "![[B]] и [текст](B.md)"), ("B", "# B")]);

        let kinds: Vec<EdgeKind> = graph.outgoing(&id("A")).iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [EdgeKind::Embed, EdgeKind::Markdown]);
    }

    #[test]
    fn unresolved_links_are_reported_not_dropped() {
        let graph = graph(&[("A", "[[Призрак]]")]);

        assert_eq!(graph.broken().len(), 1);
        assert_eq!(graph.broken()[0].target, "Призрак");
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn internal_links_do_not_create_edges() {
        let graph = graph(&[("A", "[[#Раздел]]\n\n## Раздел")]);

        assert_eq!(graph.edge_count(), 0);
        assert!(graph.broken().is_empty());
    }

    #[test]
    fn orphans_are_notes_without_backlinks() {
        let graph = graph(&[("A", "[[B]]"), ("B", "# B"), ("C", "# C")]);

        let orphans: Vec<&str> = graph.orphans().map(NoteId::as_str).collect();
        assert_eq!(orphans, ["A", "C"]);
    }
}
