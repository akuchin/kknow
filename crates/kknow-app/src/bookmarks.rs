//! Bookmarks: places in notes to return to quickly.
//!
//! They are stored in the vault, in `.kknow/bookmarks.json`: they belong to the
//! notes directory, not to the window, so the CLI also sees them.
//!
//! A bookmark points to a line, not to a text offset. Offsets differ between
//! the window and the core: the editor counts UTF-16 units and Rust counts
//! bytes, so for Cyrillic text they differ by a factor of two. The line number
//! is the same everywhere.
//!
//! Lines move when text is added above them, so the bookmark also stores the
//! line text. It uses that text to find its place again ([`Bookmark::locate`]).
//!
//! A bookmark can have a name. Files from before names have no `name` field
//! and still load: the field is optional, so the format version stays 1.

use std::fs;
use std::path::{Path, PathBuf};

use kknow_core::NoteId;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Bookmarks file, relative to the vault root.
pub const BOOKMARKS_PATH: &str = ".kknow/bookmarks.json";

/// Current file format version.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// Note ID ([`NoteId::as_str`]).
    pub note: String,
    /// Line number, starting from 1, as in the editor.
    pub line: usize,
    /// Text of the line when the bookmark was set or last updated.
    pub text: String,
    /// Name that the user gave the bookmark. Never empty: no name is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A bookmark as the editor sees it: a line of the open text and a name.
/// The window gets these with the note and sends them back on save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    /// Line number, starting from 1.
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

/// A name without the outer spaces, or `None` if nothing is left.
pub fn clean_name(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// All bookmarks of a vault, in the order they were set.
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

/// Result of reading the bookmarks file.
#[derive(Debug)]
pub struct Loaded {
    pub bookmarks: Bookmarks,
    /// The file did not parse: it is moved aside (`.json.broken`) and the
    /// bookmarks start empty.
    pub problem: Option<String>,
}

impl Bookmark {
    /// Current line of the bookmark in `content`, or `None` if its line is
    /// gone.
    ///
    /// If the line is in place with the same text, the number stays. Otherwise
    /// the nearest line with the same text wins; on a tie, the upper one wins.
    pub fn locate(&self, content: &str) -> Option<usize> {
        // On a tie, `min_by_key` returns the first minimum, and lines go top to
        // bottom, so the upper line wins. The old place is a special case with
        // distance zero.
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

/// Text of line `line` (starting from 1), or `None` if there is no such line.
fn line_text(content: &str, line: usize) -> Option<&str> {
    content.lines().nth(line.checked_sub(1)?)
}

impl Bookmarks {
    pub fn path(vault_root: impl AsRef<Path>) -> PathBuf {
        vault_root.as_ref().join(BOOKMARKS_PATH)
    }

    /// Reads bookmarks. No file gives an empty list. A broken file also does,
    /// plus a problem message and a copy next to it.
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
                        "закладки не разобрать ({error}); файл отложен в {}",
                        broken.display()
                    )),
                })
            }
        }
    }

    /// Writes through a temporary file and creates `.kknow` if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
        }

        let mut bytes = serde_json::to_vec_pretty(self).expect("закладки сериализуемы");
        bytes.push(b'\n');

        let temp = path.with_extension("json.tmp");
        fs::write(&temp, bytes).map_err(|source| Error::io(&temp, source))?;
        fs::rename(&temp, path).map_err(|source| Error::io(path, source))
    }

    /// All bookmarks, in the order they were set.
    pub fn all(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Bookmarks of one note, sorted by line.
    pub fn for_note(&self, note: &NoteId) -> Vec<&Bookmark> {
        let mut found: Vec<&Bookmark> = self
            .bookmarks
            .iter()
            .filter(|bookmark| bookmark.belongs_to(note))
            .collect();
        found.sort_by_key(|bookmark| bookmark.line);
        found
    }

    /// Sets a bookmark on line `line` of the note, or removes an existing one.
    ///
    /// `content` is the full note text; the line text comes from it. Returns
    /// `true` if the bookmark is now set. If there is no such line, nothing
    /// changes and the result is `false`.
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

    /// Gives the bookmark on line `line` of the note a name, or removes the
    /// name (`None` or only spaces). Returns `false` if there is no bookmark on
    /// that line.
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

    /// Call after a note save: bookmarks find their lines again and store the
    /// new numbers. Lost ones stay as they are: their line can be cut and
    /// pasted back.
    pub fn reanchor(&mut self, note: &NoteId, content: &str) {
        for bookmark in self.bookmarks.iter_mut().filter(|b| b.belongs_to(note)) {
            if let Some(line) = bookmark.locate(content) {
                bookmark.line = line;
            }
        }
    }

    /// The window reports the current bookmarks of the note. It moves them
    /// with the edits, so it knows them better than a text search. The names
    /// also come from the window: it is where the user edits them.
    ///
    /// Bookmarks match in line order: the first old one gets the first new line,
    /// and so on. So each bookmark keeps its place in the full list. Extra old
    /// ones are removed (their lines merged), extra new ones go to the end.
    /// Numbers that are not in the text are dropped. If two marks have the same
    /// line, the first one with a name wins.
    pub fn place(&mut self, note: &NoteId, marks: &[Mark], content: &str) {
        let mut marks: Vec<Mark> = marks
            .iter()
            .filter(|mark| line_text(content, mark.line).is_some())
            .map(|mark| Mark::new(mark.line, mark.name.as_deref()))
            .collect();
        // A stable sort keeps the order of marks on one line, and named ones
        // go first, so `dedup` keeps the name.
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
        // Remove from the end: a removal shifts everything to its right.
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

    /// Notes moved: pairs of "from -> to". One pair is a note rename, many pairs
    /// are a folder move ([`kknow_core::Vault::rename_folder`]).
    pub fn rename(&mut self, moves: &[(NoteId, NoteId)]) {
        for bookmark in &mut self.bookmarks {
            if let Some((_, to)) = moves.iter().find(|(from, _)| bookmark.belongs_to(from)) {
                bookmark.note = to.as_str().to_string();
            }
        }
    }

    /// Removes the bookmarks of a deleted note and returns how many.
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

    /// Marks without names, as the editor sends them for plain bookmarks.
    fn lines(lines: &[usize]) -> Vec<Mark> {
        lines.iter().map(|&line| Mark::new(line, None)).collect()
    }

    const PLAN: &str = "# План\n\nвыучить владение\nпотрогать tantivy\n";

    // --- toggle ---

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

    // --- all / for_note ---

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

    // --- locate ---

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

        // Line 1 is farther from line 4 (3) than line 6 is (2).
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

    // --- reanchor ---

    #[test]
    fn reanchor_moves_lines_and_keeps_the_lost_ones() {
        let mut bookmarks = Bookmarks::default();
        bookmarks.toggle(&id("план"), 3, PLAN);
        bookmarks.toggle(&id("план"), 4, PLAN);
        bookmarks.toggle(&id("другая"), 3, PLAN);

        // The ownership line moved down by one; the tantivy line is deleted.
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

    // --- place ---

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

        // Two bookmarks merged into one; 9 and 0 are outside the text.
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

    // --- rename_mark ---

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

    // --- rename / forget ---

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

    // --- load / save ---

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
