//! Unlinked mentions: places where a note name stands as plain text.
//!
//! A person writes "the release plan" and does not make it a link. The side
//! panel of the note `Release plan` shows such places, and one click makes
//! them `[[links]]`.
//!
//! Only plain text counts. A name in code, in an HTML block, in frontmatter,
//! in a link or in a tag is not a mention: a click must not change it. Text
//! between inline tags (`<b>Plan</b>`) is still text that a person reads, so
//! it counts.

use std::ops::Range;

use pulldown_cmark::{Event, Parser, Tag as MdTag, TagEnd};

use crate::parse::{options, parse};

/// The places in `source` where one of `names` stands as plain text, in text
/// order. Case does not matter; the place must start and end at a word
/// boundary. Of two names at the same place, the longer one wins.
pub fn find(source: &str, names: &[&str]) -> Vec<Range<usize>> {
    let mut wanted: Vec<Vec<char>> = names
        .iter()
        .map(|name| name.trim().chars().flat_map(char::to_lowercase).collect())
        .filter(|name: &Vec<char>| !name.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    wanted.sort_by_key(|name| std::cmp::Reverse(name.len()));

    let parsed = parse(source);
    let taken: Vec<Range<usize>> = parsed
        .wiki_links
        .iter()
        .map(|link| link.span.clone())
        .chain(parsed.tags.iter().map(|tag| tag.span.clone()))
        .collect();

    let mut found = Vec::new();
    for segment in text_segments(source, parsed.frontmatter.map_or(0, |f| f.span.end)) {
        let text = &source[segment.clone()];
        let mut at = 0;
        while at < text.len() {
            let hit = boundary_before(text, at)
                .then(|| wanted.iter().find_map(|name| match_at(text, at, name)))
                .flatten()
                .filter(|&end| boundary_after(text, end))
                .map(|end| segment.start + at..segment.start + end)
                .filter(|hit| !taken.iter().any(|span| overlaps(span, hit)));
            match hit {
                Some(hit) => {
                    at = hit.end - segment.start;
                    found.push(hit);
                }
                None => at += text[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
    }
    found
}

/// Ranges of plain text in the body, outside code, HTML, math and links.
/// Neighbour text events join into one range: the parser splits text at
/// special characters, and a name can contain one.
fn text_segments(source: &str, body_start: usize) -> Vec<Range<usize>> {
    let mut segments: Vec<Range<usize>> = Vec::new();
    // Inside a link, an image or a code block the text is not plain.
    let mut depth = 0usize;

    for (event, range) in Parser::new_ext(&source[body_start..], options()).into_offset_iter() {
        let range = range.start + body_start..range.end + body_start;
        match event {
            Event::Start(MdTag::Link { .. } | MdTag::Image { .. } | MdTag::CodeBlock(_)) => {
                depth += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => {
                depth = depth.saturating_sub(1);
            }
            Event::Text(_) if depth == 0 => match segments.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => segments.push(range),
            },
            _ => {}
        }
    }
    segments
}

/// The end of `name` when the text at `at` is the name in any case.
fn match_at(text: &str, at: usize, name: &[char]) -> Option<usize> {
    let mut want = name.iter().peekable();
    for (offset, c) in text[at..].char_indices() {
        for lower in c.to_lowercase() {
            if want.next() != Some(&lower) {
                return None;
            }
        }
        if want.peek().is_none() {
            return Some(at + offset + c.len_utf8());
        }
    }
    None
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn boundary_before(text: &str, at: usize) -> bool {
    !text[..at].chars().next_back().is_some_and(is_word)
}

fn boundary_after(text: &str, end: usize) -> bool {
    !text[end..].chars().next().is_some_and(is_word)
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found<'a>(source: &'a str, names: &[&str]) -> Vec<&'a str> {
        find(source, names)
            .into_iter()
            .map(|range| &source[range])
            .collect()
    }

    #[test]
    fn finds_a_name_in_any_case_at_word_boundaries() {
        let source = "The release plan is ready. Planning and plans are not. PLAN!";
        assert_eq!(found(source, &["Plan"]), ["plan", "PLAN"]);
    }

    #[test]
    fn the_longer_name_wins_at_the_same_place() {
        let source = "see the Release plan today";
        assert_eq!(
            found(source, &["Release", "Release plan"]),
            ["Release plan"]
        );
    }

    #[test]
    fn skips_links_code_tags_html_and_frontmatter() {
        let source = concat!(
            "---\ntitle: Plan\n---\n",
            "[[Plan]] and [the Plan](Plan.md) and `Plan` and #Plan\n",
            "<div>\nPlan\n</div>\n\n",
            "```\nPlan\n```\n\n",
            "only this Plan counts\n",
        );
        let ranges = find(source, &["Plan"]);

        assert_eq!(ranges.len(), 1, "{ranges:?}");
        assert_eq!(
            &source[ranges[0].start - 10..ranges[0].end],
            "only this Plan"
        );
    }

    #[test]
    fn a_name_can_span_text_the_parser_splits() {
        assert_eq!(found("I use C++ daily", &["C++"]), ["C++"]);
        assert_eq!(found("a [draft] plan", &["[draft] plan"]), ["[draft] plan"]);
    }

    #[test]
    fn works_with_non_ascii_letters_and_emphasis() {
        let source = "See **Über** and über-cool, not überall";
        assert_eq!(found(source, &["über"]), ["Über", "über"]);
    }

    #[test]
    fn an_empty_name_finds_nothing() {
        assert!(find("text", &["", "  "]).is_empty());
    }
}
