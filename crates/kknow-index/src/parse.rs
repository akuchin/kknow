use std::collections::BTreeMap;
use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag as MdTag, TagEnd};

use crate::agent_note;
use crate::tag::{self, Tag};
use crate::wikilink::{self, WikiLink};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub span: Range<usize>,
}

/// YAML block at the start of the file.
///
/// The parser cuts it out so that it does not get into the text and links.
/// [`Frontmatter::fields`] gets the top-level scalar fields. A note uses them
/// to pass parameters to its commands (for example, an export page id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub raw: String,
    pub span: Range<usize>,
}

impl Frontmatter {
    /// Top-level fields of the form `key: value`.
    ///
    /// This is not a YAML parser: a real one adds a dependency for things that
    /// notes almost never have. Only top-level scalar fields are taken, the
    /// usual reason for frontmatter: `status`, `page: 123456789`,
    /// `due: 2026-08-14`. Nested maps and block lists (`- item`) are skipped:
    /// they have no single value to put into an environment variable.
    ///
    /// Quotes around a value are removed, because they are there for YAML, not
    /// for the reader of the value. All else stays as written, `[a, b]` too:
    /// it is also one string, and the caller decides what to do with it.
    pub fn fields(&self) -> BTreeMap<String, String> {
        let mut fields = BTreeMap::new();

        for line in self.raw.lines() {
            // Indent means nesting, `#` a comment, `-` a list.
            if line.starts_with([' ', '\t', '#', '-']) {
                continue;
            }
            take_field(line, &mut fields);
        }

        fields
    }
}

/// Metadata as a comment: `<!-- page: 987654321 -->`.
///
/// A second way to say what frontmatter says. It is for places where others
/// see the frontmatter: a code host shows the frontmatter of a repository
/// `readme.md` as a block, and an export copies it to the page. A comment is
/// visible nowhere: not in the preview (raw HTML is removed there) and not in
/// other markdown viewers.
///
/// The comment can be anywhere in the body and can span several lines:
///
/// ```text
/// <!--
///   page: 987654321
///   space: DOCS
/// -->
/// ```
fn take_comment_fields(html: &str, fields: &mut BTreeMap<String, String>) {
    let mut rest = html;
    while let Some(open) = rest.find("<!--") {
        let after = &rest[open + 4..];
        let Some(close) = after.find("-->") else {
            return;
        };

        let body = &after[..close];
        rest = &after[close + 3..];
        // An agent task is not metadata. `«words»: text` also has a colon, and
        // without this check it becomes a field.
        if agent_note::is_note(body) {
            continue;
        }
        for line in body.lines() {
            take_field(line.trim(), fields);
        }
    }
}

/// Parses a `key: value` line and stores it if the key is not there yet.
///
/// The first match wins. Frontmatter is parsed before the body, so it beats a
/// comment, and of two equal comments the upper one wins. Both sources use one
/// rule, so nobody has to remember which one overrides which.
fn take_field(line: &str, fields: &mut BTreeMap<String, String>) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };

    let key = key.trim();
    if key.is_empty() {
        return;
    }
    fields
        .entry(key.to_string())
        .or_insert_with(|| unquote(value.trim()).to_string());
}

/// Removes quotes if they enclose the whole value.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// A plain markdown link `[text](note.md)` to a file in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    pub target: String,
    pub label: String,
    pub span: Range<usize>,
}

/// A task list item: `- [ ] text` or `- [x] text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The item text without markup, up to a nested list.
    pub text: String,
    pub done: bool,
    /// 1-based source line, frontmatter included.
    pub line: usize,
    /// The `[ ]` marker in the source.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedNote {
    pub frontmatter: Option<Frontmatter>,
    pub headings: Vec<Heading>,
    pub wiki_links: Vec<WikiLink>,
    pub markdown_links: Vec<MarkdownLink>,
    pub tags: Vec<Tag>,
    /// Task list items in source order.
    pub tasks: Vec<Task>,
    /// Scalar metadata: frontmatter fields and the same as
    /// `<!-- key: value -->` comments. A note uses them to pass parameters to
    /// its commands, for example an export page id.
    pub fields: BTreeMap<String, String>,
    /// Text without markup, for the full-text index.
    pub text: String,
}

impl ParsedNote {
    /// The first-level heading, if the note body has one.
    pub fn title(&self) -> Option<&str> {
        self.headings
            .iter()
            .find(|heading| heading.level == 1)
            .map(|heading| heading.text.as_str())
    }
}

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
}

/// Parses a whole note.
///
/// All `span` values count from the start of the given text, frontmatter
/// included.
pub fn parse(source: &str) -> ParsedNote {
    let (frontmatter, body_start) = split_frontmatter(source);

    let mut skip: Vec<Range<usize>> = Vec::new();
    if let Some(frontmatter) = &frontmatter {
        skip.push(frontmatter.span.clone());
    }

    // Frontmatter is parsed before the body, so on equal keys it beats a
    // comment: an explicit metadata block is stronger than a line lost in the
    // middle of the text.
    let mut fields = frontmatter
        .as_ref()
        .map(Frontmatter::fields)
        .unwrap_or_default();

    let mut headings = Vec::new();
    let mut markdown_links = Vec::new();
    let mut text = String::new();
    // Raw HTML of the note in order. Comment metadata comes from it.
    let mut raw_html = String::new();

    let mut heading: Option<(u8, Range<usize>, String)> = None;
    let mut link: Option<(String, Range<usize>, String)> = None;
    let mut tasks = Vec::new();
    // The task whose text is being collected.
    let mut task: Option<Task> = None;
    let mut lines = LineCounter::new(source);

    for (event, range) in Parser::new_ext(&source[body_start..], options()).into_offset_iter() {
        let range = range.start + body_start..range.end + body_start;

        match event {
            Event::Start(MdTag::Heading { level, .. }) => {
                heading = Some((level as u8, range, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, span, heading_text)) = heading.take() {
                    headings.push(Heading {
                        level,
                        text: heading_text.trim().to_string(),
                        span,
                    });
                }
                text.push('\n');
            }
            Event::Start(MdTag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), range, String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((target, span, label)) = link.take()
                    && !is_external(&target)
                {
                    markdown_links.push(MarkdownLink {
                        target: decode_target(&target),
                        label: label.trim().to_string(),
                        span,
                    });
                }
            }
            Event::Start(MdTag::CodeBlock(_)) => skip.push(range),
            Event::Code(code) => {
                skip.push(range);
                if let Some(task) = task.as_mut() {
                    task.text.push_str(&code);
                }
                text.push_str(&code);
            }
            Event::InlineMath(_) | Event::DisplayMath(_) => skip.push(range),
            // Comment metadata arrives here with all raw HTML. This is useful:
            // inside a code block the parser gives text, not HTML, so a
            // `<!-- … -->` in an example does not become metadata.
            //
            // It is parsed after the walk, not here: the parser gives a
            // multi-line comment as **several** events, one per line, and
            // `<!--` and `-->` end up in different events.
            Event::Html(html) | Event::InlineHtml(html) => {
                raw_html.push_str(&html);
                skip.push(range);
            }
            // The parser gives the marker only in a real list item, so
            // `- [ ]` in a code block is not a task.
            Event::TaskListMarker(done) => {
                task = Some(Task {
                    text: String::new(),
                    done,
                    line: lines.line_at(range.start),
                    span: range,
                });
            }
            // The text of a task ends with its paragraph or at a nested list.
            Event::Start(MdTag::List(_)) | Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                if let Some(mut found) = task.take() {
                    found.text = unwrap_wiki_links(found.text.trim());
                    tasks.push(found);
                }
                if matches!(event, Event::End(_)) {
                    text.push('\n');
                }
            }
            Event::Text(chunk) => {
                if let Some(task) = task.as_mut() {
                    task.text.push_str(&chunk);
                }
                if let Some((_, _, buffer)) = heading.as_mut() {
                    buffer.push_str(&chunk);
                }
                if let Some((_, _, label)) = link.as_mut() {
                    label.push_str(&chunk);
                }
                text.push_str(&chunk);
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(task) = task.as_mut() {
                    task.text.push(' ');
                }
                text.push('\n');
            }
            _ => {}
        }
    }

    take_comment_fields(&raw_html, &mut fields);

    let wiki_links = wikilink::scan(source, &skip);

    // A `#` inside a link is a subpath (`[[note#Section]]`, `(note.md#anchor)`),
    // not a tag, so the tag scan skips the links found.
    let mut tag_skip = skip;
    tag_skip.extend(wiki_links.iter().map(|link| link.span.clone()));
    tag_skip.extend(markdown_links.iter().map(|link| link.span.clone()));

    ParsedNote {
        frontmatter,
        headings,
        wiki_links,
        markdown_links,
        tags: tag::scan(source, &tag_skip),
        tasks,
        fields,
        text: unwrap_wiki_links(&text),
    }
}

/// Sets the task on 1-based line `line` to `done` and returns the new text.
///
/// `None` means the line has no task: the file can change after the caller
/// got the line number. Only the marker character changes. The rest of the
/// text stays byte for byte.
pub fn set_task(source: &str, line: usize, done: bool) -> Option<String> {
    let task = parse(source)
        .tasks
        .into_iter()
        .find(|task| task.line == line)?;
    let mut out = source.to_string();
    out.replace_range(
        task.span.start + 1..task.span.end - 1,
        if done { "x" } else { " " },
    );
    Some(out)
}

/// Line numbers of growing offsets, without a recount from the start.
struct LineCounter<'a> {
    source: &'a str,
    offset: usize,
    line: usize,
}

impl<'a> LineCounter<'a> {
    fn new(source: &'a str) -> Self {
        LineCounter {
            source,
            offset: 0,
            line: 1,
        }
    }

    fn line_at(&mut self, offset: usize) -> usize {
        self.line += self.source[self.offset..offset].matches('\n').count();
        self.offset = offset;
        self.line
    }
}

/// Replaces `[[note|label]]` with "label". The text for search and snippets
/// must keep what a person reads, not the link markup.
fn unwrap_wiki_links(text: &str) -> String {
    let links = wikilink::scan(text, &[]);
    if links.is_empty() {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for link in links {
        out.push_str(&text[cursor..link.span.start]);
        out.push_str(link.display_text());
        cursor = link.span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Cuts off the YAML frontmatter and returns the offset where the body starts.
fn split_frontmatter(source: &str) -> (Option<Frontmatter>, usize) {
    let Some(after_open) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return (None, 0);
    };

    let open_len = source.len() - after_open.len();
    let mut offset = open_len;

    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            let end = offset + line.len();
            return (
                Some(Frontmatter {
                    raw: source[open_len..offset].to_string(),
                    span: 0..end,
                }),
                end,
            );
        }
        offset += line.len();
    }

    // No closing delimiter: this is plain text, not frontmatter.
    (None, 0)
}

fn is_external(target: &str) -> bool {
    let lowercase = target.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "ftp://", "tel:"]
        .iter()
        .any(|scheme| lowercase.starts_with(scheme))
}

/// Decodes `%20` and similar sequences to plain characters.
fn decode_target(target: &str) -> String {
    let mut decoded = String::with_capacity(target.len());
    let mut chars = target.chars();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            decoded.push(ch);
            continue;
        }
        let rest = chars.as_str();
        match rest
            .get(..2)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            Some(byte) if byte.is_ascii() => {
                decoded.push(byte as char);
                chars.next();
                chars.next();
            }
            _ => decoded.push(ch),
        }
    }

    decoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wikilink::WikiKind;

    #[test]
    fn collects_headings_and_title() {
        let note = parse("# Заметка\n\nтекст\n\n## Раздел\n");

        assert_eq!(note.title(), Some("Заметка"));
        assert_eq!(note.headings.len(), 2);
        assert_eq!(note.headings[1].level, 2);
        assert_eq!(note.headings[1].text, "Раздел");
    }

    #[test]
    fn does_not_see_links_and_tags_inside_code() {
        let note = parse("`[[NotALink]]` и #real\n\n```\n[[AlsoNotALink]] #nope\n```\n");

        assert_eq!(note.wiki_links.len(), 0);
        assert_eq!(note.tags.len(), 1);
        assert_eq!(note.tags[0].name, "real");
    }

    #[test]
    fn frontmatter_is_cut_out_of_the_body() {
        let source = "---\ntags: [rust]\nlink: \"[[Ghost]]\"\n---\n\n# Тело\n\n[[Real]]\n";
        let note = parse(source);

        let frontmatter = note.frontmatter.as_ref().unwrap();
        assert_eq!(frontmatter.raw, "tags: [rust]\nlink: \"[[Ghost]]\"\n");
        assert_eq!(note.title(), Some("Тело"));
        assert_eq!(note.wiki_links.len(), 1);
        assert_eq!(note.wiki_links[0].target, "Real");
    }

    #[test]
    fn frontmatter_gives_up_its_scalar_fields() {
        let source = concat!(
            "---\n",
            "confluence: 123456789\n",
            "status: \"в работе\"\n",
            "space: 'DOCS'\n",
            "пусто:\n",
            "# комментарий\n",
            "tags: [rust, заметки]\n",
            "автор:\n  имя: kknow\n",
            "список:\n- раз\n",
            "---\n\n# Тело\n",
        );

        let fields = parse(source).frontmatter.unwrap().fields();

        assert_eq!(fields["confluence"], "123456789");
        assert_eq!(fields["status"], "в работе", "кавычки ставят ради YAML");
        assert_eq!(fields["space"], "DOCS");
        assert_eq!(fields["пусто"], "");
        assert_eq!(fields["tags"], "[rust, заметки]", "это тоже одна строка");
        // A nested map and a block list are not scalars: the key stays with an
        // empty value, and their content goes nowhere.
        assert_eq!(fields["автор"], "");
        assert_eq!(fields["список"], "");
        assert!(!fields.contains_key("имя"), "вложенное не всплывает наверх");
        assert!(!fields.contains_key("# комментарий"));
    }

    #[test]
    fn a_note_without_frontmatter_has_no_fields() {
        let note = parse("# Просто заголовок\n");

        assert!(note.frontmatter.is_none());
        assert!(note.fields.is_empty());
    }

    /// The second way to set metadata is a comment. It is for places where
    /// others see the frontmatter, like a repository `readme.md`.
    #[test]
    fn a_comment_carries_metadata_too() {
        let note = parse(concat!(
            "# Про проект\n\n",
            "<!-- confluence: 987654321  -->\n\n",
            "Текст.\n\n",
            "<!--\n  space: DOCS\n  status: \"в работе\"\n-->\n",
        ));

        assert_eq!(note.fields["confluence"], "987654321");
        assert_eq!(
            note.fields["space"], "DOCS",
            "комментарий бывает в несколько строк"
        );
        assert_eq!(note.fields["status"], "в работе");
        // The search text must not contain the comment: nobody reads it.
        assert!(!note.text.contains("confluence"), "{:?}", note.text);
    }

    #[test]
    fn a_comment_may_stand_at_the_end_of_a_line() {
        let note = parse("Строка <!-- confluence: 42 --> и дальше текст\n");

        assert_eq!(note.fields["confluence"], "42");
    }

    #[test]
    fn frontmatter_wins_over_a_comment_and_the_upper_comment_over_the_lower() {
        let note = parse(concat!(
            "---\nconfluence: из-шапки\n---\n\n",
            "<!-- confluence: из-комментария -->\n\n",
            "<!-- space: сверху -->\n\n",
            "<!-- space: снизу -->\n",
        ));

        assert_eq!(note.fields["confluence"], "из-шапки");
        assert_eq!(note.fields["space"], "сверху");
    }

    #[test]
    fn a_note_for_the_agent_is_not_metadata() {
        let note = parse("Поиск быстрым<!-- @agent «быстрым»: замерить -->.\n");

        assert!(note.fields.is_empty(), "{:?}", note.fields);
    }

    #[test]
    fn a_comment_inside_a_code_block_is_an_example_not_metadata() {
        let note = parse(concat!(
            "# Как это писать\n\n",
            "```markdown\n<!-- confluence: 123 -->\n```\n\n",
            "И `<!-- space: DOCS -->` тоже пример.\n",
        ));

        assert!(note.fields.is_empty(), "{:?}", note.fields);
    }

    #[test]
    fn three_dashes_without_a_closing_line_are_just_text() {
        let note = parse("---\nпросто текст\n");

        assert!(note.frontmatter.is_none());
    }

    #[test]
    fn spans_stay_valid_after_frontmatter() {
        let source = "---\na: 1\n---\n\n# Тело\n\n[[Real]]\n";
        let note = parse(source);

        assert_eq!(&source[note.wiki_links[0].span.clone()], "[[Real]]");
        assert_eq!(&source[note.headings[0].span.clone()], "# Тело\n");
    }

    #[test]
    fn keeps_internal_markdown_links_and_drops_external() {
        let note = parse("[локальная](Projects/Rust.md) и [внешняя](https://example.com)");

        assert_eq!(note.markdown_links.len(), 1);
        assert_eq!(note.markdown_links[0].target, "Projects/Rust.md");
        assert_eq!(note.markdown_links[0].label, "локальная");
    }

    #[test]
    fn decodes_percent_escapes_in_markdown_targets() {
        let note = parse("[заметка](My%20Note.md)");

        assert_eq!(note.markdown_links[0].target, "My Note.md");
    }

    #[test]
    fn collects_embeds_together_with_links() {
        let note = parse("![[diagram.png]]\n\n[[Rust]]\n");

        assert_eq!(note.wiki_links[0].kind, WikiKind::Embed);
        assert_eq!(note.wiki_links[1].kind, WikiKind::Link);
    }

    #[test]
    fn hash_inside_a_link_is_not_a_tag() {
        let note = parse("[[note#Раздел]] и [[#Планы]] и [текст](note.md#anchor), но #настоящий");

        let names: Vec<&str> = note.tags.iter().map(|tag| tag.name.as_str()).collect();
        assert_eq!(names, ["настоящий"]);
    }

    #[test]
    fn text_keeps_what_a_reader_sees_instead_of_link_syntax() {
        let note = parse("см. [[Projects/kknow|мой проект]] и [[Rust]]");

        assert_eq!(note.text.trim(), "см. мой проект и Rust");
    }

    #[test]
    fn text_drops_markup_but_keeps_words() {
        let note = parse("# Заголовок\n\nабзац с **жирным** и `кодом`.\n");

        assert!(note.text.contains("Заголовок"));
        assert!(note.text.contains("жирным"));
        assert!(note.text.contains("кодом"));
        assert!(!note.text.contains('*'));
    }

    fn summary(note: &ParsedNote) -> Vec<(usize, bool, &str)> {
        note.tasks
            .iter()
            .map(|task| (task.line, task.done, task.text.as_str()))
            .collect()
    }

    #[test]
    fn collects_tasks_with_lines_and_state() {
        let source = concat!(
            "---\nstatus: план\n---\n",
            "# Дела\n\n",
            "- [ ] купить **молоко**\n",
            "- [x] позвонить [[Люди/Иван|Ивану]]\n",
            "- обычный пункт\n",
            "* [X] сдать `отчёт`\n",
        );
        let note = parse(source);

        assert_eq!(
            summary(&note),
            [
                (6, false, "купить молоко"),
                (7, true, "позвонить Ивану"),
                (9, true, "сдать отчёт"),
            ]
        );
        assert_eq!(&source[note.tasks[0].span.clone()], "[ ]");
    }

    #[test]
    fn a_task_in_a_code_block_is_an_example() {
        let note = parse("```markdown\n- [ ] пример\n```\n\n    - [ ] отступ\n\n`- [ ] код`\n");

        assert!(note.tasks.is_empty(), "{:?}", note.tasks);
    }

    #[test]
    fn nested_tasks_keep_their_own_text() {
        let source = concat!(
            "1. [ ] проект\n",
            "   продолжение\n",
            "   - [x] шаг один\n",
            "   - [ ] шаг два\n",
            "\n",
            "> - [ ] в цитате\n",
        );
        let note = parse(source);

        assert_eq!(
            summary(&note),
            [
                (1, false, "проект продолжение"),
                (3, true, "шаг один"),
                (4, false, "шаг два"),
                (6, false, "в цитате"),
            ]
        );
    }

    #[test]
    fn set_task_changes_only_the_marker() {
        let source = "# Дела\n\n- [ ] один\n  - [x] два\n";

        assert_eq!(
            set_task(source, 3, true).as_deref(),
            Some("# Дела\n\n- [x] один\n  - [x] два\n")
        );
        assert_eq!(
            set_task(source, 4, false).as_deref(),
            Some("# Дела\n\n- [ ] один\n  - [ ] два\n")
        );
        // The same state again is not an error: two clicks must not fail.
        assert_eq!(set_task(source, 4, true).as_deref(), Some(source));
    }

    #[test]
    fn set_task_refuses_a_line_without_a_task() {
        let source = "# Дела\n\n- [ ] один\n\n```\n- [ ] пример\n```\n";

        assert_eq!(set_task(source, 1, true), None, "заголовок не задача");
        assert_eq!(set_task(source, 6, true), None, "пример в коде не задача");
        assert_eq!(set_task(source, 40, true), None, "такой строки нет");
    }
}
