use std::ops::Range;

/// A tag like `#rust` or `#projects/study`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Name without the leading `#`.
    pub name: String,
    pub span: Range<usize>,
}

/// Finds tags in the text and skips the `skip` ranges.
///
/// A tag is `#` followed directly by a non-space character. This rule also
/// tells it from a markdown heading `# Heading`. A `#` inside a word
/// (`foo#bar`, URL anchors) is not a tag.
pub fn scan(source: &str, skip: &[Range<usize>]) -> Vec<Tag> {
    let mut tags = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = source[cursor..].find('#') {
        let hash = cursor + offset;
        cursor = hash + 1;

        if skip.iter().any(|range| range.contains(&hash)) {
            continue;
        }
        if !preceded_by_boundary(source, hash) {
            continue;
        }

        let name: String = source[cursor..].chars().take_while(is_tag_char).collect();
        if name.is_empty() || !name.chars().any(char::is_alphabetic) {
            continue;
        }
        // A trailing slash belongs to the `#tag/` form, not to the name.
        let name = name.trim_end_matches('/').to_string();

        let end = cursor + name.len();
        tags.push(Tag {
            name,
            span: hash..end,
        });
        cursor = end;
    }

    tags
}

fn preceded_by_boundary(source: &str, hash: usize) -> bool {
    match source[..hash].chars().next_back() {
        None => true,
        Some(previous) => !previous.is_alphanumeric() && !matches!(previous, '#' | '/' | '_' | '-'),
    }
}

fn is_tag_char(ch: &char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<String> {
        scan(source, &[]).into_iter().map(|tag| tag.name).collect()
    }

    #[test]
    fn finds_simple_and_nested_tags() {
        assert_eq!(
            names("учу #rust и #проекты/учёба сегодня"),
            ["rust", "проекты/учёба"]
        );
    }

    #[test]
    fn ignores_markdown_headings() {
        assert_eq!(
            names("# Заголовок\n## Подзаголовок\ntext"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn ignores_hash_inside_words_and_urls() {
        assert_eq!(
            names("https://example.com/page#anchor"),
            Vec::<String>::new()
        );
        assert_eq!(names("issue42#comment"), Vec::<String>::new());
    }

    #[test]
    fn ignores_purely_numeric_tags() {
        assert_eq!(names("дом #1 и #2026"), Vec::<String>::new());
    }

    #[test]
    fn stops_at_punctuation() {
        let tags = scan("тег #rust, потом #tauri.", &[]);

        assert_eq!(tags[0].name, "rust");
        assert_eq!(tags[1].name, "tauri");
        assert_eq!(&"тег #rust, потом #tauri."[tags[0].span.clone()], "#rust");
    }

    #[test]
    fn allows_tag_after_bracket() {
        assert_eq!(names("(#rust)"), ["rust"]);
    }

    #[test]
    fn respects_skip_ranges() {
        let source = "`#code` и #real";
        let code = 0..7;

        let tags = scan(source, &[code]);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "real");
    }
}
