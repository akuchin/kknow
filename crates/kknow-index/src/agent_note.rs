//! Agent notes: the comment `<!-- @agent «words»: text -->`.
//!
//! A person selects words and leaves a task for an agent that reads the file
//! later. The task is a plain HTML comment just after the selected words:
//!
//! ```text
//! Search must be fast<!-- @agent «fast»: measure on 10k notes -->.
//! ```
//!
//! A note can have no words: `<!-- @agent: text -->` at the end of a
//! paragraph is about the whole paragraph. The editor writes it so when the
//! person leaves a note with only the cursor in the paragraph.
//!
//! The window calls these notes "comments"; the marker stays `@agent`,
//! because agents look for it.
//!
//! The words are repeated in the comment on purpose. The agent reads raw
//! markdown and does not need to guess what the task refers to. The preview
//! uses them to know what to highlight: it highlights the words **just
//! before** the comment if they match. If they differ (someone edited the
//! text), the note stays, but without a highlight.
//!
//! A note can carry an ID: `<!-- @agent#k3f «words»: text -->`. The ID names
//! the comment thread in `.kknow/threads/k3f.json`, where the agent and the
//! person talk about the task. A note gets its ID with the first reply, so an
//! old note without one is still a note.
//!
//! The frontend has the same parser (`ui/src/agent-notes.ts`): the editor
//! draws notes itself, without a call to the core. If you change the format,
//! change both places.

/// Marker at the start of an agent note comment.
pub const MARKER: &str = "@agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNote<'a> {
    /// The thread ID after `#`, if the note has one.
    pub id: Option<&'a str>,
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
    let (id, rest) = split_marker(body)?;
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
    Some(AgentNote { id, anchor, text })
}

/// True for an agent note. Field parsing skips it, because it is not metadata.
pub fn is_note(comment_body: &str) -> bool {
    split_marker(comment_body.trim_start()).is_some()
}

/// Splits `@agent#id rest` into the ID and the rest. `@agentX` and `@agent#`
/// without a valid ID are not our marker.
fn split_marker(body: &str) -> Option<(Option<&str>, &str)> {
    let rest = body.strip_prefix(MARKER)?;
    let (id, rest) = match rest.strip_prefix('#') {
        Some(after) => {
            let end = after
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
                .unwrap_or(after.len());
            if end == 0 || end > 32 {
                return None;
            }
            (Some(&after[..end]), &after[end..])
        }
        None => (None, rest),
    };
    if rest
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace() && ch != ':')
    {
        return None;
    }
    Some((id, rest))
}

/// The comment again, with the thread ID `id`. The editor and the core write
/// comments in the same form.
pub fn with_id(html: &str, id: &str) -> Option<String> {
    let note = parse(html)?;
    let anchor = if note.anchor.is_empty() {
        String::new()
    } else {
        format!(" «{}»", note.anchor)
    };
    Some(format!("<!-- {MARKER}#{id}{anchor}: {} -->", note.text))
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
    fn note_written_by_the_editor_without_words_is_parsed() {
        // The exact form `formatNote` in `ui/src/agent-notes.ts` writes.
        let note = parse("<!-- @agent: сократить вдвое -->").unwrap();

        assert_eq!(note.anchor, "");
        assert_eq!(note.text, "сократить вдвое");
    }

    #[test]
    fn note_with_a_thread_id_is_parsed() {
        let note = parse("<!-- @agent#k3f «14 недель»: проверь цифру -->").unwrap();

        assert_eq!(note.id, Some("k3f"));
        assert_eq!(note.anchor, "14 недель");
        assert_eq!(note.text, "проверь цифру");
        assert_eq!(parse("<!-- @agent: x -->").unwrap().id, None);
        assert_eq!(parse("<!-- @agent#: x -->"), None);
        assert!(is_note("@agent#a1: y"));
    }

    #[test]
    fn with_id_keeps_the_words_and_the_task() {
        assert_eq!(
            with_id("<!-- @agent «слово»: проверить -->", "k3f").as_deref(),
            Some("<!-- @agent#k3f «слово»: проверить -->")
        );
        assert_eq!(
            with_id("<!-- @agent#old: абзац -->", "new").as_deref(),
            Some("<!-- @agent#new: абзац -->")
        );
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
