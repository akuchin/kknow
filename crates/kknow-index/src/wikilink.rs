use std::ops::Range;

/// Part of the link target after `#`: `[[note#Heading]]` or `[[note#^block-id]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subpath {
    Heading(String),
    Block(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiKind {
    /// `[[note]]`: a plain link.
    Link,
    /// `![[note]]`: embeds the content.
    Embed,
}

/// A `[[…]]` wikilink, exactly as written in the text.
///
/// The target is not normalized or resolved: the graph does this, because it
/// knows the note list. The original text and `span` are kept so that links
/// can be rewritten when a note is renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// Target as written. Empty for links inside the current note, like
    /// `[[#Heading]]`.
    pub target: String,
    pub subpath: Option<Subpath>,
    /// Text after `|`, if present.
    pub alias: Option<String>,
    pub kind: WikiKind,
    /// Range in the source text, with the brackets and the leading `!` of an embed.
    pub span: Range<usize>,
}

impl WikiLink {
    /// The link points to another place in the same note.
    pub fn is_internal(&self) -> bool {
        self.target.is_empty()
    }

    /// Text to show: the alias, else the target, else the subpath.
    pub fn display_text(&self) -> &str {
        if let Some(alias) = &self.alias {
            return alias;
        }
        if !self.target.is_empty() {
            return &self.target;
        }
        match &self.subpath {
            Some(Subpath::Heading(heading)) => heading,
            Some(Subpath::Block(block)) => block,
            None => "",
        }
    }
}

/// Finds all `[[wikilinks]]` in the text and skips the `skip` ranges
/// (code, formulas, frontmatter).
///
/// The scan reads the raw text, not markdown parser events: `[[...]]` is not
/// part of CommonMark, and the parser can split it into several text events.
pub fn scan(source: &str, skip: &[Range<usize>]) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = source[cursor..].find("[[") {
        let open = cursor + offset;
        cursor = open + 2;

        if is_skipped(open, skip) {
            continue;
        }

        // A link does not continue on the next line.
        let line_end = source[cursor..]
            .find('\n')
            .map_or(source.len(), |end| cursor + end);
        let Some(offset) = source[cursor..line_end].find("]]") else {
            continue;
        };
        let close = cursor + offset;

        let inner = &source[cursor..close];
        // A nested `[[` means the real link start is further right.
        if inner.contains('[') {
            continue;
        }

        let is_embed = source[..open].ends_with('!');
        let start = if is_embed { open - 1 } else { open };

        if let Some(link) = parse_inner(inner, is_embed, start..close + 2) {
            links.push(link);
            cursor = close + 2;
        }
    }

    links
}

fn parse_inner(inner: &str, is_embed: bool, span: Range<usize>) -> Option<WikiLink> {
    // The first `|` starts the alias: `[[a|b|c]]` -> alias "b|c".
    let (path, alias) = match inner.split_once('|') {
        Some((path, alias)) => (path, non_empty(alias)),
        None => (inner, None),
    };

    let (target, subpath) = match path.split_once('#') {
        Some((target, sub)) => (target.trim(), parse_subpath(sub)),
        None => (path.trim(), None),
    };

    if target.is_empty() && subpath.is_none() {
        return None;
    }

    Some(WikiLink {
        target: target.to_string(),
        subpath,
        alias,
        kind: if is_embed {
            WikiKind::Embed
        } else {
            WikiKind::Link
        },
        span,
    })
}

fn parse_subpath(raw: &str) -> Option<Subpath> {
    let raw = raw.trim();
    match raw.strip_prefix('^') {
        Some(block) => non_empty(block).map(Subpath::Block),
        // Nested headings `#a#b` are kept whole for now.
        None => non_empty(raw).map(Subpath::Heading),
    }
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_skipped(position: usize, skip: &[Range<usize>]) -> bool {
    skip.iter().any(|range| range.contains(&position))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_all(source: &str) -> Vec<WikiLink> {
        scan(source, &[])
    }

    fn targets(source: &str) -> Vec<String> {
        scan_all(source).into_iter().map(|l| l.target).collect()
    }

    #[test]
    fn finds_plain_links() {
        assert_eq!(
            targets("см. [[Rust]] и [[Projects/Tauri]] сегодня"),
            ["Rust", "Projects/Tauri"]
        );
    }

    #[test]
    fn parses_alias_heading_and_block() {
        let links = scan_all("[[note|как звать]] [[note#Установка]] [[note#^a1b2]]");

        assert_eq!(links[0].alias.as_deref(), Some("как звать"));
        assert_eq!(links[0].subpath, None);
        assert_eq!(
            links[1].subpath,
            Some(Subpath::Heading("Установка".to_string()))
        );
        assert_eq!(links[2].subpath, Some(Subpath::Block("a1b2".to_string())));
    }

    #[test]
    fn parses_heading_together_with_alias() {
        let link = &scan_all("[[note#Установка|как ставить]]")[0];

        assert_eq!(link.target, "note");
        assert_eq!(link.subpath, Some(Subpath::Heading("Установка".into())));
        assert_eq!(link.alias.as_deref(), Some("как ставить"));
        assert_eq!(link.display_text(), "как ставить");
    }

    #[test]
    fn recognizes_embeds() {
        let links = scan_all("![[diagram]] и [[diagram]]");

        assert_eq!(links[0].kind, WikiKind::Embed);
        assert_eq!(links[1].kind, WikiKind::Link);
        // The span of an embed includes the `!`.
        assert_eq!(
            &"![[diagram]] и [[diagram]]"[links[0].span.clone()],
            "![[diagram]]"
        );
    }

    #[test]
    fn link_into_the_same_note_has_empty_target() {
        let link = &scan_all("[[#Итоги]]")[0];

        assert!(link.is_internal());
        assert_eq!(link.subpath, Some(Subpath::Heading("Итоги".into())));
        assert_eq!(link.display_text(), "Итоги");
    }

    #[test]
    fn span_points_at_the_original_text() {
        let source = "начало [[Rust]] конец";
        let link = &scan_all(source)[0];

        assert_eq!(&source[link.span.clone()], "[[Rust]]");
    }

    #[test]
    fn skips_ranges_marked_as_code() {
        let source = "`[[NotALink]]` но [[Real]]";
        let code = 0..14;

        assert_eq!(
            scan(source, &[code])
                .into_iter()
                .map(|l| l.target)
                .collect::<Vec<_>>(),
            ["Real"]
        );
    }

    #[test]
    fn ignores_broken_syntax() {
        assert!(targets("[[незакрытая ссылка").is_empty());
        assert!(targets("[[]]").is_empty());
        assert!(targets("[[перенос\nстроки]]").is_empty());
        assert!(targets("[одинарные скобки]").is_empty());
    }

    #[test]
    fn trims_whitespace_around_parts() {
        let link = &scan_all("[[  note  |  алиас  ]]")[0];

        assert_eq!(link.target, "note");
        assert_eq!(link.alias.as_deref(), Some("алиас"));
    }

    #[test]
    fn empty_alias_is_dropped() {
        assert_eq!(scan_all("[[note|]]")[0].alias, None);
    }

    #[test]
    fn handles_multibyte_text_around_links() {
        let source = "заметка про [[Раст]] — конец";
        let link = &scan_all(source)[0];

        assert_eq!(link.target, "Раст");
        assert_eq!(&source[link.span.clone()], "[[Раст]]");
    }
}
