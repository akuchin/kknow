use std::collections::BTreeSet;
use std::ops::Range;

use kknow_core::NoteId;

use crate::lines::LineContext;
use crate::{Error, Result, Workspace};

const CANDIDATES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub from: NoteId,
    pub span: Range<usize>,
    pub context: LineContext,
}

impl Workspace {
    pub fn names_of(&self, id: &NoteId) -> Vec<String> {
        let mut seen = BTreeSet::new();
        std::iter::once(id.name().to_string())
            .chain(
                self.parsed(id)
                    .into_iter()
                    .flat_map(|p| p.aliases.iter().cloned()),
            )
            .filter(|name| !name.trim().is_empty() && seen.insert(name.to_lowercase()))
            .collect()
    }

    pub fn unlinked_mentions(&self, id: &NoteId) -> Result<Vec<Mention>> {
        let names = self.names_of(id);
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        let mut candidates = BTreeSet::new();
        for name in &names {
            candidates.extend(self.search.containing(name, CANDIDATES)?);
        }
        candidates.remove(id);

        let mut mentions = Vec::new();
        for from in candidates {
            let Ok(source) = self.read_plain(&from) else {
                continue;
            };
            for span in kknow_index::mention::find(&source, &refs) {
                if let Some(context) = LineContext::around(&source, &span) {
                    mentions.push(Mention {
                        from: from.clone(),
                        span,
                        context,
                    });
                }
            }
        }
        Ok(mentions)
    }

    pub fn link_mention(
        &mut self,
        from: &NoteId,
        span: Range<usize>,
        text: &str,
        target: &NoteId,
    ) -> Result<String> {
        let source = self.read_plain(from)?;
        let names = self.names_of(target);
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let line = source
            .get(..span.start)
            .map_or(0, |head| head.matches('\n').count() + 1);
        let still = source.get(span.clone()) == Some(text)
            && kknow_index::mention::find(&source, &refs).contains(&span);
        if !still || from == target {
            return Err(Error::NotAMention(from.clone(), line));
        }

        let link = self.link_text(from, target);
        let written = if link == text {
            format!("[[{link}]]")
        } else {
            format!("[[{link}|{text}]]")
        };
        let changed = format!("{}{written}{}", &source[..span.start], &source[span.end..]);
        self.save(from, &changed)?;
        Ok(changed)
    }

    fn link_text(&self, from: &NoteId, target: &NoteId) -> String {
        let name = target.name();
        if self.resolve(name, from).as_ref() == Some(target) {
            name.to_string()
        } else {
            target.as_str().to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn workspace(notes: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in notes {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    #[test]
    fn mentions_use_the_name_and_the_aliases() {
        let (_dir, workspace) = workspace(&[
            (
                "Release plan.md",
                "---\naliases: [Roadmap]\n---\n# Release plan\n",
            ),
            ("Notes.md", "the release plan is late\n\nsee the roadmap\n"),
            ("Linked.md", "[[Release plan]] only\n"),
            ("Other.md", "nothing to see\n"),
        ]);
        let target = id("Release plan");

        assert_eq!(workspace.names_of(&target), ["Release plan", "Roadmap"]);
        let mentions = workspace.unlinked_mentions(&target).unwrap();
        let rows: Vec<(&str, usize, &str)> = mentions
            .iter()
            .map(|m| (m.from.as_str(), m.context.line, m.context.text.as_str()))
            .collect();
        assert_eq!(
            rows,
            [("Notes", 1, "release plan"), ("Notes", 3, "roadmap")]
        );
    }

    #[test]
    fn a_note_does_not_mention_itself() {
        let (_dir, workspace) = workspace(&[("Plan.md", "# Plan\n\nthe Plan\n")]);
        assert!(workspace.unlinked_mentions(&id("Plan")).unwrap().is_empty());
    }

    #[test]
    fn linking_a_mention_writes_an_alias_when_the_text_differs() {
        let (dir, mut workspace) = workspace(&[
            ("Projects/Plan.md", "# Plan\n"),
            ("Notes.md", "the plan and the Plan\n"),
        ]);
        let target = id("Projects/Plan");
        let mentions = workspace.unlinked_mentions(&target).unwrap();
        assert_eq!(mentions.len(), 2);

        let second = mentions[1].clone();
        workspace
            .link_mention(&second.from, second.span, &second.context.text, &target)
            .unwrap();
        let first = mentions[0].clone();
        let text = workspace
            .link_mention(&first.from, first.span, &first.context.text, &target)
            .unwrap();

        assert_eq!(text, "the [[Plan|plan]] and the [[Plan]]\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("Notes.md")).unwrap(),
            text
        );
        assert_eq!(workspace.backlinks(&target).len(), 2);
        assert!(workspace.unlinked_mentions(&target).unwrap().is_empty());
    }

    #[test]
    fn a_name_that_finds_another_note_links_by_path() {
        let (_dir, mut workspace) = workspace(&[
            ("A/Plan.md", "# Plan A\n"),
            ("B/Plan.md", "# Plan B\n"),
            ("B/Notes.md", "about the Plan\n"),
        ]);
        let target = id("A/Plan");
        let mention = workspace.unlinked_mentions(&target).unwrap()[0].clone();

        let text = workspace
            .link_mention(&mention.from, mention.span, "Plan", &target)
            .unwrap();
        assert_eq!(text, "about the [[A/Plan|Plan]]\n");
    }

    #[test]
    fn a_stale_mention_is_an_error() {
        let (dir, mut workspace) =
            workspace(&[("Plan.md", "# Plan\n"), ("Notes.md", "the Plan\n")]);
        let target = id("Plan");
        let mention = workspace.unlinked_mentions(&target).unwrap()[0].clone();
        fs::write(dir.path().join("Notes.md"), "now [[Plan]]\n").unwrap();

        let error = workspace
            .link_mention(&mention.from, mention.span, "Plan", &target)
            .unwrap_err();
        assert!(matches!(error, Error::NotAMention(_, 1)), "{error}");
    }
}
