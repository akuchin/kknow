//! The line around a place in a note: for a backlink or a mention.
//!
//! A list of backlinks shows where each link stands. The whole line can be a
//! long paragraph, so the text around the place is cut to a few words on each
//! side.

use std::ops::Range;

/// How many characters to keep on each side of the place.
const SIDE: usize = 60;

/// The text of a line, in three parts: before the place, the place itself
/// and after it. The window marks the middle part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineContext {
    /// 1-based line number, frontmatter included.
    pub line: usize,
    pub before: String,
    pub text: String,
    pub after: String,
}

impl LineContext {
    /// The line around `span` in `source`. `None` when the span does not fit
    /// the text: the file changed after the parse.
    pub fn around(source: &str, span: &Range<usize>) -> Option<Self> {
        let text = source.get(span.clone())?;
        let start = source[..span.start].rfind('\n').map_or(0, |at| at + 1);
        let end = source[span.end..]
            .find('\n')
            .map_or(source.len(), |at| span.end + at);

        Some(LineContext {
            line: source[..span.start].matches('\n').count() + 1,
            before: tail(source[start..span.start].trim_start()),
            text: text.to_string(),
            after: head(source[span.end..end].trim_end()),
        })
    }
}

/// The last `SIDE` characters, with `…` when the text was longer.
fn tail(text: &str) -> String {
    match text.char_indices().rev().nth(SIDE - 1) {
        Some((at, _)) if at > 0 => format!("…{}", text[at..].trim_start()),
        _ => text.to_string(),
    }
}

/// The first `SIDE` characters, with `…` when the text was longer.
fn head(text: &str) -> String {
    match text.char_indices().nth(SIDE) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(source: &str, part: &str) -> LineContext {
        let start = source.find(part).unwrap();
        LineContext::around(source, &(start..start + part.len())).unwrap()
    }

    #[test]
    fn gives_the_line_in_three_parts() {
        let context = around("# Title\n\n  - see [[Plan]] today\nnext", "[[Plan]]");

        assert_eq!(context.line, 3);
        assert_eq!(context.before, "- see ");
        assert_eq!(context.text, "[[Plan]]");
        assert_eq!(context.after, " today");
    }

    #[test]
    fn cuts_a_long_line_on_both_sides() {
        let words = "word ".repeat(30);
        let source = format!("{words}[[Plan]]{words}");
        let context = around(&source, "[[Plan]]");

        assert!(context.before.starts_with('…'), "{}", context.before);
        assert!(context.after.ends_with('…'), "{}", context.after);
        assert!(context.before.chars().count() <= SIDE + 1);
        assert!(context.after.chars().count() <= SIDE + 1);
    }

    #[test]
    fn counts_characters_not_bytes() {
        let source = format!("{}[[План]]", "ж".repeat(SIDE));
        let context = around(&source, "[[План]]");

        assert_eq!(context.before, "ж".repeat(SIDE), "exactly SIDE characters is not cut");
    }

    #[test]
    fn a_span_outside_the_text_gives_nothing() {
        assert!(LineContext::around("short", &(3..40)).is_none());
        assert!(LineContext::around("жук", &(1..2)).is_none(), "not a char boundary");
    }
}
