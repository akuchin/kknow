use std::ops::Range;

use kknow_core::NoteId;
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd, TextMergeStream};

use crate::agent_note;
use crate::graph::Resolver;
use crate::parse::{ParsedNote, options, parse};
use crate::wikilink::{WikiKind, WikiLink};

/// URL scheme of the links that the renderer makes. The UI catches clicks on
/// them: `kknow://note/<id>` opens a note, `kknow://missing/<target>` is a
/// note that does not exist, `kknow://tag/<name>` filters by tag.
pub const SCHEME: &str = "kknow://";

/// Code block language that marks a mermaid diagram.
pub const MERMAID_LANG: &str = "mermaid";

/// Class of the block with the diagram source. The frontend draws it: mermaid
/// is JavaScript, and the core does not have it.
pub const MERMAID_CLASS: &str = "mermaid";

/// Class of an image from a note. Its path is in `data-src`, not `src`: the
/// webview does not read the disk, and the window gets the bytes from the core
/// with a separate command.
pub const IMAGE_CLASS: &str = "note-image";

/// Class that highlights the words an agent note refers to.
pub const AGENT_NOTE_CLASS: &str = "agent-note";

/// Class of an empty mark "source line N starts here" (`data-line`).
///
/// The preview is HTML, and it does not show which note line a paragraph comes
/// from. The window needs this for bookmarks: the icon goes next to the mark,
/// and ⌘⇧B in reading mode gets the line from it. The mark is at the start of
/// a paragraph, heading and list item. Before a code block, table and rule it
/// is a separate zero-height block (`is-block`): it cannot go inside `<pre>`
/// and `<table>`.
pub const LINE_MARK_CLASS: &str = "line-mark";

/// Marker of a line mark in the event stream while it goes through the
/// filters. A note cannot contain NUL: the parser replaces it with U+FFFD. So
/// raw HTML from the text cannot pass for a line mark.
const LINE_SENTINEL: &str = "\0kknow-line:";

/// Renders a note to HTML.
///
/// Wikilinks and tags become plain markdown links before parsing. So the
/// renderer sees them, and the code does not need to change the parser event
/// stream.
pub fn to_html(source: &str, from: &NoteId, resolver: &Resolver) -> String {
    let parsed = parse(source);
    let (prepared, lines) = rewrite(source, &parsed, from, resolver);
    markdown_to_html(&prepared, &lines)
}

/// Where a piece of the prepared text comes from in the source.
struct Piece {
    /// Start of the piece in the prepared text.
    prepared: usize,
    /// Start in the source.
    source: usize,
    /// Copied as is, so offsets inside the piece match. If not, it is a
    /// rewritten link or tag, and all offsets inside it map to its start.
    copied: bool,
}

/// Source line numbers for offsets in the prepared text.
///
/// Preparation rewrites links and drops the frontmatter, so parser offsets do
/// not map to the source directly.
struct LineMap {
    pieces: Vec<Piece>,
    /// Start offsets of the source lines.
    starts: Vec<usize>,
}

impl LineMap {
    fn line(&self, prepared: usize) -> usize {
        let index = self
            .pieces
            .partition_point(|piece| piece.prepared <= prepared);
        let source = match index.checked_sub(1).map(|index| &self.pieces[index]) {
            Some(piece) if piece.copied => piece.source + (prepared - piece.prepared),
            Some(piece) => piece.source,
            None => 0,
        };
        self.starts.partition_point(|&start| start <= source)
    }
}

fn rewrite(
    source: &str,
    parsed: &ParsedNote,
    from: &NoteId,
    resolver: &Resolver,
) -> (String, LineMap) {
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();

    for link in &parsed.wiki_links {
        replacements.push((link.span.clone(), link_markdown(link, from, resolver)));
    }
    for link in &parsed.markdown_links {
        let text = escape_text(&link.label);
        let href = match resolver.resolve(&link.target, from) {
            Some(id) => format!("{SCHEME}note/{}", encode(id.as_str())),
            None => format!("{SCHEME}missing/{}", encode(&link.target)),
        };
        replacements.push((link.span.clone(), format!("[{text}](<{href}>)")));
    }
    for tag in &parsed.tags {
        let name = escape_text(&tag.name);
        let href = encode(&tag.name);
        replacements.push((tag.span.clone(), format!("[#{name}](<{SCHEME}tag/{href}>)")));
    }
    replacements.sort_by_key(|(span, _)| span.start);

    // Frontmatter is not rendered: it is metadata, not note text.
    let mut cursor = parsed
        .frontmatter
        .as_ref()
        .map_or(0, |block| block.span.end);
    let mut out = String::with_capacity(source.len());
    let mut pieces = Vec::new();
    let copy = |out: &mut String, pieces: &mut Vec<Piece>, from: usize, to: usize| {
        pieces.push(Piece {
            prepared: out.len(),
            source: from,
            copied: true,
        });
        out.push_str(&source[from..to]);
    };

    for (span, markdown) in replacements {
        if span.start < cursor {
            continue;
        }
        copy(&mut out, &mut pieces, cursor, span.start);
        pieces.push(Piece {
            prepared: out.len(),
            source: span.start,
            copied: false,
        });
        out.push_str(&markdown);
        cursor = span.end;
    }
    copy(&mut out, &mut pieces, cursor, source.len());

    let starts = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    (out, LineMap { pieces, starts })
}

fn link_markdown(link: &WikiLink, from: &NoteId, resolver: &Resolver) -> String {
    // `![[diagram.png|300]]` is an embedded image. From here it takes the same
    // path as `![300](diagram.png)`, with the size in the alt text.
    if link.kind == WikiKind::Embed && kknow_core::image::is_image(&link.target) {
        let alt = escape_text(link.alias.as_deref().unwrap_or_default());
        let target = link.target.replace(['<', '>'], "");
        return format!("![{alt}](<{target}>)");
    }

    let text = escape_text(link.display_text());

    if link.is_internal() {
        let anchor = link.subpath.as_ref().map(subpath_text).unwrap_or_default();
        return format!("[{text}](<{SCHEME}heading/{}>)", encode(&anchor));
    }

    match resolver.resolve(&link.target, from) {
        Some(id) => format!("[{text}](<{SCHEME}note/{}>)", encode(id.as_str())),
        None => format!("[{text}](<{SCHEME}missing/{}>)", encode(&link.target)),
    }
}

fn subpath_text(subpath: &crate::wikilink::Subpath) -> String {
    match subpath {
        crate::wikilink::Subpath::Heading(heading) => heading.clone(),
        crate::wikilink::Subpath::Block(block) => block.clone(),
    }
}

fn markdown_to_html(source: &str, lines: &LineMap) -> String {
    // Text is merged into one event per paragraph piece. The parser splits it
    // at special characters, but the words to highlight are searched at the end
    // of one piece.
    let marked = Parser::new_ext(source, options())
        .into_offset_iter()
        .flat_map(|(event, range)| mark_line(event, lines.line(range.start)));
    let events = agent_notes(TextMergeStream::new(marked));

    let mut html = String::with_capacity(source.len() * 3 / 2);
    let events = mermaid_blocks(images(events.into_iter())).map(line_marks);
    pulldown_cmark::html::push_html(&mut html, events);
    html
}

/// Puts a line mark at the start of a block. In a paragraph, heading and list
/// item it goes after the opening tag. Before code, a table and a rule it goes
/// before the tag.
fn mark_line(event: Event<'_>, line: usize) -> Vec<Event<'_>> {
    let mark = |kind: &str| Event::Html(format!("{LINE_SENTINEL}{kind}{line}").into());
    match event {
        Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::Item) => vec![event, mark("i")],
        Event::Start(Tag::CodeBlock(_) | Tag::Table(_)) | Event::Rule => vec![mark("b"), event],
        other => vec![other],
    }
}

/// Turns a mark from the event stream into an empty `<span data-line>`.
fn line_marks(event: Event<'_>) -> Event<'_> {
    let Event::Html(html) = &event else {
        return event;
    };
    let Some(mark) = html.strip_prefix(LINE_SENTINEL) else {
        return event;
    };
    let (class, line) = match mark.strip_prefix('b') {
        Some(line) => (format!("{LINE_MARK_CLASS} is-block"), line),
        None => (LINE_MARK_CLASS.to_string(), mark.trim_start_matches('i')),
    };
    Event::InlineHtml(format!("<span class=\"{class}\" data-line=\"{line}\"></span>").into())
}

/// Removes raw HTML and turns agent notes into highlights.
///
/// Raw HTML from a note must not reach the webview: the note can come from
/// sync or from another person's vault, and a script in it runs with the app
/// rights. An agent note `<!-- @agent «words»: text -->` is the exception, but
/// the HTML gets our `<mark>` with escaped text, not the note itself.
///
/// The words just before the comment are highlighted if they match the words
/// in it. If not, only an icon stays: the task exists, but the words near it
/// were changed.
fn agent_notes<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::new();

    for event in events {
        let html = match event {
            // Our own line mark, not text from the note.
            Event::Html(ref html) if html.starts_with(LINE_SENTINEL) => {
                out.push(event);
                continue;
            }
            Event::InlineHtml(html) => html,
            // An agent note on its own line has nothing to highlight.
            Event::Html(_) => continue,
            other => {
                out.push(other);
                continue;
            }
        };
        let Some(note) = agent_note::parse(&html) else {
            continue;
        };
        let title = escape_html(note.text);

        let before = match out.last() {
            Some(Event::Text(text)) if !note.anchor.is_empty() => text.strip_suffix(note.anchor),
            _ => None,
        };
        match before {
            Some(before) => {
                let before = before.to_string();
                out.pop();
                if !before.is_empty() {
                    out.push(Event::Text(before.into()));
                }
                out.push(Event::InlineHtml(
                    format!("<mark class=\"{AGENT_NOTE_CLASS}\" title=\"{title}\">").into(),
                ));
                out.push(Event::Text(note.anchor.to_string().into()));
                out.push(Event::InlineHtml("</mark>".into()));
            }
            None => out.push(Event::InlineHtml(
                format!("<span class=\"{AGENT_NOTE_CLASS} is-detached\" title=\"{title}\"></span>")
                    .into(),
            )),
        }
    }
    out
}

/// Turns an image into an `<img>` that the window fills in.
///
/// The alt text can carry a size: `![diagram|300](a.png)` sets the width,
/// `|300x200` sets width and height, `![300](a.png)` is a size without alt
/// text. CSS limits the rest, so a huge screenshot does not break reading.
fn images<'a>(events: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    // `Some` while inside an image: the URL and the alt text collected so far.
    let mut image: Option<(String, String)> = None;

    events.filter_map(move |event| match event {
        Event::Start(Tag::Image { dest_url, .. }) => {
            image = Some((dest_url.to_string(), String::new()));
            None
        }
        Event::End(TagEnd::Image) => image.take().map(|(target, alt)| image_html(&target, &alt)),
        Event::Text(text) | Event::Code(text) if image.is_some() => {
            if let Some((_, alt)) = image.as_mut() {
                alt.push_str(&text);
            }
            None
        }
        _ if image.is_some() => None,
        other => Some(other),
    })
}

fn image_html<'a>(target: &str, alt: &str) -> Event<'a> {
    let (alt, size) = match alt.rsplit_once('|') {
        Some((alt, size)) if parse_size(size).is_some() => (alt, parse_size(size)),
        _ if parse_size(alt).is_some() => ("", parse_size(alt)),
        _ => (alt, None),
    };
    let style = match size {
        Some((width, Some(height))) => format!(" style=\"width:{width}px;height:{height}px\""),
        Some((width, None)) => format!(" style=\"width:{width}px\""),
        None => String::new(),
    };
    let target = escape_html(target);
    let alt = escape_html(alt.trim());
    Event::InlineHtml(
        format!("<img class=\"{IMAGE_CLASS}\" data-src=\"{target}\" alt=\"{alt}\"{style}>").into(),
    )
}

/// `300` or `300x200`: image size in pixels.
fn parse_size(raw: &str) -> Option<(u32, Option<u32>)> {
    let raw = raw.trim();
    let (width, height) = match raw.split_once('x') {
        Some((width, height)) => (width, Some(height.parse().ok()?)),
        None => (raw, None),
    };
    Some((width.parse().ok()?, height))
}

/// Turns ```` ```mermaid ```` into `<pre class="mermaid">` with the diagram
/// source inside.
///
/// Code block markup does not work for the frontend: mermaid reads the element
/// content as text, and `<pre><code>` adds a nested tag. The frontend draws
/// the diagram: mermaid is JavaScript, and the core does not have it.
fn mermaid_blocks<'a>(events: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    // `Some` while inside a diagram block: its text collected so far.
    let mut diagram: Option<String> = None;

    events.filter_map(move |event| match event {
        Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info))) if is_mermaid(info) => {
            diagram = Some(String::new());
            None
        }
        Event::Text(text) => match diagram.as_mut() {
            Some(source) => {
                source.push_str(&text);
                None
            }
            None => Some(Event::Text(text)),
        },
        Event::End(TagEnd::CodeBlock) => match diagram.take() {
            Some(source) => {
                let source = escape_html(&source);
                Some(Event::Html(
                    format!("<pre class=\"{MERMAID_CLASS}\">{source}</pre>\n").into(),
                ))
            }
            None => Some(Event::End(TagEnd::CodeBlock)),
        },
        other => Some(other),
    })
}

/// `mermaid`, `Mermaid`, `mermaid theme=dark`: the block language is the first word.
fn is_mermaid(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|lang| lang.eq_ignore_ascii_case(MERMAID_LANG))
}

/// Escapes text inside a tag. The diagram source comes from a note, that is,
/// possibly from another person's vault, and a `<script>` in it would run with
/// the app rights.
fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '[' | ']' | '\\' | '<' | '>' | '`' | '*' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// Percent-encodes a link path. `/` stays a separator.
fn encode(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(ids: &[&str]) -> Resolver {
        let ids: Vec<NoteId> = ids.iter().map(|id| NoteId::parse(id).unwrap()).collect();
        Resolver::new(ids.iter())
    }

    fn render(source: &str, known: &[&str]) -> String {
        to_html(source, &NoteId::parse("Current").unwrap(), &resolver(known))
    }

    #[test]
    fn wiki_link_becomes_an_anchor() {
        let html = render("см. [[Rust]]", &["Rust"]);

        assert!(html.contains(r#"href="kknow://note/Rust""#), "{html}");
        assert!(html.contains(">Rust</a>"));
    }

    #[test]
    fn alias_is_used_as_the_anchor_text() {
        let html = render(
            "[[Projects/Rust/kknow|мой проект]]",
            &["Projects/Rust/kknow"],
        );

        assert!(
            html.contains(r#"href="kknow://note/Projects/Rust/kknow""#),
            "{html}"
        );
        assert!(html.contains(">мой проект</a>"));
    }

    #[test]
    fn markdown_links_to_notes_are_rewritten_too() {
        let html = render(
            "[к проекту](Projects/Rust/kknow.md)",
            &["Projects/Rust/kknow"],
        );

        assert!(
            html.contains(r#"href="kknow://note/Projects/Rust/kknow""#),
            "{html}"
        );
        assert!(html.contains(">к проекту</a>"));
    }

    #[test]
    fn external_links_are_left_alone() {
        let html = render("[сайт](https://example.com)", &[]);

        assert!(html.contains(r#"href="https://example.com""#), "{html}");
    }

    #[test]
    fn missing_note_gets_a_distinct_scheme() {
        let html = render("[[Призрак]]", &[]);

        assert!(html.contains("kknow://missing/"), "{html}");
    }

    #[test]
    fn tags_become_links() {
        let html = render("текст #rust дальше", &[]);

        assert!(html.contains(r#"href="kknow://tag/rust""#), "{html}");
        assert!(html.contains(">#rust</a>"));
    }

    #[test]
    fn frontmatter_is_not_rendered() {
        let html = render("---\ntags: [секрет]\n---\n\n# Тело\n", &[]);

        assert!(!html.contains("секрет"), "{html}");
        // Frontmatter lines count: the heading is line 5 of the file.
        assert!(
            html.contains(&format!("<h1>{}Тело</h1>", mark(5))),
            "{html}"
        );
    }

    #[test]
    fn raw_html_is_dropped() {
        let html = render("текст <script>alert(1)</script> и <b>жирный</b>", &[]);

        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<b>"), "{html}");
        assert!(html.contains("alert(1)"));
    }

    #[test]
    fn agent_note_highlights_its_words() {
        let html = render(
            "Поиск должен быть быстрым<!-- @agent «быстрым»: замерить -->.",
            &[],
        );

        assert!(
            html.contains(r#"быть <mark class="agent-note" title="замерить">быстрым</mark>."#),
            "{html}"
        );
        assert!(!html.contains("<!--"), "{html}");
    }

    #[test]
    fn agent_note_text_is_escaped() {
        let html = render("слово<!-- @agent «слово»: \"><script>x</script> -->", &[]);

        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("&quot;&gt;&lt;script&gt;"), "{html}");
    }

    #[test]
    fn detached_agent_note_leaves_a_marker() {
        let html = render("другие слова<!-- @agent «слово»: проверить -->", &[]);

        assert!(html.contains("другие слова<span"), "{html}");
        assert!(
            html.contains(r#"class="agent-note is-detached" title="проверить""#),
            "{html}"
        );
    }

    #[test]
    fn agent_note_without_words_leaves_a_marker_at_the_paragraph_end() {
        let html = render("Весь абзац целиком.<!-- @agent: переписать короче -->", &[]);

        assert!(html.contains("Весь абзац целиком.<span"), "{html}");
        assert!(
            html.contains(r#"class="agent-note is-detached" title="переписать короче""#),
            "{html}"
        );
        assert!(!html.contains("<mark"), "{html}");
    }

    #[test]
    fn agent_note_after_formatting_is_detached_not_broken() {
        let html = render("**жирно**<!-- @agent «жирно»: x -->", &[]);

        assert!(html.contains("<strong>жирно</strong><span"), "{html}");
    }

    #[test]
    fn other_comments_are_still_dropped() {
        let html = render("текст <!-- confluence: 42 --> дальше", &[]);

        assert!(!html.contains("confluence"), "{html}");
        assert!(!html.contains("agent-note"), "{html}");
    }

    #[test]
    fn code_blocks_survive_untouched() {
        let html = render("```rust\nlet x = [[1, 2]];\n```", &[]);

        assert!(html.contains("<code"), "{html}");
        assert!(html.contains("[[1, 2]]"), "{html}");
        assert!(!html.contains("kknow://"));
    }

    #[test]
    fn markdown_formatting_still_works() {
        let html = render("# Заголовок\n\n- пункт **жирный**\n", &[]);

        assert!(
            html.contains(&format!("<h1>{}Заголовок</h1>", mark(1))),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "<li>{}пункт <strong>жирный</strong></li>",
                mark(3)
            )),
            "{html}"
        );
    }

    fn mark(line: usize) -> String {
        format!(r#"<span class="line-mark" data-line="{line}"></span>"#)
    }

    fn block_mark(line: usize) -> String {
        format!(r#"<span class="line-mark is-block" data-line="{line}"></span>"#)
    }

    #[test]
    fn blocks_are_marked_with_their_source_lines() {
        let html = render(
            "# Заголовок\n\nабзац\nего вторая строка\n\n```rust\nfn main() {}\n```\n\n---\n\n| a |\n|---|\n| 1 |\n",
            &[],
        );

        assert!(html.contains(&format!("<p>{}абзац", mark(3))), "{html}");
        // `push_html` puts a newline before the block itself.
        assert!(
            html.contains(&format!("{}\n<pre>", block_mark(6))),
            "{html}"
        );
        assert!(
            html.contains(&format!("{}\n<hr />", block_mark(10))),
            "{html}"
        );
        assert!(
            html.contains(&format!("{}<table>", block_mark(12))),
            "{html}"
        );
        assert!(
            !html.contains("data-line=\"4\""),
            "продолжение абзаца — тот же абзац"
        );
    }

    #[test]
    fn rewritten_links_do_not_shift_the_lines() {
        // Links and tags become longer markup before parsing. Line numbers
        // must not shift because of this.
        let html = render("[[Rust|язык]] и #тег\n\n[[Rust]]\n\nтретий\n", &["Rust"]);

        assert!(html.contains(&format!("<p>{}<a", mark(1))), "{html}");
        assert!(html.contains(&format!("<p>{}<a", mark(3))), "{html}");
        assert!(html.contains(&format!("<p>{}третий", mark(5))), "{html}");
    }

    #[test]
    fn a_line_mark_cannot_be_forged_from_the_note() {
        let html = render("<div>\u{0}kknow-line:i1</div>\n", &[]);

        assert!(!html.contains("data-line"), "{html}");
    }

    #[test]
    fn mermaid_diagram_keeps_its_line_mark() {
        let html = render("текст\n\n```mermaid\ngraph TD\n```\n", &[]);

        assert!(
            html.contains(&format!("{}<pre class=\"mermaid\">", block_mark(3))),
            "{html}"
        );
    }

    #[test]
    fn mermaid_block_becomes_a_diagram_holder() {
        let html = render("```mermaid\ngraph TD;\n  A-->B;\n```", &[]);

        assert!(html.contains(r#"<pre class="mermaid">"#), "{html}");
        assert!(html.contains("graph TD;"), "{html}");
        // Only text inside: mermaid reads the element content as is.
        assert!(!html.contains("<code"), "{html}");
    }

    #[test]
    fn mermaid_language_is_recognised_loosely() {
        for info in ["Mermaid", "mermaid ", "mermaid theme=dark"] {
            let html = render(&format!("```{info}\ngraph TD;\n```"), &[]);
            assert!(html.contains(r#"<pre class="mermaid">"#), "{info}: {html}");
        }
    }

    #[test]
    fn diagram_source_is_escaped() {
        let html = render("```mermaid\ngraph TD;\n  A-->B & <script>;\n```", &[]);

        assert!(!html.contains("<script"), "{html}");
        assert!(html.contains("--&gt;B &amp; &lt;script&gt;"), "{html}");
    }

    #[test]
    fn links_inside_a_diagram_are_left_alone() {
        let html = render("```mermaid\ngraph TD;\n  A[[[Rust]]];\n```", &["Rust"]);

        assert!(!html.contains("kknow://"), "{html}");
        assert!(html.contains("[[Rust]]"), "{html}");
    }

    #[test]
    fn other_code_blocks_stay_code_blocks() {
        let html = render("```rust\nlet mermaid = 1;\n```", &[]);

        assert!(html.contains("<code"), "{html}");
        assert!(!html.contains(r#"class="mermaid""#), "{html}");
    }

    #[test]
    fn text_after_a_diagram_is_rendered_as_usual() {
        let html = render("```mermaid\ngraph TD;\n```\n\nдальше [[Rust]]\n", &["Rust"]);

        assert!(html.contains(r#"<pre class="mermaid">"#), "{html}");
        assert!(html.contains(r#"href="kknow://note/Rust""#), "{html}");
    }

    #[test]
    fn markdown_image_waits_for_the_window() {
        let html = render("текст ![схема **связей**](<img/a b.png>) дальше", &[]);

        assert!(
            html.contains(r#"<img class="note-image" data-src="img/a b.png" alt="схема связей">"#),
            "{html}"
        );
        assert!(!html.contains(" src="), "{html}");
    }

    #[test]
    fn image_embed_becomes_an_image() {
        let html = render("![[Скрин 1.png]]", &[]);

        assert!(html.contains(r#"data-src="Скрин 1.png""#), "{html}");
        assert!(!html.contains("kknow://"), "{html}");
    }

    #[test]
    fn note_embed_is_still_a_link() {
        let html = render("![[Rust]]", &["Rust"]);

        assert!(html.contains(r#"href="kknow://note/Rust""#), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn size_comes_from_the_caption() {
        let cases = [
            ("![[a.png|300]]", r#"alt="" style="width:300px""#),
            (
                "![схема|300x200](a.png)",
                r#"alt="схема" style="width:300px;height:200px""#,
            ),
            ("![версия 2|x](a.png)", r#"alt="версия 2|x">"#),
        ];
        for (source, expected) in cases {
            let html = render(source, &[]);
            assert!(html.contains(expected), "{source}: {html}");
        }
    }

    #[test]
    fn image_attributes_are_escaped() {
        let html = render(r#"![x" onerror="alert(1)](<a"b.png>)"#, &[]);

        assert!(!html.contains(r#"" onerror"#), "{html}");
        assert!(html.contains("&quot;"), "{html}");
    }

    #[test]
    fn non_ascii_targets_are_percent_encoded() {
        let html = render("[[Заметка]]", &["Заметка"]);

        assert!(html.contains("kknow://note/%D0%97"), "{html}");
        assert!(html.contains(">Заметка</a>"));
    }
}
