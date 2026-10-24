//! Link rewrite after notes move.
//!
//! The rewrite changes only the target part of a link. The alias, the heading
//! and the block stay as written.

use std::collections::HashMap;
use std::ops::Range;

use kknow_core::{NoteId, is_markdown_extension};

use crate::graph::Resolver;
use crate::parse::{ParsedNote, decode_target};
use crate::wikilink::{WikiKind, WikiLink};

/// Rewrites links so that they point to the same notes after a move.
///
/// `before` knows the old note ids, `after` the new ones. `moves` maps each
/// old id to its new id.
pub struct Relink<'a> {
    pub before: &'a Resolver,
    pub after: &'a Resolver,
    pub moves: &'a HashMap<NoteId, NoteId>,
}

impl Relink<'_> {
    /// Returns the new text and the number of changed links. `None` means
    /// that no link changed.
    ///
    /// `from` is the old id of the note, `parsed` the parse of `source`.
    pub fn apply(
        &self,
        from: &NoteId,
        source: &str,
        parsed: &ParsedNote,
    ) -> Option<(String, usize)> {
        let from_after = self.moves.get(from).unwrap_or(from);
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();

        for link in parsed.wiki_links.iter().filter(|link| !link.is_internal()) {
            let range = wiki_target(source, link);
            if let Some(text) = self.retarget(&source[range.clone()], from, from_after) {
                edits.push((range, text));
            }
        }

        for link in &parsed.markdown_links {
            let Some((range, angle)) = markdown_path(source, link.span.clone()) else {
                continue;
            };
            let target = decode_target(&source[range.clone()]);
            if let Some(text) = self.retarget(&target, from, from_after) {
                // A bare destination cannot hold a space.
                let text = if angle {
                    text
                } else {
                    text.replace(' ', "%20")
                };
                edits.push((range, text));
            }
        }

        if edits.is_empty() {
            return None;
        }

        edits.sort_by_key(|(range, _)| range.start);
        let mut out = String::with_capacity(source.len());
        let mut cursor = 0;
        for (range, text) in &edits {
            out.push_str(&source[cursor..range.start]);
            out.push_str(text);
            cursor = range.end;
        }
        out.push_str(&source[cursor..]);
        Some((out, edits.len()))
    }

    /// New target text, if the link would break or point to another note.
    ///
    /// A link by name stays a link by name while the name finds the note.
    /// Else the link gets the full path. A written extension stays.
    fn retarget(&self, target: &str, from: &NoteId, from_after: &NoteId) -> Option<String> {
        let old = self.before.resolve(target, from)?;
        let expected = self.moves.get(&old).unwrap_or(&old);
        if self.after.resolve(target, from_after).as_ref() == Some(expected) {
            return None;
        }

        let by_name = !target.contains(['/', '\\'])
            && self.after.resolve(expected.name(), from_after).as_ref() == Some(expected);
        let mut text = if by_name {
            expected.name()
        } else {
            expected.as_str()
        }
        .to_string();

        if let Some((_, ext)) = target.rsplit_once('.')
            && is_markdown_extension(ext)
        {
            text.push('.');
            text.push_str(ext);
        }
        Some(text)
    }
}

/// Range of the target in a wikilink: the text before `|` or `#`, trimmed.
fn wiki_target(source: &str, link: &WikiLink) -> Range<usize> {
    let open = link.span.start
        + match link.kind {
            WikiKind::Link => 2,
            WikiKind::Embed => 3,
        };
    let inner = &source[open..link.span.end - 2];
    let raw = &inner[..inner.find(['|', '#']).unwrap_or(inner.len())];
    let start = open + raw.len() - raw.trim_start().len();
    start..start + raw.trim().len()
}

/// Range of the path in `[text](path#anchor "title")`, and whether the
/// destination is in angle brackets. `None` for a reference link
/// `[text][ref]`: its destination is in a different place.
fn markdown_path(source: &str, span: Range<usize>) -> Option<(Range<usize>, bool)> {
    let body = source[span.clone()].strip_suffix(')')?;
    let open = body.rfind("](")? + 2;
    let rest = &body[open..];
    let start = span.start + open + rest.len() - rest.trim_start().len();
    let rest = rest.trim_start();

    let (start, dest, angle) = match rest.strip_prefix('<') {
        Some(inner) => (start + 1, &inner[..inner.find('>')?], true),
        None => (
            start,
            &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())],
            false,
        ),
    };
    let path = &dest[..dest.find('#').unwrap_or(dest.len())];
    (!path.is_empty()).then_some((start..start + path.len(), angle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    /// Moves notes in a vault of `ids` and rewrites `source`, written in the
    /// note `from`.
    fn relink(ids: &[&str], moves: &[(&str, &str)], from: &str, source: &str) -> String {
        let before_ids: Vec<NoteId> = ids.iter().map(|raw| id(raw)).collect();
        let moves: HashMap<NoteId, NoteId> =
            moves.iter().map(|(old, new)| (id(old), id(new))).collect();
        let after_ids: Vec<NoteId> = before_ids
            .iter()
            .map(|old| moves.get(old).unwrap_or(old).clone())
            .collect();
        let before = Resolver::new(&before_ids);
        let after = Resolver::new(&after_ids);
        let relink = Relink {
            before: &before,
            after: &after,
            moves: &moves,
        };

        relink
            .apply(&id(from), source, &parse(source))
            .map_or_else(|| source.to_string(), |(text, _)| text)
    }

    const VAULT: [&str; 3] = ["Index", "Projects/kknow", "Projects/Rust"];
    const MOVE: [(&str, &str); 1] = [("Projects/kknow", "Архив/kknow-old")];

    #[test]
    fn rewrites_every_wikilink_form_and_keeps_its_parts() {
        let source = concat!(
            "[[Projects/kknow]] и [[Projects/kknow|мой проект]]\n",
            "[[Projects/kknow#Установка]] и [[ Projects/kknow #^a1b2|блок]]\n",
            "![[Projects/kknow]] и [[kknow]]\n",
        );

        assert_eq!(
            relink(&VAULT, &MOVE, "Index", source),
            concat!(
                "[[Архив/kknow-old]] и [[Архив/kknow-old|мой проект]]\n",
                "[[Архив/kknow-old#Установка]] и [[ Архив/kknow-old #^a1b2|блок]]\n",
                "![[Архив/kknow-old]] и [[kknow-old]]\n",
            )
        );
    }

    #[test]
    fn rewrites_markdown_links_in_their_encoding_style() {
        let moves = [("Projects/kknow", "Архив/Мой проект")];
        let source = concat!(
            "[раз](Projects/kknow.md) и [два](Projects/kknow.md#Установка \"заголовок\")\n",
            "[три](<Projects/kknow.md>) и [четыре](https://example.com/Projects/kknow.md)\n",
        );

        assert_eq!(
            relink(&VAULT, &moves, "Index", source),
            concat!(
                "[раз](Архив/Мой%20проект.md) и [два](Архив/Мой%20проект.md#Установка \"заголовок\")\n",
                "[три](<Архив/Мой проект.md>) и [четыре](https://example.com/Projects/kknow.md)\n",
            )
        );
    }

    #[test]
    fn a_short_name_that_still_finds_the_note_stays() {
        let source = "[[kknow]] и [[kknow.md|алиас]] и [о проекте](kknow.md)";

        assert_eq!(
            relink(
                &VAULT,
                &[("Projects/kknow", "Архив/kknow")],
                "Index",
                source
            ),
            source
        );
    }

    #[test]
    fn a_short_name_becomes_a_path_when_another_note_takes_the_name() {
        let ids = ["Work/Daily/Monday", "Work/Daily/Notes", "Work/Notes"];
        let moves = [("Work/Daily/Notes", "Archive/Notes")];

        // `Work/Notes` is now nearer than the moved note and wins.
        assert_eq!(
            relink(&ids, &moves, "Work/Daily/Monday", "[[Notes]]"),
            "[[Archive/Notes]]"
        );
    }

    #[test]
    fn links_inside_code_stay() {
        let source =
            "`[[Projects/kknow]]`\n\n```\n[[Projects/kknow]] [т](Projects/kknow.md)\n```\n";

        assert_eq!(relink(&VAULT, &MOVE, "Index", source), source);
    }

    #[test]
    fn links_to_other_and_missing_notes_stay() {
        let source = "[[Projects/Rust]] и [[Призрак]] и [[#Раздел]]";

        assert_eq!(relink(&VAULT, &MOVE, "Index", source), source);
    }

    #[test]
    fn a_moved_note_keeps_the_target_of_its_own_links() {
        let ids = ["A/note", "A/Notes", "B/Notes"];
        let moves = [("A/note", "B/note")];

        assert_eq!(relink(&ids, &moves, "A/note", "[[Notes]]"), "[[A/Notes]]");
    }

    #[test]
    fn counts_the_changed_links() {
        let ids: Vec<NoteId> = VAULT.iter().map(|raw| id(raw)).collect();
        let moves: HashMap<NoteId, NoteId> = [(id("Projects/kknow"), id("Архив/kknow"))].into();
        let after: Vec<NoteId> = ids
            .iter()
            .map(|old| moves.get(old).unwrap_or(old).clone())
            .collect();
        let (before, after) = (Resolver::new(&ids), Resolver::new(&after));
        let source = "[[Projects/kknow]] [[kknow]] [т](Projects/kknow.md)";

        let (_, count) = Relink {
            before: &before,
            after: &after,
            moves: &moves,
        }
        .apply(&id("Index"), source, &parse(source))
        .unwrap();

        assert_eq!(count, 2);
    }
}
