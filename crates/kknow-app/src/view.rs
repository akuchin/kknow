use kknow_core::NoteId;
use kknow_index::Edge;
use serde::Serialize;

use crate::bookmarks::Mark;
use crate::proposals::ProposalView;
use crate::threads::Thread;
use crate::{Error, Result, Workspace};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LinkRef {
    pub id: String,
    pub title: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NoteView {
    pub id: String,
    pub title: String,
    pub content: String,
    pub html: String,
    pub backlinks: Vec<LinkRef>,
    pub outgoing: Vec<LinkRef>,
    pub tags: Vec<String>,
    pub bookmarks: Vec<Mark>,
    pub threads: Vec<Thread>,
    pub proposals: Vec<ProposalView>,
    pub locked: bool,
    pub closed: bool,
}

impl NoteView {
    pub fn has_agent_answers(&self) -> bool {
        !self.locked && !kknow_index::parse::agent_answers(&self.content).is_empty()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LinkPreview {
    pub id: String,
    pub title: String,
    pub html: String,
    pub line: Option<usize>,
    pub part_missing: bool,
    pub locked: bool,
}

impl Workspace {
    pub fn note_title(&self, id: &NoteId) -> String {
        self.parsed(id)
            .and_then(|parsed| parsed.title())
            .unwrap_or_else(|| id.name())
            .to_string()
    }

    pub fn link_ref(&self, id: &NoteId, label: Option<&String>) -> LinkRef {
        LinkRef {
            id: id.as_str().to_string(),
            title: self.note_title(id),
            label: label.cloned(),
        }
    }

    pub fn link_refs(&self, edges: &[Edge], pick: impl Fn(&Edge) -> &NoteId) -> Vec<LinkRef> {
        let mut seen: Vec<&NoteId> = Vec::new();
        let mut refs = Vec::new();

        for edge in edges {
            let id = pick(edge);
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            refs.push(self.link_ref(id, edge.label.as_ref()));
        }

        refs
    }

    pub fn note_view(&self, id: &NoteId, content: String) -> NoteView {
        let parsed = self.parsed(id);
        let bookmarks = self.note_bookmarks(id, &content);
        let locked = self.is_locked(id);

        NoteView {
            locked,
            closed: false,
            id: id.as_str().to_string(),
            title: self.note_title(id),
            html: self.render(id, &content),
            content,
            backlinks: self.link_refs(self.backlinks(id), |edge| &edge.from),
            outgoing: self.link_refs(self.outgoing(id), |edge| &edge.to),
            tags: parsed
                .map(|parsed| parsed.tags.iter().map(|tag| tag.name.clone()).collect())
                .unwrap_or_default(),
            bookmarks,
            threads: if locked {
                Vec::new()
            } else {
                self.note_threads(id).unwrap_or_default()
            },
            proposals: if locked {
                Vec::new()
            } else {
                self.note_proposals(id).unwrap_or_default()
            },
        }
    }

    pub fn open_view(&self, id: &NoteId) -> Result<NoteView> {
        match self.open_text(id) {
            Ok(content) => Ok(self.note_view(id, content)),
            Err(Error::NotesClosed) => Ok(NoteView {
                closed: true,
                html: String::new(),
                ..self.note_view(id, String::new())
            }),
            Err(error) => Err(error),
        }
    }

    pub fn load_note(&mut self, id: &NoteId) -> Result<NoteView> {
        let view = self.open_view(id)?;
        if !view.has_agent_answers() {
            return Ok(view);
        }
        self.adopt_agent_answers(id)?;
        let content = self.read(id)?;
        Ok(self.note_view(id, content))
    }

    pub fn link_preview(&self, from: &NoteId, target: &str) -> Result<Option<LinkPreview>> {
        let Some(found) = self.link_target(from, target) else {
            return Ok(None);
        };
        let id = found.id;
        if self.is_locked(&id) {
            return Ok(Some(LinkPreview {
                id: id.as_str().to_string(),
                title: self.note_title(&id),
                html: String::new(),
                line: None,
                part_missing: false,
                locked: true,
            }));
        }
        let subpath = target
            .split('|')
            .next()
            .and_then(|link| link.split_once('#'))
            .map(|(_, sub)| sub.trim())
            .filter(|sub| !sub.is_empty());
        let (section, part_missing) = match self.render_section(&id, subpath)? {
            Some(section) => (section, false),
            None => match self.render_section(&id, None)? {
                Some(whole) => (whole, subpath.is_some()),
                None => return Ok(None),
            },
        };
        Ok(Some(LinkPreview {
            id: id.as_str().to_string(),
            title: self.note_title(&id),
            html: section.html,
            line: section.line,
            part_missing,
            locked: false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kknow_crypto::KdfParams;
    use std::fs;

    const WEAK: KdfParams = KdfParams {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn locked_workspace() -> (tempfile::TempDir, Workspace) {
        let (dir, mut workspace) = workspace(&[
            ("Diary.md", "# Diary\n\nThe zebra. #health\n"),
            ("Anna.md", "# Anna\n\nSee [[Diary]].\n"),
        ]);
        workspace
            .set_lock_password_with("correct horse", WEAK)
            .unwrap();
        workspace.lock_note(&id("Diary")).unwrap();
        (dir, workspace)
    }

    #[test]
    fn a_locked_note_has_no_threads_or_proposals() {
        let (_dir, workspace) = locked_workspace();
        let view = workspace.note_view(&id("Diary"), "# Diary\n".to_string());
        assert!(view.locked);
        assert!(view.threads.is_empty());
        assert!(view.proposals.is_empty());
    }

    #[test]
    fn a_closed_lock_gives_a_closed_view_without_html() {
        let (_dir, mut workspace) = locked_workspace();
        workspace.lock_all();
        let view = workspace.open_view(&id("Diary")).unwrap();
        assert!(view.closed);
        assert!(view.locked);
        assert!(view.html.is_empty());
    }

    #[test]
    fn load_note_moves_an_agent_answer_into_the_thread() {
        let text = "Demo zero<!-- @agent «zero»: start for one -->\n<!-- @agent-note: VARIANTS: 1) a 2) b -->\nDemo one\n";
        let (_dir, mut workspace) = workspace(&[("Demo.md", text)]);
        let view = workspace.load_note(&id("Demo")).unwrap();
        assert!(!view.content.contains("@agent-note"));
        assert_eq!(view.threads.len(), 1);
        assert_eq!(view.threads[0].messages.len(), 2);
    }

    #[test]
    fn load_note_leaves_a_locked_note_alone() {
        let (_dir, mut workspace) = locked_workspace();
        let view = workspace.load_note(&id("Diary")).unwrap();
        assert!(view.locked);
        assert!(!view.closed);
        assert!(view.threads.is_empty());
    }

    #[test]
    fn link_preview_follows_a_heading_and_reports_a_missing_one() {
        let (_dir, workspace) = workspace(&[
            ("Index.md", "# Index\n"),
            ("Topic.md", "# Topic\n\nintro\n\n## Part\n\nbody text\n"),
        ]);
        let found = workspace
            .link_preview(&id("Index"), "Topic#Part")
            .unwrap()
            .unwrap();
        assert!(!found.part_missing);
        assert!(found.html.contains("body text"));
        let missing = workspace
            .link_preview(&id("Index"), "Topic#Nothing | alias")
            .unwrap()
            .unwrap();
        assert!(missing.part_missing);
        assert!(missing.html.contains("intro"));
        assert!(
            workspace
                .link_preview(&id("Index"), "Nowhere")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn link_preview_of_a_locked_note_shows_nothing() {
        let (_dir, workspace) = locked_workspace();
        let preview = workspace
            .link_preview(&id("Anna"), "Diary")
            .unwrap()
            .unwrap();
        assert!(preview.locked);
        assert!(preview.html.is_empty());
    }
}
