//! Agent notes: the comment `<!-- @agent «words»: text -->`.
//!
//! A person selects words and leaves a task for an agent that reads the file
//! later. The task is a plain HTML comment just after the selected words:
//!
//! ```text
//! Search must be fast<!-- @agent «fast»: measure on 10k notes -->.
//! ```
//!
//! The words are repeated in the comment on purpose. The agent reads raw
//! markdown and does not need to guess what the task refers to. The preview
//! uses them to know what to highlight: it highlights the words **just
//! before** the comment if they match. If they differ (someone edited the
//! text), the note stays, but without a highlight.
//!
//! The frontend has the same parser (`ui/src/agent-notes.ts`): the editor
//! draws notes itself, without a call to the core. If you change the format,
//! change both places.

/// Marker at the start of an agent note comment.
pub const MARKER: &str = "@agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNote<'a> {
    /// Words the note refers to. Empty if the note is not attached to words.
    pub anchor: &'a str,
    /// The task text.
    pub text: &'a str,
}

/// Parses `<!-- @agent «words»: text -->`. Returns `None` for anything else.
///
/// The quoted words are optional: `<!-- @agent text -->` is also a note, just
/// not attached to words.
pub fn parse(html: &str) -> Option<AgentNote<'_>> {
    let body = html
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    let rest = body.strip_prefix(MARKER)?;
    // `@agentX` is not our marker.
    if rest
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace() && ch != ':')
    {
        return None;
    }
    let rest = rest.trim_start();

    let (anchor, text) = match rest
        .strip_prefix('«')
        .and_then(|after| after.split_once('»'))
    {
        Some((anchor, text)) => (anchor, text),
        None => ("", rest),
    };
    let text = text.trim_start();
    let text = text.strip_prefix(':').unwrap_or(text).trim();
    Some(AgentNote { anchor, text })
}

/// True for an agent note. Field parsing skips it, because it is not metadata.
pub fn is_note(comment_body: &str) -> bool {
    comment_body
        .trim_start()
        .strip_prefix(MARKER)
        .is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|ch| ch.is_whitespace() || ch == ':')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_note_is_parsed() {
        let note = parse("<!-- @agent «быстрым»: замерить скорость -->").unwrap();

        assert_eq!(note.anchor, "быстрым");
        assert_eq!(note.text, "замерить скорость");
    }

    #[test]
    fn colon_after_the_anchor_is_optional() {
        let note = parse("<!-- @agent «слово» проверить -->").unwrap();

        assert_eq!(note.anchor, "слово");
        assert_eq!(note.text, "проверить");
    }

    #[test]
    fn note_without_an_anchor_is_still_a_note() {
        let note = parse("<!-- @agent: переписать абзац -->").unwrap();

        assert_eq!(note.anchor, "");
        assert_eq!(note.text, "переписать абзац");
    }

    #[test]
    fn other_comments_are_not_notes() {
        assert_eq!(parse("<!-- confluence: 42 -->"), None);
        assert_eq!(parse("<!-- @agents «x»: y -->"), None);
        assert_eq!(parse("<b>@agent</b>"), None);
    }

    #[test]
    fn is_note_matches_the_comment_body() {
        assert!(is_note(" @agent «x»: y "));
        assert!(is_note("@agent"));
        assert!(!is_note(" confluence: 42"));
        assert!(!is_note("@agentX: y"));
    }
}
