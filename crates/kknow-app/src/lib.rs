pub mod attachments;
pub mod bookmarks;
pub mod commands;
pub mod files;
pub mod git_client;
pub mod keymap;
pub mod lines;
pub mod lock;
pub mod mcp;
pub mod mentions;
pub mod merge;
pub mod meta;
pub mod problem;
pub mod proposals;
pub mod sections;
pub mod settings;
pub mod text;
pub mod theme;
pub mod threads;
pub mod vaults;
pub mod view;
pub mod watch;
mod workspace;

pub const DEFAULT_SEARCH_LIMIT: usize = 10;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use kknow_core::{NoteId, Vault};
use kknow_index::{Block, Edge, Heading, LinkGraph, ParsedNote, Resolver, SearchIndex};
use kknow_sync::CONFLICT_MARK;

use crate::bookmarks::Bookmarks;
use crate::lines::LineContext;
use crate::watch::VaultWatcher;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Vault(#[from] kknow_core::Error),

    #[error(transparent)]
    Search(#[from] kknow_index::search::Error),

    #[error(transparent)]
    Sync(#[from] kknow_sync::Error),

    #[error(transparent)]
    Crypto(#[from] kknow_crypto::Error),

    #[error(transparent)]
    Run(#[from] kknow_run::Error),

    #[error("cannot watch the folder: {0}")]
    Watch(#[from] notify::Error),

    #[error("invalid note name: {0}")]
    InvalidName(String),

    #[error("the note already exists: {0}")]
    AlreadyExists(NoteId),

    #[error("{0} has no task on line {1}: the note changed")]
    NotATask(NoteId, usize),

    #[error("{0} has no mention on line {1}: the note changed")]
    NotAMention(NoteId, usize),

    #[error("{0} has no comment \"{2}\" on line {1}: the note changed")]
    NotAnAgentComment(NoteId, usize, String),

    #[error("proposal {0} is not found")]
    UnknownProposal(String),

    #[error("{0}")]
    BadProposal(proposals::ProposalProblem),

    #[error("edit {1} of proposal {0} no longer finds its text: the note changed")]
    StaleEdit(String, usize),

    #[error("comment {0} is not found")]
    UnknownComment(String),

    #[error("comment {0} is busy: {word}", word = .1.word())]
    CommentBusy(String, threads::Status),

    #[error("no random numbers for the MCP token: {0}")]
    NoRandomNumbers(String),

    #[error("the message is empty")]
    EmptyMessage,

    #[error("the note {0} is locked")]
    NoteLocked(NoteId),

    #[error("locked notes are closed: enter the vault password")]
    NotesClosed,

    #[error("the vault has no password for locked notes")]
    NoLockPassword,

    #[error("the vault already has a password for locked notes")]
    LockPasswordExists,

    #[error("the note {0} is not locked")]
    NotLocked(NoteId),

    #[error("the note {0} is already locked")]
    AlreadyLocked(NoteId),

    #[error("vault is not found: {0}")]
    UnknownVault(String),

    #[error("not a note or a folder of this vault: {0}")]
    UnknownTarget(String),

    #[error("not a folder: {0}")]
    NotADirectory(PathBuf),

    #[error("not a markdown file: {0}")]
    NotAMarkdownFile(PathBuf),

    #[error("this folder is already in the vault list: {0}")]
    DuplicateVault(PathBuf),

    #[error("a vault cannot be inside another vault: {0} is inside {1}")]
    NestedVault(PathBuf, PathBuf),

    #[error("{}", kknow_core::io_text(path, source))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Changes {
    pub updated: Vec<NoteId>,
    pub removed: Vec<NoteId>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.removed.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    Written,
    Conflict { copy: NoteId, disk: String },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Relinked {
    pub notes: usize,
    pub links: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget<'a> {
    pub id: NoteId,
    pub line: Option<usize>,
    pub headings: &'a [Heading],
    pub blocks: &'a [Block],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkLine<'a> {
    pub edge: &'a Edge,
    pub context: LineContext,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VaultStats {
    pub notes: usize,
    pub folders: usize,
    pub bytes: u64,
}

pub struct Workspace {
    vault: Vault,
    parsed: BTreeMap<NoteId, ParsedNote>,
    hashes: HashMap<NoteId, u64>,
    graph: LinkGraph,
    resolver: Resolver,
    search: SearchIndex,
    bookmarks: Bookmarks,
    locked: BTreeSet<NoteId>,
    notes_key: Option<kknow_crypto::locked::NotesKey>,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let vault = Vault::open(root)?;
        let mut workspace = Workspace {
            vault,
            parsed: BTreeMap::new(),
            hashes: HashMap::new(),
            graph: LinkGraph::default(),
            resolver: Resolver::new(std::iter::empty()),
            search: SearchIndex::new()?,
            bookmarks: Bookmarks::default(),
            locked: BTreeSet::new(),
            notes_key: None,
        };
        workspace.load_bookmarks()?;
        workspace.reparse_all()?;
        Ok(workspace)
    }

    pub fn watch(&self) -> Result<VaultWatcher> {
        Ok(VaultWatcher::start(self.vault.root())?)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.vault.rescan()?;
        self.load_bookmarks()?;
        self.reparse_all()
    }
}

pub(crate) fn subpath_line(parsed: &ParsedNote, sub: &str) -> Option<usize> {
    match sub.strip_prefix('^') {
        Some(block) => parsed
            .blocks
            .iter()
            .find(|each| each.id.eq_ignore_ascii_case(block.trim()))
            .map(|each| each.line),
        None => heading_line(&parsed.headings, sub.rsplit('#').next().unwrap_or(sub)),
    }
}

fn heading_line(headings: &[Heading], wanted: &str) -> Option<usize> {
    fn plain(text: &str) -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    fn slug(text: &str) -> String {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
    let (plain_wanted, slug_wanted) = (plain(wanted), slug(wanted));
    headings
        .iter()
        .find(|heading| plain(&heading.text) == plain_wanted)
        .or_else(|| {
            headings
                .iter()
                .find(|heading| !slug_wanted.is_empty() && slug(&heading.text) == slug_wanted)
        })
        .map(|heading| heading.line)
}

fn conflict_copy(id: &NoteId, content: &str) -> NoteId {
    let short = format!("{:016x}", hash(content));
    NoteId::parse(&format!("{id}{CONFLICT_MARK}{})", &short[..8]))
        .expect("the copy name differs from the original only by a suffix")
}

pub(crate) fn hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::Mark;
    use crate::watch::VaultEvent;
    use kknow_sync::SyncEngine;
    use std::fs;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Projects")).unwrap();
        fs::write(
            dir.path().join("Index.md"),
            "# Оглавление\n\n[[Projects/kknow]]",
        )
        .unwrap();
        fs::write(
            dir.path().join("Projects/kknow.md"),
            "# kknow\n\nПроект про заметки и синхронизацию. #rust",
        )
        .unwrap();

        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn link_workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Projects")).unwrap();
        fs::write(
            dir.path().join("Projects/Plan.md"),
            "# Plan\n\n## Next steps\n\nShip it ^ship\n\n### Risks\n",
        )
        .unwrap();
        fs::write(dir.path().join("Home.md"), "# Home\n\n[[Plan]]\n").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        (dir, workspace)
    }

    fn found(workspace: &Workspace, query: &str) -> Vec<String> {
        workspace
            .search(query, 10)
            .unwrap()
            .into_iter()
            .map(|hit| hit.id.as_str().to_string())
            .collect()
    }

    fn lines(marks: Vec<Mark>) -> Vec<usize> {
        marks.into_iter().map(|mark| mark.line).collect()
    }

    fn bookmarked(workspace: &Workspace) -> Vec<(String, usize)> {
        workspace
            .bookmarks()
            .into_iter()
            .map(|bookmark| (bookmark.note.clone(), bookmark.line))
            .collect()
    }

    #[test]
    fn the_workspace_lists_duplicate_files() {
        let (dir, _) = workspace();
        fs::write(dir.path().join("Index.markdown"), "other").unwrap();

        let workspace = Workspace::open(dir.path()).unwrap();

        let duplicates = workspace.duplicates();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].id.as_str(), "Index");
        assert!(duplicates[0].skipped.ends_with("Index.markdown"));
    }

    #[test]
    fn tasks_come_from_all_notes_in_order() {
        let (dir, mut workspace) = workspace();
        let plan = NoteId::parse("План").unwrap();
        workspace
            .save(&plan, "# План\n\n- [x] начать\n- [ ] закончить\n")
            .unwrap();
        fs::write(
            dir.path().join("Index.md"),
            "# Оглавление\n\n- [ ] прочитать\n",
        )
        .unwrap();
        workspace.reload().unwrap();

        let tasks: Vec<(&str, usize, bool)> = workspace
            .tasks()
            .into_iter()
            .map(|(id, task)| (id.as_str(), task.line, task.done))
            .collect();
        assert_eq!(
            tasks,
            [("Index", 3, false), ("План", 3, true), ("План", 4, false)]
        );
    }

    #[test]
    fn resolve_agent_comment_removes_it_from_the_file_and_the_parse() {
        let (dir, mut workspace) = workspace();
        let plan = NoteId::parse("План").unwrap();
        workspace
            .save(
                &plan,
                "# План\n\nПоиск быстрый<!-- @agent «быстрый»: замерить -->.\n",
            )
            .unwrap();
        assert_eq!(workspace.agent_comments().len(), 1);

        let error = workspace
            .resolve_agent_comment(&plan, 3, "другое")
            .unwrap_err();
        assert!(
            matches!(error, Error::NotAnAgentComment(_, 3, _)),
            "{error}"
        );

        let content = workspace
            .resolve_agent_comment(&plan, 3, "замерить")
            .unwrap();
        assert_eq!(content, "# План\n\nПоиск быстрый.\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("План.md")).unwrap(),
            content
        );
        assert!(workspace.agent_comments().is_empty(), "разбор обновился");
    }

    #[test]
    fn set_task_writes_the_file_and_refuses_a_stale_line() {
        let (dir, mut workspace) = workspace();
        let plan = NoteId::parse("План").unwrap();
        workspace.save(&plan, "# План\n\n- [ ] начать\n").unwrap();

        let content = workspace.set_task_plain(&plan, 3, true).unwrap();
        assert_eq!(content, "# План\n\n- [x] начать\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("План.md")).unwrap(),
            content
        );
        assert!(
            workspace.tasks()[0].1.done,
            "разбор обновился вместе с файлом"
        );

        fs::write(
            dir.path().join("План.md"),
            "# План\n\nвступление\n- [x] начать\n",
        )
        .unwrap();
        let error = workspace.set_task_plain(&plan, 3, false).unwrap_err();
        assert!(matches!(error, Error::NotATask(_, 3)), "{error}");
        assert_eq!(
            fs::read_to_string(dir.path().join("План.md")).unwrap(),
            "# План\n\nвступление\n- [x] начать\n",
            "файл не тронут"
        );
    }

    #[test]
    fn bookmarks_survive_reopening_the_vault() {
        let (dir, mut workspace) = workspace();
        let kknow = NoteId::parse("Projects/kknow").unwrap();

        assert!(workspace.toggle_bookmark(&kknow, 3).unwrap());
        drop(workspace);
        let workspace = Workspace::open(dir.path()).unwrap();

        assert_eq!(bookmarked(&workspace), [("Projects/kknow".to_string(), 3)]);
        assert!(dir.path().join(".kknow/bookmarks.json").exists());
    }

    #[test]
    fn saving_moves_a_bookmark_with_its_line() {
        let (_dir, mut workspace) = workspace();
        let kknow = NoteId::parse("Projects/kknow").unwrap();
        workspace.toggle_bookmark(&kknow, 3).unwrap();

        let edited = "# kknow\n\nВступление.\n\nПроект про заметки и синхронизацию. #rust";
        workspace.save(&kknow, edited).unwrap();

        assert_eq!(lines(workspace.note_bookmarks(&kknow, edited)), [5]);
    }

    #[test]
    fn bookmarks_follow_renames_and_go_with_deleted_notes() {
        let (_dir, mut workspace) = workspace();
        let kknow = NoteId::parse("Projects/kknow").unwrap();
        let index = NoteId::parse("Index").unwrap();
        workspace.toggle_bookmark(&kknow, 1).unwrap();
        workspace.toggle_bookmark(&index, 1).unwrap();

        workspace.rename_folder("Projects", "Учёба").unwrap();
        workspace.delete(&index).unwrap();

        assert_eq!(bookmarked(&workspace), [("Учёба/kknow".to_string(), 1)]);
    }

    #[test]
    fn a_lost_line_keeps_its_bookmark_within_the_text() {
        let (_dir, mut workspace) = workspace();
        let kknow = NoteId::parse("Projects/kknow").unwrap();
        workspace.toggle_bookmark(&kknow, 3).unwrap();

        assert_eq!(lines(workspace.note_bookmarks(&kknow, "# kknow\n")), [1]);
        assert!(workspace.note_bookmarks(&kknow, "").is_empty());
    }

    #[test]
    fn a_bookmark_name_survives_reopening_and_saves() {
        let (dir, mut workspace) = workspace();
        let kknow = NoteId::parse("Projects/kknow").unwrap();
        workspace.toggle_bookmark(&kknow, 3).unwrap();

        assert!(workspace.name_bookmark(&kknow, 3, Some("суть")).unwrap());
        assert!(
            !workspace
                .name_bookmark(&kknow, 1, Some("заголовок"))
                .unwrap()
        );
        drop(workspace);
        let mut workspace = Workspace::open(dir.path()).unwrap();

        let edited = "# kknow\n\nВступление.\n\nПроект про заметки и синхронизацию. #rust";
        workspace.save(&kknow, edited).unwrap();

        assert_eq!(
            workspace.note_bookmarks(&kknow, edited),
            [Mark::new(5, Some("суть"))]
        );
    }

    #[test]
    fn opens_and_parses_every_note() {
        let (_dir, workspace) = workspace();

        assert_eq!(workspace.len(), 2);
        assert_eq!(
            workspace
                .parsed(&NoteId::parse("Projects/kknow").unwrap())
                .unwrap()
                .title(),
            Some("kknow")
        );
    }

    #[test]
    fn search_is_ready_right_after_open() {
        let (_dir, workspace) = workspace();

        assert_eq!(found(&workspace, "синхронизацию"), ["Projects/kknow"]);
        assert_eq!(found(&workspace, "tag:rust"), ["Projects/kknow"]);
    }

    #[test]
    fn stats_count_notes_folders_and_bytes() {
        let (dir, mut workspace) = workspace();
        fs::create_dir_all(dir.path().join("Архив/2026/июль")).unwrap();
        fs::write(dir.path().join("Архив/2026/июль/Итоги.md"), "# Итоги\n").unwrap();
        workspace.reload().unwrap();

        let stats = workspace.stats();

        assert_eq!(stats.notes, 3);
        assert_eq!(stats.folders, 4);

        let expected: u64 = ["Index.md", "Projects/kknow.md", "Архив/2026/июль/Итоги.md"]
            .iter()
            .map(|name| fs::metadata(dir.path().join(name)).unwrap().len())
            .sum();
        assert_eq!(stats.bytes, expected);
    }

    #[test]
    fn find_falls_back_to_the_file_name() {
        let (_dir, workspace) = workspace();

        assert_eq!(workspace.find("kknow").unwrap().as_str(), "Projects/kknow");
        assert!(workspace.find("нет такой").is_none());
    }

    #[test]
    fn careful_save_writes_when_the_file_is_as_the_editor_saw_it() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Index").unwrap();
        let base = workspace.read_plain(&id).unwrap();

        let saved = workspace
            .save_if_unchanged(&id, "# Новое\n", &base)
            .unwrap();

        assert_eq!(saved, Saved::Written);
        assert_eq!(
            fs::read_to_string(dir.path().join("Index.md")).unwrap(),
            "# Новое\n"
        );
    }

    #[test]
    fn careful_save_keeps_an_outside_edit_and_puts_ours_aside() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Index").unwrap();
        let base = workspace.read_plain(&id).unwrap();
        fs::write(dir.path().join("Index.md"), "# Правка снаружи\n").unwrap();

        let saved = workspace
            .save_if_unchanged(&id, "# Моя правка\n", &base)
            .unwrap();

        let Saved::Conflict { copy, disk } = saved else {
            panic!("ожидался конфликт, а не запись");
        };
        assert_eq!(disk, "# Правка снаружи\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("Index.md")).unwrap(),
            disk
        );
        assert!(kknow_sync::is_conflict_copy(&copy));
        assert_eq!(workspace.read_plain(&copy).unwrap(), "# Моя правка\n");
    }

    #[test]
    fn careful_save_is_not_a_conflict_when_both_sides_agree() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Index").unwrap();
        fs::write(dir.path().join("Index.md"), "# Одинаково\n").unwrap();

        let saved = workspace
            .save_if_unchanged(&id, "# Одинаково\n", "старое")
            .unwrap();

        assert_eq!(saved, Saved::Written);
    }

    #[test]
    fn careful_save_brings_back_a_note_deleted_outside() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Index").unwrap();
        let base = workspace.read_plain(&id).unwrap();
        fs::remove_file(dir.path().join("Index.md")).unwrap();

        let saved = workspace
            .save_if_unchanged(&id, "# Вернулась\n", &base)
            .unwrap();

        assert_eq!(saved, Saved::Written);
        assert_eq!(
            fs::read_to_string(dir.path().join("Index.md")).unwrap(),
            "# Вернулась\n"
        );
    }

    #[test]
    fn save_updates_parse_graph_and_search() {
        let (_dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        workspace
            .save(&id, "# kknow\n\nтеперь про компиляторы и [[Index]]")
            .unwrap();

        assert_eq!(
            workspace.backlinks(&NoteId::parse("Index").unwrap()).len(),
            1
        );
        assert_eq!(found(&workspace, "компиляторы"), ["Projects/kknow"]);
        assert!(found(&workspace, "синхронизацию").is_empty());
    }

    #[test]
    fn create_makes_a_note_with_a_heading() {
        let (_dir, mut workspace) = workspace();

        let id = workspace.create("Daily/2026-07-26").unwrap();

        assert_eq!(workspace.read_plain(&id).unwrap(), "# 2026-07-26\n\n");
        assert!(workspace.contains(&id));
    }

    #[test]
    fn create_refuses_to_overwrite() {
        let (_dir, mut workspace) = workspace();

        assert!(matches!(
            workspace.create("Index"),
            Err(Error::AlreadyExists(_))
        ));
    }

    #[test]
    fn delete_removes_the_note_from_graph_and_search() {
        let (_dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        workspace.delete(&id).unwrap();

        assert!(!workspace.contains(&id));
        assert_eq!(workspace.graph().broken().len(), 1);
        assert!(found(&workspace, "синхронизацию").is_empty());
    }

    #[test]
    fn a_restored_note_returns_to_graph_and_search() {
        let (_dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();
        workspace.delete(&id).unwrap();

        let entries = workspace.trash().entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            workspace.restore(&entries[0].id).unwrap(),
            std::slice::from_ref(&id)
        );

        assert!(workspace.contains(&id));
        assert!(workspace.graph().broken().is_empty());
        assert_eq!(found(&workspace, "синхронизацию"), ["Projects/kknow"]);
        assert!(workspace.trash().entries().unwrap().is_empty());
    }

    #[test]
    fn rename_moves_the_note_in_search_and_graph() {
        let (_dir, mut workspace) = workspace();
        let from = NoteId::parse("Projects/kknow").unwrap();
        let to = NoteId::parse("Архив/kknow").unwrap();

        workspace.rename(&from, &to).unwrap();

        assert!(!workspace.contains(&from));
        assert!(workspace.contains(&to));
        assert_eq!(found(&workspace, "синхронизацию"), ["Архив/kknow"]);
        assert!(workspace.graph().broken().is_empty());
        assert_eq!(
            workspace
                .read_plain(&NoteId::parse("Index").unwrap())
                .unwrap(),
            "# Оглавление\n\n[[Архив/kknow]]"
        );
    }

    #[test]
    fn rename_keeps_links_by_short_name_alive() {
        let (_dir, mut workspace) = workspace();
        let index = NoteId::parse("Index").unwrap();
        workspace.save(&index, "# Оглавление\n\n[[kknow]]").unwrap();

        let relinked = workspace
            .rename(
                &NoteId::parse("Projects/kknow").unwrap(),
                &NoteId::parse("Архив/kknow").unwrap(),
            )
            .unwrap();

        assert_eq!(relinked, Relinked::default());
        assert_eq!(
            workspace.read_plain(&index).unwrap(),
            "# Оглавление\n\n[[kknow]]"
        );
        assert!(workspace.graph().broken().is_empty());
        assert_eq!(
            workspace
                .backlinks(&NoteId::parse("Архив/kknow").unwrap())
                .len(),
            1
        );
    }

    #[test]
    fn rename_rewrites_links_in_every_form_but_not_in_code() {
        let (_dir, mut workspace) = workspace();
        let index = NoteId::parse("Index").unwrap();
        let note = NoteId::parse("Заметки/Ссылки").unwrap();
        workspace
            .save(
                &note,
                concat!(
                    "[[Projects/kknow|проект]] и ![[Projects/kknow#Схема]]\n\n",
                    "[подробно](Projects/kknow.md) и [[Projects/kknow#^a1b2]]\n\n",
                    "`[[Projects/kknow]]`\n\n```\n[[Projects/kknow]]\n```\n",
                ),
            )
            .unwrap();

        let relinked = workspace
            .rename(
                &NoteId::parse("Projects/kknow").unwrap(),
                &NoteId::parse("Архив/Старый проект").unwrap(),
            )
            .unwrap();

        assert_eq!(relinked, Relinked { notes: 2, links: 5 });
        assert_eq!(
            workspace.read_plain(&note).unwrap(),
            concat!(
                "[[Архив/Старый проект|проект]] и ![[Архив/Старый проект#Схема]]\n\n",
                "[подробно](Архив/Старый%20проект.md) и [[Архив/Старый проект#^a1b2]]\n\n",
                "`[[Projects/kknow]]`\n\n```\n[[Projects/kknow]]\n```\n",
            )
        );
        let backlinks = workspace.backlinks(&NoteId::parse("Архив/Старый проект").unwrap());
        assert_eq!(backlinks.len(), 5, "{backlinks:?}");
        assert!(workspace.graph().broken().is_empty());
        let changes = workspace
            .apply(&[VaultEvent::Changed(index), VaultEvent::Changed(note)])
            .unwrap();
        assert!(changes.is_empty());
    }

    #[test]
    fn rename_folder_rewrites_links_to_its_notes() {
        let (_dir, mut workspace) = workspace();
        let index = NoteId::parse("Index").unwrap();

        let relinked = workspace.rename_folder("Projects", "Архив").unwrap();

        assert_eq!(relinked, Relinked { notes: 1, links: 1 });
        assert_eq!(
            workspace.read_plain(&index).unwrap(),
            "# Оглавление\n\n[[Архив/kknow]]"
        );
        assert!(workspace.graph().broken().is_empty());
    }

    #[test]
    fn rename_refuses_a_taken_id() {
        let (_dir, mut workspace) = workspace();

        assert!(matches!(
            workspace.rename(
                &NoteId::parse("Projects/kknow").unwrap(),
                &NoteId::parse("Index").unwrap()
            ),
            Err(Error::AlreadyExists(_))
        ));
    }

    #[test]
    fn duplicate_picks_the_first_free_copy_name() {
        let (_dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        let first = workspace.duplicate(&id).unwrap();
        let second = workspace.duplicate(&id).unwrap();

        assert_eq!(first.as_str(), "Projects/kknow copy");
        assert_eq!(second.as_str(), "Projects/kknow copy 2");
        assert_eq!(
            workspace.read_plain(&first).unwrap(),
            workspace.read_plain(&id).unwrap()
        );
        let mut hits = found(&workspace, "синхронизацию");
        hits.sort();
        assert_eq!(
            hits,
            [
                "Projects/kknow",
                "Projects/kknow copy",
                "Projects/kknow copy 2"
            ]
        );
    }

    #[test]
    fn rename_folder_moves_its_notes_in_search_and_graph() {
        let (dir, mut workspace) = workspace();

        workspace
            .rename_folder("Projects", "Work/Projects")
            .unwrap();

        let moved = NoteId::parse("Work/Projects/kknow").unwrap();
        assert!(workspace.contains(&moved));
        assert!(!workspace.contains(&NoteId::parse("Projects/kknow").unwrap()));
        assert!(dir.path().join("Work/Projects/kknow.md").is_file());
        assert_eq!(found(&workspace, "синхронизацию"), ["Work/Projects/kknow"]);
        assert!(workspace.graph().broken().is_empty());
        assert_eq!(workspace.backlinks(&moved).len(), 1);
    }

    #[test]
    fn created_folder_is_listed_even_empty() {
        let (_dir, workspace) = workspace();

        workspace.create_folder("Черновики").unwrap();

        assert!(
            workspace
                .folders()
                .unwrap()
                .contains(&"Черновики".to_string())
        );
    }

    #[test]
    fn delete_folder_removes_its_notes_from_graph_and_search() {
        let (_dir, mut workspace) = workspace();

        workspace.delete_folder("Projects").unwrap();

        assert!(!workspace.contains(&NoteId::parse("Projects/kknow").unwrap()));
        assert_eq!(workspace.graph().broken().len(), 1);
        assert!(found(&workspace, "синхронизацию").is_empty());
        assert!(
            !workspace
                .folders()
                .unwrap()
                .contains(&"Projects".to_string())
        );
    }

    #[test]
    fn synced_notes_land_in_the_graph_and_the_search() {
        let (_dir, mut source) = workspace();
        let cloud = kknow_sync::MemoryProvider::new();
        let engine = SyncEngine::new(cloud.clone());
        source.sync(&engine).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let mut target = Workspace::open(target_dir.path()).unwrap();
        let report = target.sync(&SyncEngine::new(cloud)).unwrap();

        assert_eq!(report.downloaded.len(), 2);
        assert_eq!(target.len(), 2);
        assert_eq!(found(&target, "синхронизацию"), ["Projects/kknow"]);
        assert_eq!(
            target
                .backlinks(&NoteId::parse("Projects/kknow").unwrap())
                .len(),
            1
        );
    }

    #[test]
    fn a_note_deleted_elsewhere_leaves_the_search_too() {
        let (_dir, mut source) = workspace();
        let cloud = kknow_sync::MemoryProvider::new();
        source.sync(&SyncEngine::new(cloud.clone())).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let mut target = Workspace::open(target_dir.path()).unwrap();
        target.sync(&SyncEngine::new(cloud.clone())).unwrap();

        source
            .delete(&NoteId::parse("Projects/kknow").unwrap())
            .unwrap();
        source.sync(&SyncEngine::new(cloud.clone())).unwrap();
        let report = target.sync(&SyncEngine::new(cloud)).unwrap();

        assert_eq!(report.removed_local.len(), 1);
        assert_eq!(target.len(), 1);
        assert!(found(&target, "синхронизацию").is_empty());
        assert_eq!(
            target.trash().entries().unwrap()[0].path,
            "Projects/kknow.md"
        );
    }

    #[test]
    fn render_resolves_links_against_the_vault() {
        let (_dir, workspace) = workspace();
        let id = NoteId::parse("Index").unwrap();

        let html = workspace.render(&id, "[[kknow]] и [[Призрак]]");

        assert!(html.contains("kknow://note/Projects/kknow"), "{html}");
        assert!(html.contains("kknow://missing/"), "{html}");
    }

    #[test]
    fn external_edit_updates_everything() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        fs::write(
            dir.path().join("Projects/kknow.md"),
            "# kknow\n\nправка из внешнего редактора #заметки",
        )
        .unwrap();
        let changes = workspace.apply(&[VaultEvent::Changed(id.clone())]).unwrap();

        assert_eq!(changes.updated, std::slice::from_ref(&id));
        assert_eq!(found(&workspace, "внешнего"), ["Projects/kknow"]);
        assert_eq!(workspace.parsed(&id).unwrap().tags[0].name, "заметки");
    }

    #[test]
    fn external_delete_updates_everything() {
        let (dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        fs::remove_file(dir.path().join("Projects/kknow.md")).unwrap();
        let changes = workspace.apply(&[VaultEvent::Removed(id.clone())]).unwrap();

        assert_eq!(changes.removed, std::slice::from_ref(&id));
        assert_eq!(workspace.len(), 1);
        assert!(found(&workspace, "синхронизацию").is_empty());
    }

    #[test]
    fn our_own_save_does_not_come_back_as_a_change() {
        let (_dir, mut workspace) = workspace();
        let id = NoteId::parse("Projects/kknow").unwrap();

        workspace
            .save(&id, "# kknow\n\nсохранено приложением")
            .unwrap();
        let changes = workspace.apply(&[VaultEvent::Changed(id)]).unwrap();

        assert!(changes.is_empty());
    }

    #[test]
    fn a_new_file_becomes_a_note() {
        let (dir, mut workspace) = workspace();

        fs::write(dir.path().join("Извне.md"), "# Извне\n\nпоявилась сама").unwrap();
        let id = NoteId::parse("Извне").unwrap();
        let changes = workspace.apply(&[VaultEvent::Changed(id.clone())]).unwrap();

        assert_eq!(changes.updated, std::slice::from_ref(&id));
        assert!(workspace.contains(&id));
        assert_eq!(found(&workspace, "появилась"), ["Извне"]);
    }

    #[test]
    fn backlinks_come_with_their_lines_in_note_and_text_order() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Plan.md"), "# Plan\n").unwrap();
        fs::write(
            dir.path().join("Zeta.md"),
            "# Zeta\n\nsee [Plan](Plan.md) and\n- [[Plan|the plan]] here\n",
        )
        .unwrap();
        fs::write(dir.path().join("Alpha.md"), "intro\n![[Plan]]\n").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();

        let lines = workspace.backlink_lines(&NoteId::parse("Plan").unwrap());
        let rows: Vec<(&str, usize, &str, &str, &str)> = lines
            .iter()
            .map(|each| {
                let context = &each.context;
                (
                    each.edge.from.as_str(),
                    context.line,
                    context.before.as_str(),
                    context.text.as_str(),
                    context.after.as_str(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("Alpha", 2, "", "![[Plan]]", ""),
                ("Zeta", 3, "see ", "[Plan](Plan.md)", " and"),
                ("Zeta", 4, "- ", "[[Plan|the plan]]", " here"),
            ]
        );
    }

    #[test]
    fn a_link_finds_the_note_and_the_line_of_its_heading() {
        let (_dir, workspace) = link_workspace();
        let home = NoteId::parse("Home").unwrap();

        let plain = workspace.link_target(&home, "Plan").unwrap();
        assert_eq!(plain.id.as_str(), "Projects/Plan");
        assert_eq!(plain.line, None);
        let headings: Vec<_> = plain.headings.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(headings, ["Plan", "Next steps", "Risks"]);
        assert_eq!(plain.blocks[0].id, "ship");

        let line = |link: &str| workspace.link_target(&home, link).unwrap().line;
        assert_eq!(line("Plan#Next steps"), Some(3));
        assert_eq!(line("Plan#next  STEPS|alias"), Some(3));
        assert_eq!(line("Plan#Next steps#Risks"), Some(7));
        assert_eq!(line("Plan.md#next-steps"), Some(3));
        assert_eq!(line("Plan#^ship"), Some(5));
        assert_eq!(line("Plan#No such heading"), None);
    }

    #[test]
    fn a_link_with_only_a_heading_points_to_the_same_note() {
        let (_dir, workspace) = link_workspace();
        let plan = NoteId::parse("Projects/Plan").unwrap();

        let target = workspace.link_target(&plan, "#Risks").unwrap();
        assert_eq!(target.id, plan);
        assert_eq!(target.line, Some(7));
    }

    #[test]
    fn a_link_to_a_missing_note_finds_nothing() {
        let (_dir, workspace) = link_workspace();
        let home = NoteId::parse("Home").unwrap();

        assert!(workspace.link_target(&home, "Ghost").is_none());
        assert!(workspace.link_target(&home, "Ghost#Plan").is_none());
    }
}
