use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::NoteId;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const BOOKMARKS_PATH: &str = ".kknow/bookmarks.json";

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub note: String,
    pub line: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "BookmarkMark"))]
pub struct Mark {
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub name: Option<String>,
}

impl Mark {
    pub fn new(line: usize, name: Option<&str>) -> Self {
        Mark {
            line,
            name: clean_name(name),
        }
    }
}

pub fn clean_name(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmarks {
    #[serde(default = "current_version")]
    version: u32,
    #[serde(default)]
    bookmarks: Vec<Bookmark>,
}

fn current_version() -> u32 {
    VERSION
}

impl Default for Bookmarks {
    fn default() -> Self {
        Bookmarks {
            version: VERSION,
            bookmarks: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Loaded {
    pub bookmarks: Bookmarks,
    pub problem: Option<String>,
}

impl Bookmark {
    pub fn locate(&self, content: &str) -> Option<usize> {
        content
            .lines()
            .enumerate()
            .filter(|(_, text)| *text == self.text)
            .map(|(index, _)| index + 1)
            .min_by_key(|line| line.abs_diff(self.line))
    }

    fn belongs_to(&self, note: &NoteId) -> bool {
        self.note == note.as_str()
    }
}

fn line_text(content: &str, line: usize) -> Option<&str> {
    content.lines().nth(line.checked_sub(1)?)
}

impl Bookmarks {
    pub fn path(vault_root: impl AsRef<Path>) -> PathBuf {
        vault_root.as_ref().join(BOOKMARKS_PATH)
    }

    pub fn load(path: &Path) -> Result<Loaded> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Loaded {
                    bookmarks: Bookmarks::default(),
                    problem: None,
                });
            }
            Err(source) => return Err(Error::io(path, source)),
        };

        match serde_json::from_slice::<Bookmarks>(&bytes) {
            Ok(bookmarks) => Ok(Loaded {
                bookmarks,
                problem: None,
            }),
            Err(error) => {
                let broken = path.with_extension("json.broken");
                let _ = fs::rename(path, &broken);
                Ok(Loaded {
                    bookmarks: Bookmarks::default(),
                    problem: Some(format!(
                        "cannot parse the bookmarks ({error}); the file is moved to {}",
                        broken.display()
                    )),
                })
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("bookmarks serialize");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    pub fn merged(&self, theirs: &Bookmarks) -> Bookmarks {
        let mut out = self.clone();
        for bookmark in &theirs.bookmarks {
            let same = out.bookmarks.iter_mut().find(|ours| {
                ours.note == bookmark.note
                    && ours.line == bookmark.line
                    && ours.text == bookmark.text
            });
            match same {
                Some(ours) => {
                    if ours.name.is_none() {
                        ours.name = bookmark.name.clone();
                    }
                }
                None => out.bookmarks.push(bookmark.clone()),
            }
        }
        out
    }

    pub fn all(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    pub fn for_note(&self, note: &NoteId) -> Vec<&Bookmark> {
        let mut found: Vec<&Bookmark> = self
            .bookmarks
            .iter()
            .filter(|bookmark| bookmark.belongs_to(note))
            .collect();
        found.sort_by_key(|bookmark| bookmark.line);
        found
    }

    pub fn toggle(&mut self, note: &NoteId, line: usize, content: &str) -> bool {
        let Some(text) = line_text(content, line) else {
            return false;
        };
        let existing = self
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.belongs_to(note) && bookmark.line == line);
        match existing {
            Some(index) => {
                self.bookmarks.remove(index);
                false
            }
            None => {
                self.bookmarks.push(Bookmark {
                    note: note.as_str().to_string(),
                    line,
                    text: text.to_string(),
                    name: None,
                });
                true
            }
        }
    }

    pub fn rename_mark(&mut self, note: &NoteId, line: usize, name: Option<&str>) -> bool {
        let found = self
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.belongs_to(note) && bookmark.line == line);
        match found {
            Some(bookmark) => {
                bookmark.name = clean_name(name);
                true
            }
            None => false,
        }
    }

    pub fn reanchor(&mut self, note: &NoteId, content: &str) {
        for bookmark in self.bookmarks.iter_mut().filter(|b| b.belongs_to(note)) {
            if let Some(line) = bookmark.locate(content) {
                bookmark.line = line;
            }
        }
    }

    pub fn place(&mut self, note: &NoteId, marks: &[Mark], content: &str) {
        let mut marks: Vec<Mark> = marks
            .iter()
            .filter(|mark| line_text(content, mark.line).is_some())
            .map(|mark| Mark::new(mark.line, mark.name.as_deref()))
            .collect();
        marks.sort_by_key(|mark| (mark.line, mark.name.is_none()));
        marks.dedup_by_key(|mark| mark.line);
        let mut marks = marks.into_iter();

        let mut slots: Vec<usize> = (0..self.bookmarks.len())
            .filter(|&index| self.bookmarks[index].belongs_to(note))
            .collect();
        slots.sort_by_key(|&index| self.bookmarks[index].line);

        let mut gone = Vec::new();
        for index in slots {
            match marks.next() {
                Some(mark) => {
                    let bookmark = &mut self.bookmarks[index];
                    bookmark.text = line_text(content, mark.line)
                        .unwrap_or_default()
                        .to_string();
                    bookmark.line = mark.line;
                    bookmark.name = mark.name;
                }
                None => gone.push(index),
            }
        }
        gone.sort_unstable();
        for index in gone.into_iter().rev() {
            self.bookmarks.remove(index);
        }
        for mark in marks {
            self.bookmarks.push(Bookmark {
                note: note.as_str().to_string(),
                line: mark.line,
                text: line_text(content, mark.line)
                    .unwrap_or_default()
                    .to_string(),
                name: mark.name,
            });
        }
    }

    pub fn rename(&mut self, moves: &[(NoteId, NoteId)]) {
        for bookmark in &mut self.bookmarks {
            if let Some((_, to)) = moves.iter().find(|(from, _)| bookmark.belongs_to(from)) {
                bookmark.note = to.as_str().to_string();
            }
        }
    }

    pub fn forget(&mut self, note: &NoteId) -> usize {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|bookmark| !bookmark.belongs_to(note));
        before - self.bookmarks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: &str) -> NoteId {
        NoteId::parse(raw).unwrap()
    }

    fn mark(note: &str, line: usize, text: &str) -> Bookmark {
        Bookmark {
            note: note.to_string(),
            line,
            text: text.to_string(),
            name: None,
        }
    }

    fn named(note: &str, line: usize, text: &str, name: &str) -> Bookmark {
        Bookmark {
            name: Some(name.to_string()),
            ..mark(note, line, text)
        }
    }

    fn lines(lines: &[usize]) -> Vec<Mark> {
        lines.iter().map(|&line| Mark::new(line, None)).collect()
    }

    const PLAN: &str = "# План\n\nвыучить владение\nпотрогать tantivy\n";

    #[test]
    fn toggle_sets_a_bookmark_with_the_text_of_its_line() {
        let mut bookmarks = Bookmarks::default();

        assert!(bookmarks.toggle(&id("план"), 3, PLAN));

        assert_eq!(bookmarks.all(), [mark("план", 3, "выучить владение")]);
    }

    #[test]
    fn toggle_twice_removes_the_bookmark() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.toggle(&id("план"), 3, PLAN);
        assert!(!bookmarks.toggle(&id("план"), 3, PLAN));

        assert!(bookmarks.all().is_empty());
    }

    #[test]
    fn toggle_outside_the_text_changes_nothing() {
        let mut bookmarks = Bookmarks::default();

        assert!(
            !bookmarks.toggle(&id("план"), 0, PLAN),
            "строки считаются с единицы"
        );
        assert!(
            !bookmarks.toggle(&id("план"), 5, PLAN),
            "в плане четыре строки"
        );

        assert!(bookmarks.all().is_empty());
    }

    #[test]
    fn toggle_does_not_see_the_line_ending() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.toggle(&id("windows"), 2, "первая\r\nвторая\r\n");

        assert_eq!(bookmarks.all()[0].text, "вторая");
    }

    #[test]
    fn the_same_line_of_another_note_is_another_bookmark() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.toggle(&id("план"), 3, PLAN);
        assert!(bookmarks.toggle(&id("архив/план"), 3, PLAN));

        assert_eq!(bookmarks.all().len(), 2);
    }

    #[test]
    fn all_keeps_the_order_bookmarks_were_set_in() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.toggle(&id("план"), 4, PLAN);
        bookmarks.toggle(&id("другая"), 1, "текст\n");
        bookmarks.toggle(&id("план"), 1, PLAN);

        let lines: Vec<(&str, usize)> = bookmarks
            .all()
            .iter()
            .map(|bookmark| (bookmark.note.as_str(), bookmark.line))
            .collect();
        assert_eq!(lines, [("план", 4), ("другая", 1), ("план", 1)]);
    }

    #[test]
    fn for_note_gives_only_that_note_sorted_by_line() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 4, PLAN);
        bookmarks.toggle(&id("другая"), 1, "текст\n");
        bookmarks.toggle(&id("план"), 1, PLAN);

        let lines: Vec<usize> = bookmarks
            .for_note(&id("план"))
            .iter()
            .map(|bookmark| bookmark.line)
            .collect();

        assert_eq!(lines, [1, 4]);
        assert!(bookmarks.for_note(&id("нет такой")).is_empty());
    }

    #[test]
    fn a_line_in_place_is_found_in_place() {
        let bookmark = mark("план", 3, "выучить владение");

        assert_eq!(bookmark.locate(PLAN), Some(3));
    }

    #[test]
    fn a_line_pushed_down_is_found_lower() {
        let bookmark = mark("план", 3, "выучить владение");
        let edited = "# План\n\nсначала кофе\nпотом чай\n\nвыучить владение\n";

        assert_eq!(bookmark.locate(edited), Some(6));
    }

    #[test]
    fn of_two_equal_lines_the_nearest_wins() {
        let bookmark = mark("список", 4, "- [ ] сделать");
        let edited = "- [ ] сделать\nа\nб\nв\nг\n- [ ] сделать\n";

        assert_eq!(bookmark.locate(edited), Some(6));
    }

    #[test]
    fn at_equal_distance_the_upper_line_wins() {
        let bookmark = mark("список", 3, "- [ ] сделать");
        let edited = "а\n- [ ] сделать\nб\n- [ ] сделать\n";

        assert_eq!(bookmark.locate(edited), Some(2));
    }

    #[test]
    fn a_deleted_line_is_lost() {
        let bookmark = mark("план", 3, "выучить владение");

        assert_eq!(bookmark.locate("# План\n\nпотрогать tantivy\n"), None);
    }

    #[test]
    fn reanchor_moves_lines_and_keeps_the_lost_ones() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("план"), 4, PLAN);
        bookmarks.toggle(&id("другая"), 3, PLAN);

        bookmarks.reanchor(&id("план"), "# План\n\nпро кофе\nвыучить владение\n");

        assert_eq!(
            bookmarks.all(),
            [
                mark("план", 4, "выучить владение"),
                mark("план", 4, "потрогать tantivy"),
                mark("другая", 3, "выучить владение"),
            ],
            "потерявшаяся закладка и закладка чужой заметки не тронуты"
        );
    }

    #[test]
    fn place_moves_bookmarks_and_takes_fresh_text() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("другая"), 1, "текст\n");
        let edited = "# План\n\nпро кофе\nвыучить заимствование\n";

        bookmarks.place(&id("план"), &lines(&[4]), edited);

        assert_eq!(
            bookmarks.all(),
            [
                mark("план", 4, "выучить заимствование"),
                mark("другая", 1, "текст"),
            ],
            "закладка осталась первой в списке"
        );
    }

    #[test]
    fn place_adds_drops_and_ignores_bad_lines() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 1, PLAN);
        bookmarks.toggle(&id("план"), 3, PLAN);

        bookmarks.place(&id("план"), &lines(&[3, 3, 0, 9]), PLAN);
        assert_eq!(bookmarks.all(), [mark("план", 3, "выучить владение")]);

        bookmarks.place(&id("план"), &lines(&[3, 4]), PLAN);
        assert_eq!(
            bookmarks.all(),
            [
                mark("план", 3, "выучить владение"),
                mark("план", 4, "потрогать tantivy"),
            ]
        );

        bookmarks.place(&id("план"), &[], PLAN);
        assert!(bookmarks.all().is_empty());
    }

    #[test]
    fn place_takes_names_from_the_window() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("план"), 4, PLAN);

        bookmarks.place(
            &id("план"),
            &[Mark::new(3, Some("  владение ")), Mark::new(4, Some("   "))],
            PLAN,
        );

        assert_eq!(
            bookmarks.all(),
            [
                named("план", 3, "выучить владение", "владение"),
                mark("план", 4, "потрогать tantivy"),
            ],
            "пробелы по краям срезаны, пустое имя — нет имени"
        );
    }

    #[test]
    fn of_merged_marks_the_named_one_wins() {
        let mut bookmarks = Bookmarks::default();

        bookmarks.place(
            &id("план"),
            &[Mark::new(3, None), Mark::new(3, Some("владение"))],
            PLAN,
        );

        assert_eq!(
            bookmarks.all(),
            [named("план", 3, "выучить владение", "владение")]
        );
    }

    #[test]
    fn rename_mark_sets_and_removes_a_name() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);

        assert!(bookmarks.rename_mark(&id("план"), 3, Some("владение")));
        assert_eq!(
            bookmarks.all(),
            [named("план", 3, "выучить владение", "владение")]
        );

        assert!(bookmarks.rename_mark(&id("план"), 3, Some(" ")));
        assert_eq!(bookmarks.all(), [mark("план", 3, "выучить владение")]);

        assert!(
            !bookmarks.rename_mark(&id("план"), 4, Some("tantivy")),
            "на строке 4 закладки нет"
        );
    }

    #[test]
    fn bookmarks_follow_their_notes() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("rust/владение"), 1, "текст\n");
        bookmarks.toggle(&id("rust/трейты"), 1, "текст\n");
        bookmarks.toggle(&id("рецепты"), 1, "текст\n");

        bookmarks.rename(&[
            (id("rust/владение"), id("учёба/rust/владение")),
            (id("rust/трейты"), id("учёба/rust/трейты")),
        ]);

        let notes: Vec<&str> = bookmarks
            .all()
            .iter()
            .map(|bookmark| bookmark.note.as_str())
            .collect();
        assert_eq!(
            notes,
            ["учёба/rust/владение", "учёба/rust/трейты", "рецепты"]
        );
    }

    #[test]
    fn forget_removes_only_that_note() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 1, PLAN);
        bookmarks.toggle(&id("другая"), 1, "текст\n");
        bookmarks.toggle(&id("план"), 3, PLAN);

        assert_eq!(bookmarks.forget(&id("план")), 2);
        assert_eq!(
            bookmarks.forget(&id("план")),
            0,
            "второй раз забывать нечего"
        );

        assert_eq!(bookmarks.all(), [mark("другая", 1, "текст")]);
    }

    #[test]
    fn bookmarks_survive_save_and_load() {
        let vault = tempfile::tempdir().unwrap();
        let path = Bookmarks::path(vault.path());
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("другая"), 1, "текст\n");

        bookmarks.save(&path).unwrap();
        let loaded = Bookmarks::load(&path).unwrap();

        assert!(path.ends_with(".kknow/bookmarks.json"));
        assert_eq!(loaded.bookmarks, bookmarks);
        assert!(loaded.problem.is_none());
        assert!(
            !path.with_extension("json.tmp").exists(),
            "временный файл не остаётся"
        );
    }

    #[test]
    fn no_file_means_no_bookmarks() {
        let vault = tempfile::tempdir().unwrap();

        let loaded = Bookmarks::load(&Bookmarks::path(vault.path())).unwrap();

        assert!(loaded.bookmarks.all().is_empty());
        assert!(loaded.problem.is_none());
    }

    #[test]
    fn a_broken_file_is_set_aside() {
        let vault = tempfile::tempdir().unwrap();
        let path = Bookmarks::path(vault.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ не json").unwrap();

        let loaded = Bookmarks::load(&path).unwrap();

        assert!(loaded.bookmarks.all().is_empty());
        assert!(loaded.problem.is_some());
        assert!(path.with_extension("json.broken").exists());
    }

    #[test]
    fn a_file_without_names_still_loads() {
        let vault = tempfile::tempdir().unwrap();
        let path = Bookmarks::path(vault.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{ "version": 1, "bookmarks": [{ "note": "план", "line": 3, "text": "выучить владение" }] }"#,
        )
        .unwrap();

        let loaded = Bookmarks::load(&path).unwrap();

        assert!(loaded.problem.is_none());
        assert_eq!(
            loaded.bookmarks.all(),
            [mark("план", 3, "выучить владение")]
        );
    }

    #[test]
    fn a_name_goes_to_the_file_only_when_there_is_one() {
        let vault = tempfile::tempdir().unwrap();
        let path = Bookmarks::path(vault.path());
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("план"), 4, PLAN);
        bookmarks.rename_mark(&id("план"), 3, Some("владение"));

        bookmarks.save(&path).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert_eq!(
            json["bookmarks"],
            serde_json::json!([
                { "note": "план", "line": 3, "text": "выучить владение", "name": "владение" },
                { "note": "план", "line": 4, "text": "потрогать tantivy" },
            ])
        );
        assert_eq!(Bookmarks::load(&path).unwrap().bookmarks, bookmarks);
    }

    #[test]
    fn the_file_is_readable_json() {
        let vault = tempfile::tempdir().unwrap();
        let path = Bookmarks::path(vault.path());
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);

        bookmarks.save(&path).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "version": 1,
                "bookmarks": [{ "note": "план", "line": 3, "text": "выучить владение" }]
            })
        );
    }
}
