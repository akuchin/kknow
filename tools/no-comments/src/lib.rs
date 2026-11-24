use std::ops::Range;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Syntax {
    Rust,
    Css,
}

impl Syntax {
    pub fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            "css" => Some(Self::Css),
            _ => None,
        }
    }
}

const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "test-results",
    "playwright-report",
    "gen",
    "cache",
];

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root exists")
}

pub fn checked_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && SKIPPED_DIRS.contains(&name.as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && Syntax::of(entry.path()).is_some())
        .map(|entry| entry.into_path())
        .collect();
    files.sort();
    files
}

pub fn find_comments(source: &str, syntax: Syntax) -> Vec<Range<usize>> {
    Lexer {
        source,
        bytes: source.as_bytes(),
        syntax,
    }
    .comments()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    syntax: Syntax,
}

impl Lexer<'_> {
    fn comments(&self) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        let mut at = 0;
        while at < self.bytes.len() {
            let next = self.byte(at + 1);
            at = match self.bytes[at] {
                b'/' if next == Some(b'/') && self.syntax == Syntax::Rust => {
                    let end = self.line_end(at);
                    found.push(at..end);
                    end
                }
                b'/' if next == Some(b'*') => {
                    let end = self.block_comment_end(at);
                    found.push(at..end);
                    end
                }
                b'"' => self.quoted_end(at),
                b'\'' if self.syntax == Syntax::Css => self.quoted_end(at),
                b'\'' => self.char_or_lifetime_end(at),
                b'r' | b'b' | b'c' if self.syntax == Syntax::Rust && !self.ident_before(at) => {
                    self.prefixed_literal_end(at)
                }
                _ => at + 1,
            };
        }
        found
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.bytes.get(at).copied()
    }

    fn line_end(&self, from: usize) -> usize {
        let end = self.bytes[from..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(self.bytes.len(), |offset| from + offset);
        if end > from && self.bytes[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    fn block_comment_end(&self, from: usize) -> usize {
        let nests = self.syntax == Syntax::Rust;
        let mut depth = 0;
        let mut at = from;
        while at < self.bytes.len() {
            match (self.bytes[at], self.byte(at + 1)) {
                (b'/', Some(b'*')) if depth == 0 || nests => {
                    depth += 1;
                    at += 2;
                }
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        return at;
                    }
                }
                _ => at += 1,
            }
        }
        self.bytes.len()
    }

    fn quoted_end(&self, from: usize) -> usize {
        let quote = self.bytes[from];
        let mut at = from + 1;
        while at < self.bytes.len() {
            match self.bytes[at] {
                b'\\' => at += 2,
                byte if byte == quote => return at + 1,
                _ => at += 1,
            }
        }
        self.bytes.len()
    }

    fn char_or_lifetime_end(&self, from: usize) -> usize {
        if self.byte(from + 1) == Some(b'\\') {
            return self.quoted_end(from);
        }
        let Some(first) = self.source[from + 1..].chars().next() else {
            return self.bytes.len();
        };
        let after = from + 1 + first.len_utf8();
        if self.byte(after) == Some(b'\'') {
            after + 1
        } else {
            from + 1
        }
    }

    fn prefixed_literal_end(&self, from: usize) -> usize {
        let mut at = from;
        let has_prefix = matches!(self.bytes[at], b'b' | b'c');
        if has_prefix {
            at += 1;
        }
        if self.byte(at) == Some(b'r') {
            let hashes = self.bytes[at + 1..]
                .iter()
                .take_while(|&&byte| byte == b'#')
                .count();
            let quote = at + 1 + hashes;
            if self.byte(quote) == Some(b'"') {
                return self.raw_string_end(quote, hashes);
            }
        } else if has_prefix && self.byte(at) == Some(b'"') {
            return self.quoted_end(at);
        } else if has_prefix && self.byte(at) == Some(b'\'') {
            return self.char_or_lifetime_end(at);
        }
        self.identifier_end(from)
    }

    fn raw_string_end(&self, quote: usize, hashes: usize) -> usize {
        let closing: String = std::iter::once('"')
            .chain(std::iter::repeat_n('#', hashes))
            .collect();
        self.source[quote + 1..]
            .find(&closing)
            .map_or(self.bytes.len(), |offset| {
                quote + 1 + offset + closing.len()
            })
    }

    fn identifier_end(&self, from: usize) -> usize {
        from + self.bytes[from..]
            .iter()
            .take_while(|&&byte| is_identifier_byte(byte))
            .count()
            .max(1)
    }

    fn ident_before(&self, at: usize) -> bool {
        at > 0 && is_identifier_byte(self.bytes[at - 1])
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

const REMOVED_LINE: &str = "\u{0}";

pub fn strip(source: &str, comments: &[Range<usize>]) -> String {
    let without_comments = cut_comments(source, &merge_same_line(source, comments));
    drop_lines_left_by_comments(&without_comments)
}

fn merge_same_line(source: &str, comments: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::new();
    for comment in comments {
        if let Some(last) = merged.last_mut() {
            let gap = &source[last.end..comment.start];
            if gap.chars().all(|ch| ch == ' ' || ch == '\t') {
                last.end = comment.end;
                continue;
            }
        }
        merged.push(comment.clone());
    }
    merged
}

fn cut_comments(source: &str, comments: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for comment in comments {
        let line_start = source[..comment.start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = source[comment.end..]
            .find('\n')
            .map_or(source.len(), |at| comment.end + at);
        let blank_before = source[line_start..comment.start].trim().is_empty();
        let blank_after = source[comment.end..line_end].trim().is_empty();
        let spaces_before = comment.start - trim_spaces_back(source, comment.start);
        let spaces_after = skip_spaces(source, comment.end) - comment.end;

        if blank_before && blank_after {
            out.push_str(&source[cursor..line_start]);
            out.push_str(REMOVED_LINE);
            if line_end < source.len() {
                out.push('\n');
            }
            cursor = (line_end + 1).min(source.len());
        } else if blank_after {
            out.push_str(&source[cursor..comment.start - spaces_before]);
            cursor = comment.end;
        } else if blank_before {
            out.push_str(&source[cursor..comment.start]);
            cursor = comment.end + spaces_after;
        } else {
            let left = comment.start - spaces_before;
            let right = comment.end + spaces_after;
            out.push_str(&source[cursor..left]);
            if needs_space(source, left, right, spaces_before + spaces_after > 0) {
                out.push(' ');
            }
            cursor = right;
        }
    }
    out.push_str(&source[cursor..]);
    out
}

fn needs_space(source: &str, left: usize, right: usize, had_space: bool) -> bool {
    let before = source[..left].chars().next_back();
    let after = source[right..].chars().next();
    had_space
        && !before.is_some_and(|ch| "([{".contains(ch))
        && !after.is_some_and(|ch| ")]},;.".contains(ch))
}

fn trim_spaces_back(source: &str, from: usize) -> usize {
    from - source[..from].len() + source[..from].trim_end_matches([' ', '\t']).len()
}

fn skip_spaces(source: &str, from: usize) -> usize {
    source.len() - source[from..].trim_start_matches([' ', '\t']).len()
}

fn drop_lines_left_by_comments(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut at = 0;
    while at < lines.len() {
        if lines[at] != REMOVED_LINE {
            kept.push(lines[at]);
            at += 1;
            continue;
        }
        while at < lines.len() && lines[at] == REMOVED_LINE {
            at += 1;
        }
        let previous = kept.last().copied();
        let next = lines.get(at).copied();
        let next_is_last_empty = at + 1 == lines.len() && next == Some("");
        if next.is_some_and(|line| is_blank(line) && !next_is_last_empty)
            && previous.is_none_or(|line| is_blank(line) || opens_block(line))
        {
            at += 1;
        } else if previous.is_some_and(is_blank)
            && next.is_none_or(|line| closes_block(line) || next_is_last_empty)
        {
            kept.pop();
        }
    }
    kept.join("\n")
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn opens_block(line: &str) -> bool {
    line.trim_end().ends_with(['{', '(', '['])
}

fn closes_block(line: &str) -> bool {
    line.trim_start().starts_with(['}', ')', ']'])
}

pub fn line_of(source: &str, offset: usize) -> usize {
    source[..offset].matches('\n').count() + 1
}
