pub fn matches(pattern: &str, path: &str) -> bool {
    walk(&parts(pattern), &parts(path))
}

fn parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

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

fn segment(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();

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
