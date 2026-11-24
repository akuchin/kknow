use kknow_no_comments::{Syntax, checked_files, find_comments, line_of, strip, workspace_root};

fn comments(source: &str) -> Vec<&str> {
    find_comments(source, Syntax::Rust)
        .into_iter()
        .map(|range| &source[range])
        .collect()
}

#[test]
fn the_repository_has_no_comments() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    for path in checked_files(&root) {
        let syntax = Syntax::of(&path).expect("checked files have a known syntax");
        let source = std::fs::read_to_string(&path).expect("a checked file is readable");
        for comment in find_comments(&source, syntax) {
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            offenders.push(format!("{}:{}", relative.display(), line_of(&source, comment.start)));
        }
    }
    assert!(
        offenders.is_empty(),
        "comments are not allowed; express the intent in code or in ARCHITECTURE.md.\n\
         `pnpm comments:strip` removes them.\n{}",
        offenders.join("\n")
    );
}

#[test]
fn finds_line_block_and_doc_comments() {
    let source = "//! crate\n/// item\nfn a() {} // tail\n/* block /* nested */ still */ fn b() {}\n";
    assert_eq!(
        comments(source),
        ["//! crate", "/// item", "// tail", "/* block /* nested */ still */"]
    );
}

#[test]
fn ignores_comment_markers_inside_literals() {
    let source = concat!(
        "let url = \"http://x\";\n",
        "let raw = r#\"a \"// b\" /* c */\"#;\n",
        "let bytes = b\"//\";\n",
        "let slash = '/';\n",
        "let quote = '\\'';\n",
        "fn f<'a>(x: &'a str) -> &'a str { x }\n",
        "let r#type = 1;\n",
    );
    assert!(comments(source).is_empty());
}

#[test]
fn css_has_only_block_comments() {
    let source = "a { background: url(//x.png); /* note */ }\n";
    let found: Vec<&str> = find_comments(source, Syntax::Css)
        .into_iter()
        .map(|range| &source[range])
        .collect();
    assert_eq!(found, ["/* note */"]);
}

#[test]
fn strip_removes_whole_lines_and_the_blank_line_they_leave() {
    let source = "//! crate\n\nuse x;\n\n/// doc\nfn a() {\n    // inside\n\n    b(); // tail\n}\n";
    let stripped = strip(source, &find_comments(source, Syntax::Rust));
    assert_eq!(stripped, "use x;\n\nfn a() {\n    b();\n}\n");
}

#[test]
fn strip_keeps_code_around_inline_block_comments() {
    let source = "call(/* a */ x, y /* b */);\nlet z = 1 /* c */ + 2;\n";
    let stripped = strip(source, &find_comments(source, Syntax::Rust));
    assert_eq!(stripped, "call(x, y);\nlet z = 1 + 2;\n");
}

#[test]
fn strip_removes_a_blank_line_before_a_closing_brace() {
    let source = "fn a() {\n    b();\n\n    // last\n}\n";
    let stripped = strip(source, &find_comments(source, Syntax::Rust));
    assert_eq!(stripped, "fn a() {\n    b();\n}\n");
}

#[test]
fn strip_keeps_crlf_line_endings() {
    let source = "a(); // x\r\nb();\r\n";
    let stripped = strip(source, &find_comments(source, Syntax::Rust));
    assert_eq!(stripped, "a();\r\nb();\r\n");
}
