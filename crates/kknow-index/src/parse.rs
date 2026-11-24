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
    pub line: usize,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub raw: String,
    pub span: Range<usize>,
}

impl Frontmatter {
    pub fn fields(&self) -> BTreeMap<String, String> {
        let mut fields = BTreeMap::new();

        for line in self.raw.lines() {
            if line.starts_with([' ', '\t', '#', '-']) {
                continue;
            }
            take_field(line, &mut fields);
        }

        fields
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        let mut lines = self.raw.lines();
        let value = loop {
            let Some(line) = lines.next() else {
                return Vec::new();
            };
            if line.starts_with([' ', '\t', '#', '-']) {
                continue;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.trim() == key
            {
                break value.trim();
            }
        };

        let items: Vec<&str> =
            if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                split_inline_list(inner)
            } else if value.is_empty() {
                lines
                    .map(str::trim_start)
                    .take_while(|line| line.starts_with('-') || line.is_empty())
                    .filter_map(|line| line.strip_prefix('-'))
                    .collect()
            } else {
                vec![value]
            };

        items
            .into_iter()
            .map(|item| unquote(item.trim()).trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }
}

fn split_inline_list(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (at, c) in inner.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, ',') => {
                items.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    items
}

fn take_comment_fields(html: &str, fields: &mut BTreeMap<String, String>) {
    let mut rest = html;
    while let Some(open) = rest.find("<!--") {
        let after = &rest[open + 4..];
        let Some(close) = after.find("-->") else {
            return;
        };

        let body = &after[..close];
        rest = &after[close + 3..];
        if agent_note::is_note(body) {
            continue;
        }
        for line in body.lines() {
            take_field(line.trim(), fields);
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    pub target: String,
    pub label: String,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub text: String,
    pub done: bool,
    pub line: usize,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentComment {
    pub id: Option<String>,
    pub anchor: String,
    pub text: String,
    pub line: usize,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedNote {
    pub frontmatter: Option<Frontmatter>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<Block>,
    pub wiki_links: Vec<WikiLink>,
    pub markdown_links: Vec<MarkdownLink>,
    pub tags: Vec<Tag>,
    pub tasks: Vec<Task>,
    pub agent_comments: Vec<AgentComment>,
    pub fields: BTreeMap<String, String>,
    pub aliases: Vec<String>,
    pub text: String,
}

impl ParsedNote {
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

struct OpenHeading {
    level: u8,
    line: usize,
    span: Range<usize>,
    text: String,
}

struct OpenLink {
    target: String,
    span: Range<usize>,
    label: String,
}

struct Walk<'a> {
    skip: Vec<Range<usize>>,
    headings: Vec<Heading>,
    markdown_links: Vec<MarkdownLink>,
    text: String,
    raw_html: String,
    heading: Option<OpenHeading>,
    link: Option<OpenLink>,
    tasks: Vec<Task>,
    agent_comments: Vec<AgentComment>,
    task: Option<Task>,
    lines: LineCounter<'a>,
}

impl<'a> Walk<'a> {
    fn new(source: &'a str, skip: Vec<Range<usize>>) -> Self {
        Walk {
            skip,
            headings: Vec::new(),
            markdown_links: Vec::new(),
            text: String::new(),
            raw_html: String::new(),
            heading: None,
            link: None,
            tasks: Vec::new(),
            agent_comments: Vec::new(),
            task: None,
            lines: LineCounter::new(source),
        }
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(MdTag::Heading { level, .. }) => self.start_heading(level as u8, range),
            Event::End(TagEnd::Heading(_)) => self.end_heading(),
            Event::Start(MdTag::Link { dest_url, .. }) => self.start_link(&dest_url, range),
            Event::End(TagEnd::Link) => self.end_link(),
            Event::Start(MdTag::CodeBlock(_)) => self.skip.push(range),
            Event::Code(code) => self.code(&code, range),
            Event::InlineMath(_) | Event::DisplayMath(_) => self.skip.push(range),
            Event::Html(html) | Event::InlineHtml(html) => self.html(&html, range),
            Event::TaskListMarker(done) => self.start_task(done, range),
            Event::Start(MdTag::List(_)) => self.finish_task(),
            Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                self.finish_task();
                self.text.push('\n');
            }
            Event::Text(chunk) => self.chunk(&chunk),
            Event::SoftBreak | Event::HardBreak => self.line_break(),
            _ => {}
        }
    }

    fn start_heading(&mut self, level: u8, range: Range<usize>) {
        self.heading = Some(OpenHeading {
            level,
            line: self.lines.line_at(range.start),
            span: range,
            text: String::new(),
        });
    }

    fn end_heading(&mut self) {
        if let Some(open) = self.heading.take() {
            self.headings.push(Heading {
                level: open.level,
                text: open.text.trim().to_string(),
                line: open.line,
                span: open.span,
            });
        }
        self.text.push('\n');
    }

    fn start_link(&mut self, target: &str, range: Range<usize>) {
        self.link = Some(OpenLink {
            target: target.to_string(),
            span: range,
            label: String::new(),
        });
    }

    fn end_link(&mut self) {
        if let Some(open) = self.link.take()
            && !is_external(&open.target)
        {
            self.markdown_links.push(MarkdownLink {
                target: decode_target(&open.target),
                label: open.label.trim().to_string(),
                span: open.span,
            });
        }
    }

    fn code(&mut self, code: &str, range: Range<usize>) {
        self.skip.push(range);
        if let Some(task) = self.task.as_mut() {
            task.text.push_str(code);
        }
        self.text.push_str(code);
    }

    fn html(&mut self, html: &str, range: Range<usize>) {
        if let Some(note) = agent_note::parse(html) {
            let start = range.start + (html.len() - html.trim_start().len());
            self.agent_comments.push(AgentComment {
                id: note.id.map(String::from),
                anchor: note.anchor.to_string(),
                text: note.text.to_string(),
                line: self.lines.line_at(start),
                span: start..start + html.trim().len(),
            });
        }
        self.raw_html.push_str(html);
        self.skip.push(range);
    }

    fn start_task(&mut self, done: bool, range: Range<usize>) {
        self.task = Some(Task {
            text: String::new(),
            done,
            line: self.lines.line_at(range.start),
            span: range,
        });
    }

    fn finish_task(&mut self) {
        if let Some(mut found) = self.task.take() {
            found.text = unwrap_wiki_links(found.text.trim());
            self.tasks.push(found);
        }
    }

    fn chunk(&mut self, chunk: &str) {
        if let Some(task) = self.task.as_mut() {
            task.text.push_str(chunk);
        }
        if let Some(open) = self.heading.as_mut() {
            open.text.push_str(chunk);
        }
        if let Some(open) = self.link.as_mut() {
            open.label.push_str(chunk);
        }
        self.text.push_str(chunk);
    }

    fn line_break(&mut self) {
        if let Some(task) = self.task.as_mut() {
            task.text.push(' ');
        }
        self.text.push('\n');
    }
}

fn walk(source: &str, body_start: usize, skip: Vec<Range<usize>>) -> Walk<'_> {
    let mut walk = Walk::new(source, skip);
    for (event, range) in Parser::new_ext(&source[body_start..], options()).into_offset_iter() {
        walk.event(event, range.start + body_start..range.end + body_start);
    }
    walk
}

fn aliases_of(frontmatter: Option<&Frontmatter>) -> Vec<String> {
    let Some(frontmatter) = frontmatter else {
        return Vec::new();
    };
    let aliases = frontmatter.list("aliases");
    if aliases.is_empty() {
        frontmatter.list("alias")
    } else {
        aliases
    }
}

fn tag_skips(
    skip: &[Range<usize>],
    wiki_links: &[WikiLink],
    markdown_links: &[MarkdownLink],
) -> Vec<Range<usize>> {
    let mut tag_skip = skip.to_vec();
    tag_skip.extend(wiki_links.iter().map(|link| link.span.clone()));
    tag_skip.extend(markdown_links.iter().map(|link| link.span.clone()));
    tag_skip
}

pub fn parse(source: &str) -> ParsedNote {
    let (frontmatter, body_start) = split_frontmatter(source);

    let skip: Vec<Range<usize>> = frontmatter
        .iter()
        .map(|frontmatter| frontmatter.span.clone())
        .collect();
    let mut fields = frontmatter
        .as_ref()
        .map(Frontmatter::fields)
        .unwrap_or_default();

    let walked = walk(source, body_start, skip);
    take_comment_fields(&walked.raw_html, &mut fields);

    let wiki_links = wikilink::scan(source, &walked.skip);
    let blocks = scan_blocks(source, &walked.skip);
    let tag_skip = tag_skips(&walked.skip, &wiki_links, &walked.markdown_links);

    ParsedNote {
        aliases: aliases_of(frontmatter.as_ref()),
        frontmatter,
        headings: walked.headings,
        blocks,
        wiki_links,
        tags: tag::scan(source, &tag_skip),
        markdown_links: walked.markdown_links,
        tasks: walked.tasks,
        agent_comments: walked.agent_comments,
        fields,
        text: unwrap_wiki_links(&walked.text),
    }
}

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

pub fn remove_agent_comment(source: &str, line: usize, text: &str) -> Option<String> {
    let comment = parse(source)
        .agent_comments
        .into_iter()
        .find(|comment| comment.line == line && comment.text == text.trim())?;
    let line_start = source[..comment.span.start]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let line_end = source[comment.span.end..]
        .find('\n')
        .map_or(source.len(), |at| comment.span.end + at + 1);
    let alone = source[line_start..comment.span.start].trim().is_empty()
        && source[comment.span.end..line_end].trim().is_empty();
    let mut out = source.to_string();
    if alone {
        out.replace_range(line_start..line_end, "");
    } else {
        out.replace_range(comment.span.clone(), "");
    }
    Some(out)
}

pub fn remove_agent_comment_by_id(source: &str, id: &str) -> Option<String> {
    let comment = parse(source)
        .agent_comments
        .into_iter()
        .find(|comment| comment.id.as_deref() == Some(id))?;
    remove_agent_comment(source, comment.line, &comment.text)
}

pub const AGENT_ANSWER: &str = "@agent-note";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAnswer {
    pub comment: AgentComment,
    pub text: String,
    pub span: Range<usize>,
}

pub fn agent_answers(source: &str) -> Vec<AgentAnswer> {
    let mut answers = Vec::new();
    for comment in parse(source).agent_comments {
        let start = comment.span.end;
        let mut end = start;
        let mut texts = Vec::new();
        while let Some((text, next)) = answer_after(source, end) {
            texts.push(text);
            end = next;
        }
        if texts.is_empty() {
            continue;
        }
        answers.push(AgentAnswer {
            comment,
            text: texts.join("\n\n"),
            span: start..end,
        });
    }
    answers
}

fn answer_after(source: &str, at: usize) -> Option<(String, usize)> {
    let rest = &source[at..];
    let gap = rest.len() - rest.trim_start().len();
    if rest[..gap].matches('\n').count() > 1 {
        return None;
    }
    let comment = rest[gap..].strip_prefix("<!--")?;
    let close = comment.find("-->")?;
    let body = comment[..close].trim().strip_prefix(AGENT_ANSWER)?;
    if body
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace() && ch != ':')
    {
        return None;
    }
    let text = body.trim_start().strip_prefix(':').unwrap_or(body).trim();
    let mut end = at + gap + 4 + close + 3;
    let tail = &source[end..];
    let spaces = tail.len() - tail.trim_start_matches([' ', '\t']).len();
    if rest[..gap].contains('\n') && (tail[spaces..].starts_with('\n') || tail[spaces..].is_empty())
    {
        end += spaces;
    }
    Some((text.to_string(), end))
}

pub fn set_agent_comment_id(source: &str, line: usize, text: &str, id: &str) -> Option<String> {
    let comment = parse(source)
        .agent_comments
        .into_iter()
        .find(|comment| comment.line == line && comment.text == text.trim())?;
    let marked = agent_note::with_id(&source[comment.span.clone()], id)?;
    let mut out = source.to_string();
    out.replace_range(comment.span, &marked);
    Some(out)
}

fn scan_blocks(source: &str, skip: &[Range<usize>]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut start = 0;
    for (index, line) in source.split('\n').enumerate() {
        let text = line.trim_end();
        if let Some(caret) = text.rfind('^') {
            let id = &text[caret + 1..];
            let before = &text[..caret];
            let at = start + caret;
            if !id.is_empty()
                && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && (before.is_empty() || before.ends_with([' ', '\t']))
                && !skip.iter().any(|range| range.contains(&at))
            {
                blocks.push(Block {
                    id: id.to_string(),
                    line: index + 1,
                });
            }
        }
        start += line.len() + 1;
    }
    blocks
}

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

    (None, 0)
}

fn is_external(target: &str) -> bool {
    let lowercase = target.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "ftp://", "tel:"]
        .iter()
        .any(|scheme| lowercase.starts_with(scheme))
}

pub(crate) fn decode_target(target: &str) -> String {
    let source = target.as_bytes();
    let mut bytes = Vec::with_capacity(source.len());
    let mut position = 0;

    while position < source.len() {
        let hex = (source[position] == b'%')
            .then(|| source.get(position + 1..position + 3))
            .flatten()
            .and_then(hex_byte);
        match hex {
            Some(byte) => {
                bytes.push(byte);
                position += 3;
            }
            None => {
                bytes.push(source[position]);
                position += 1;
            }
        }
    }

    String::from_utf8(bytes).unwrap_or_else(|_| target.to_string())
}

fn hex_byte(pair: &[u8]) -> Option<u8> {
    let pair = std::str::from_utf8(pair).ok()?;
    if !pair.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(pair, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wikilink::WikiKind;

    #[test]
    fn decodes_percent_sequences_as_utf8() {
        assert_eq!(decode_target("%D0%9F%D0%BB%D0%B0%D0%BD.md"), "План.md");
        assert_eq!(decode_target("a%20b"), "a b");
        assert_eq!(decode_target("%FF.md"), "%FF.md");
        assert_eq!(decode_target("100%"), "100%");
        assert_eq!(decode_target("%+1"), "%+1");
    }

    #[test]
    fn encoded_non_latin_link_names_resolve() {
        let source = parse("[x](%D0%9F%D0%BB%D0%B0%D0%BD.md)");
        let plan = kknow_core::NoteId::parse("План").unwrap();
        let from = kknow_core::NoteId::parse("Index").unwrap();
        let target = source.markdown_links[0].target.clone();
        let graph = crate::graph::LinkGraph::build([
            (from.clone(), &source),
            (plan.clone(), &parse("# Plan")),
        ]);

        assert_eq!(target, "План.md");
        assert_eq!(graph.backlinks(&plan).len(), 1);
    }

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
        assert_eq!(fields["автор"], "");
        assert_eq!(fields["список"], "");
        assert!(!fields.contains_key("имя"), "вложенное не всплывает наверх");
        assert!(!fields.contains_key("# комментарий"));
    }

    #[test]
    fn aliases_come_from_an_inline_or_a_block_list() {
        let inline = parse("---\naliases: [Plan, \"Q3, draft\", '']\n---\n# Body\n");
        assert_eq!(inline.aliases, ["Plan", "Q3, draft"]);

        let block = parse("---\naliases:\n  - Plan\n  - \"The plan\"\ntags: [a]\n---\n");
        assert_eq!(block.aliases, ["Plan", "The plan"]);

        let scalar = parse("---\nalias: Plan\n---\n");
        assert_eq!(scalar.aliases, ["Plan"]);
    }

    #[test]
    fn a_list_field_ends_at_the_next_key() {
        let source = "---\naliases:\n- One\n\n- Two\nnext:\n- Not an alias\n---\n";
        let frontmatter = parse(source).frontmatter.unwrap();

        assert_eq!(frontmatter.list("aliases"), ["One", "Two"]);
        assert!(frontmatter.list("missing").is_empty());
        assert!(parse("# No frontmatter\n").aliases.is_empty());
    }

    #[test]
    fn a_note_without_frontmatter_has_no_fields() {
        let note = parse("# Просто заголовок\n");

        assert!(note.frontmatter.is_none());
        assert!(note.fields.is_empty());
    }

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
        assert_eq!(set_task(source, 4, true).as_deref(), Some(source));
    }

    #[test]
    fn set_task_refuses_a_line_without_a_task() {
        let source = "# Дела\n\n- [ ] один\n\n```\n- [ ] пример\n```\n";

        assert_eq!(set_task(source, 1, true), None, "заголовок не задача");
        assert_eq!(set_task(source, 6, true), None, "пример в коде не задача");
        assert_eq!(set_task(source, 40, true), None, "такой строки нет");
    }

    #[test]
    fn collects_agent_comments_with_lines_and_spans() {
        let source = "---\na: 1\n---\n# План\n\nПоиск быстрый<!-- @agent «быстрый»: замерить -->.\n\nАбзац целиком.<!-- @agent: сократить -->\n<!-- просто комментарий -->\n";
        let note = parse(source);

        let found: Vec<(usize, &str, &str)> = note
            .agent_comments
            .iter()
            .map(|comment| (comment.line, comment.anchor.as_str(), comment.text.as_str()))
            .collect();
        assert_eq!(found, [(6, "быстрый", "замерить"), (8, "", "сократить")]);
        assert_eq!(
            &source[note.agent_comments[0].span.clone()],
            "<!-- @agent «быстрый»: замерить -->"
        );
    }

    #[test]
    fn agent_comment_in_code_is_an_example() {
        let note = parse("```\nтекст<!-- @agent: пример -->\n```\n\n`<!-- @agent: тоже -->`\n");

        assert!(note.agent_comments.is_empty(), "{:?}", note.agent_comments);
    }

    #[test]
    fn a_comment_on_its_own_line_is_found() {
        let note = parse("# Т\n\n<!-- @agent: проверить ссылки -->\n\nтекст\n");

        assert_eq!(note.agent_comments.len(), 1);
        assert_eq!(note.agent_comments[0].line, 3);
        assert_eq!(note.agent_comments[0].text, "проверить ссылки");
    }

    #[test]
    fn remove_agent_comment_keeps_the_words() {
        let source = "Поиск быстрый<!-- @agent «быстрый»: замерить -->.\n";

        assert_eq!(
            remove_agent_comment(source, 1, "замерить").as_deref(),
            Some("Поиск быстрый.\n")
        );
    }

    #[test]
    fn remove_agent_comment_takes_a_lone_line_with_it() {
        let source = "# Т\n\n<!-- @agent: проверить -->\n\nтекст\n";

        assert_eq!(
            remove_agent_comment(source, 3, "проверить").as_deref(),
            Some("# Т\n\n\nтекст\n")
        );
    }

    #[test]
    fn remove_agent_comment_picks_the_comment_by_text() {
        let source = "а<!-- @agent «а»: раз --> б<!-- @agent «б»: два -->\n";

        assert_eq!(
            remove_agent_comment(source, 1, "два").as_deref(),
            Some("а<!-- @agent «а»: раз --> б\n")
        );
    }

    #[test]
    fn remove_agent_comment_refuses_a_stale_place() {
        let source = "текст<!-- @agent: задача -->\n";

        assert_eq!(
            remove_agent_comment(source, 2, "задача"),
            None,
            "не та строка"
        );
        assert_eq!(
            remove_agent_comment(source, 1, "другая"),
            None,
            "не тот текст"
        );
    }

    #[test]
    fn an_agent_answer_after_a_comment_is_found_and_cut() {
        let source = "demo zero<!-- @agent «zero»: start for one -->\n<!-- @agent-note: VARIANTS: 1) a 2) b -->\ndemo one\n";
        let answers = agent_answers(source);

        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].comment.text, "start for one");
        assert_eq!(answers[0].text, "VARIANTS: 1) a 2) b");
        let mut cut = source.to_string();
        cut.replace_range(answers[0].span.clone(), "");
        assert_eq!(
            cut,
            "demo zero<!-- @agent «zero»: start for one -->\ndemo one\n"
        );
    }

    #[test]
    fn answers_on_the_same_line_join() {
        let source = "x<!-- @agent: do it --> <!-- @agent-note: why? --><!-- @agent-note: or this -->\nnext\n";
        let answers = agent_answers(source);

        assert_eq!(answers[0].text, "why?\n\nor this");
        let mut cut = source.to_string();
        cut.replace_range(answers[0].span.clone(), "");
        assert_eq!(cut, "x<!-- @agent: do it -->\nnext\n");
    }

    #[test]
    fn a_note_after_an_empty_line_or_alone_is_not_an_answer() {
        assert!(
            agent_answers("x<!-- @agent: do it -->\n\n<!-- @agent-note: other -->\n").is_empty()
        );
        assert!(agent_answers("text\n<!-- @agent-note: cannot do it -->\n").is_empty());
        assert!(agent_answers("x<!-- @agent: do it -->\n<!-- @agent-notes: no -->\n").is_empty());
    }

    #[test]
    fn headings_know_their_lines_with_frontmatter() {
        let note = parse("---\ntitle: x\n---\n# Top\n\ntext\n\n## Below\n");

        let lines: Vec<_> = note
            .headings
            .iter()
            .map(|h| (h.text.as_str(), h.line))
            .collect();
        assert_eq!(lines, [("Top", 4), ("Below", 8)]);
    }

    #[test]
    fn block_ids_end_a_line_after_a_space() {
        let note = parse("A paragraph ^intro\n\n- item ^item-2\n\nx^2 is math\nonly^tail\n");

        let blocks: Vec<_> = note
            .blocks
            .iter()
            .map(|b| (b.id.as_str(), b.line))
            .collect();
        assert_eq!(blocks, [("intro", 1), ("item-2", 3)]);
    }

    #[test]
    fn a_block_id_in_code_or_frontmatter_is_text() {
        let note = parse("---\nkey: a ^meta\n---\n```\ncode ^nope\n```\n`inline ^no`\nreal ^yes\n");

        let ids: Vec<_> = note.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["yes"]);
    }
}
