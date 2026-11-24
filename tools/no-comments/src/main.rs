use std::process::ExitCode;

use kknow_no_comments::{Syntax, checked_files, find_comments, strip, workspace_root};

fn main() -> ExitCode {
    let root = workspace_root();
    let mut changed = 0;
    for path in checked_files(&root) {
        let Some(syntax) = Syntax::of(&path) else {
            continue;
        };
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let comments = find_comments(&source, syntax);
        if comments.is_empty() {
            continue;
        }
        if let Err(error) = std::fs::write(&path, strip(&source, &comments)) {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
        changed += 1;
        println!("{}", path.strip_prefix(&root).unwrap_or(&path).display());
    }
    println!("comments removed from {changed} files");
    ExitCode::SUCCESS
}
