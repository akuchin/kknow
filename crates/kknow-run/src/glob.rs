//! Path patterns: `Docs/**`, `**/*.md`, `Journal/2026-??-*`.
//!
//! No library is used: the pattern language is small, and its tests should
//! stay next to the code that uses it. The same reason applies to the app's
//! own icons.

/// Checks if a path matches a pattern.
///
/// The path is relative to the vault root and uses `/` on all platforms. We
/// compare what the manifest says, not the system path separator.
pub fn matches(pattern: &str, path: &str) -> bool {
    walk(&parts(pattern), &parts(path))
}

/// Names without empty parts: `Docs//a` and `Docs/a` are the same path.
fn parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

/// Compares level by level: `**` takes any number of levels, including zero.
///
/// So `Docs/**` also matches the `Docs` folder itself, not only its contents:
/// a command on a subtree also applies to its root.
fn walk(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| walk(rest, &path[skip..])),
        Some((head, rest)) => match path.split_first() {
            Some((first, tail)) => segment(head, first) && walk(rest, tail),
            None => false,
        },
    }
}

/// One name: `*` is any number of characters, `?` is exactly one. Neither
/// crosses `/`; use `**` for that.
fn segment(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();

    // Backtracking: at `*`, remember the position. On a mismatch, give the
    // star one more character.
    let (mut at, mut seen) = (0, 0);
    let (mut star, mut taken) = (None, 0);

    while seen < name.len() {
        if at < pattern.len() && (pattern[at] == '?' || pattern[at] == name[seen]) {
            at += 1;
            seen += 1;
        } else if at < pattern.len() && pattern[at] == '*' {
            star = Some(at);
            at += 1;
            taken = seen;
        } else if let Some(back) = star {
            at = back + 1;
            taken += 1;
            seen = taken;
        } else {
            return false;
        }
    }

    // A tail of only stars matches zero characters.
    pattern[at..].iter().all(|symbol| *symbol == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_matches_itself() {
        assert!(matches("Index.md", "Index.md"));
        assert!(!matches("Index.md", "Другое.md"));
        assert!(
            !matches("Index.md", "Docs/Index.md"),
            "уровень имеет значение"
        );
    }

    #[test]
    fn a_star_stays_inside_one_name() {
        assert!(matches("*.md", "Index.md"));
        assert!(matches("Docs/*.md", "Docs/Index.md"));
        assert!(!matches("Docs/*.md", "Docs/Внутри/Index.md"));
        assert!(matches("2026-*", "2026-07-26"));
    }

    #[test]
    fn a_question_mark_is_exactly_one_symbol() {
        assert!(matches("2026-??-26", "2026-07-26"));
        assert!(!matches("2026-?-26", "2026-07-26"));
    }

    #[test]
    fn a_double_star_crosses_levels_and_may_take_none() {
        assert!(matches("Docs/**", "Docs/Внутри/Index.md"));
        assert!(
            matches("Docs/**", "Docs"),
            "команда на ветке относится и к её корню"
        );
        assert!(matches("**/*.md", "Index.md"));
        assert!(matches("**/*.md", "Docs/Внутри/Index.md"));
        assert!(!matches("**/*.md", "Docs/Внутри"));
    }

    #[test]
    fn backtracking_does_not_give_up_too_early() {
        // The first star must give back `a`, or the tail `ab` does not match.
        assert!(matches("*ab", "aab"));
        assert!(matches("*a*b", "xxaxxb"));
        assert!(!matches("*ab", "aba"));
    }

    #[test]
    fn empty_parts_do_not_change_the_path() {
        assert!(matches("Docs//Index.md", "Docs/Index.md"));
        assert!(
            matches("**", ""),
            "хранилищу целиком подходит любой образец"
        );
    }
}
