pub mod bookmarks;
pub mod commands;
pub mod files;
pub mod keymap;
pub mod lines;
pub mod lock;
pub mod mcp;
pub mod mentions;
pub mod merge;
pub mod meta;
pub mod proposals;
pub mod sections;
pub mod settings;
pub mod text;
pub mod theme;
pub mod threads;
pub mod vaults;
pub mod watch;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use kknow_core::{Note, NoteId, Trash, Vault};
use kknow_index::{
    AgentComment, Block, Edge, Heading, LinkGraph, ParsedNote, Relink, Resolver, SearchHit,
    SearchIndex, Task,
};
use kknow_sync::{
    CONFLICT_MARK, Fetched, Git, GitAuth, GitReport, GitSettings, Merged, Plan as SyncPlan,
    Report as SyncReport, Staged, SyncEngine, SyncProvider,
};

use crate::bookmarks::{Bookmark, Bookmarks, Mark};
use crate::lines::LineContext;
use crate::theme::ThemeSet;
use crate::watch::{VaultEvent, VaultWatcher};

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub fn git_token(settings: &GitSettings, stored: Option<String>) -> Option<String> {
    match settings.auth {
        GitAuth::System => None,
        GitAuth::Token => stored,
        GitAuth::Env => settings.env.as_deref().and_then(kknow_run::login::variable),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Vault(#[from] kknow_core::Error),

    #[error(transparent)]
    Search(#[from] kknow_index::search::Error),

    #[error(transparent)]
    Sync(#[from] kknow_sync::Error),

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
    BadProposal(String),

    #[error("edit {1} of proposal {0} no longer finds its text: the note changed")]
    StaleEdit(String, usize),

    #[error("comment {0} is not found")]
    UnknownComment(String),

    #[error("comment {0} is busy: {word}", word = .1.word())]
    CommentBusy(String, threads::Status),

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
    notes_key: Option<kknow_sync::locked::NotesKey>,
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

    fn load_bookmarks(&mut self) -> Result<()> {
        self.bookmarks = Bookmarks::load(&Bookmarks::path(self.vault.root()))?.bookmarks;
        Ok(())
    }

    fn edit_bookmarks(&mut self, edit: impl FnOnce(&mut Bookmarks)) -> Result<()> {
        let before = self.bookmarks.clone();
        edit(&mut self.bookmarks);
        if self.bookmarks == before {
            return Ok(());
        }
        self.bookmarks.save(&Bookmarks::path(self.vault.root()))
    }

    pub fn bookmarks(&self) -> Vec<&Bookmark> {
        self.bookmarks
            .all()
            .iter()
            .filter(|bookmark| NoteId::parse(&bookmark.note).is_some_and(|id| self.contains(&id)))
            .collect()
    }

    pub fn note_bookmarks(&self, id: &NoteId, content: &str) -> Vec<Mark> {
        let last = content.lines().count();
        if last == 0 {
            return Vec::new();
        }
        let mut marks: Vec<Mark> = self
            .bookmarks
            .for_note(id)
            .into_iter()
            .map(|bookmark| Mark {
                line: bookmark
                    .locate(content)
                    .unwrap_or(bookmark.line)
                    .clamp(1, last),
                name: bookmark.name.clone(),
            })
            .collect();
        marks.sort_by_key(|mark| (mark.line, mark.name.is_none()));
        marks.dedup_by_key(|mark| mark.line);
        marks
    }

    pub fn toggle_bookmark(&mut self, id: &NoteId, line: usize) -> Result<bool> {
        let content = self.read(id)?;
        let mut set = false;
        self.edit_bookmarks(|bookmarks| set = bookmarks.toggle(id, line, &content))?;
        Ok(set)
    }

    pub fn name_bookmark(&mut self, id: &NoteId, line: usize, name: Option<&str>) -> Result<bool> {
        if self.locked.contains(id) {
            return Err(Error::NoteLocked(id.clone()));
        }
        let mut found = false;
        self.edit_bookmarks(|bookmarks| found = bookmarks.rename_mark(id, line, name))?;
        Ok(found)
    }

    pub fn set_bookmarks(&mut self, id: &NoteId, marks: &[Mark], content: &str) -> Result<()> {
        if self.locked.contains(id) {
            return self.anchor_bookmarks(id, "");
        }
        self.edit_bookmarks(|bookmarks| bookmarks.place(id, marks, content))
    }

    fn reparse_all(&mut self) -> Result<()> {
        let ids: Vec<NoteId> = self.vault.ids().cloned().collect();

        self.parsed.clear();
        self.hashes.clear();
        self.locked.clear();
        self.search = SearchIndex::new()?;

        for id in ids {
            let content = self.vault.read(&id)?;
            self.ingest(&id, &content)?;
        }

        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    fn ingest(&mut self, id: &NoteId, content: &str) -> Result<()> {
        let locked = kknow_sync::locked::is_locked(content.as_bytes());
        let parsed = kknow_index::parse(if locked { "" } else { content });
        if locked {
            self.locked.insert(id.clone());
        } else {
            self.locked.remove(id);
        }

        let title = parsed.title().unwrap_or_else(|| id.name()).to_string();
        let tags: Vec<String> = parsed.tags.iter().map(|tag| tag.name.clone()).collect();
        self.search.upsert(id, &title, &parsed.text, &tags)?;

        self.hashes.insert(id.clone(), hash(content));
        self.parsed.insert(id.clone(), parsed);
        Ok(())
    }

    fn rebuild_graph(&mut self) {
        self.graph = LinkGraph::build(self.parsed.iter().map(|(id, note)| (id.clone(), note)));
        self.resolver = Resolver::new(self.parsed.keys());
    }

    pub fn apply(&mut self, events: &[VaultEvent]) -> Result<Changes> {
        let mut changes = Changes::default();

        for event in events {
            match event {
                VaultEvent::Changed(id) => {
                    if self.vault.refresh(id)?.is_none() {
                        continue;
                    }
                    let content = self.vault.read(id)?;

                    if self.hashes.get(id) == Some(&hash(&content)) {
                        continue;
                    }

                    self.ingest(id, &content)?;
                    self.anchor_bookmarks(id, &content)?;
                    changes.updated.push(id.clone());
                }
                VaultEvent::Removed(id) => {
                    self.vault.refresh(id)?;
                    if self.parsed.remove(id).is_some() {
                        self.hashes.remove(id);
                        self.locked.remove(id);
                        self.search.remove(id)?;
                        changes.removed.push(id.clone());
                    }
                }
            }
        }

        if !changes.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }

        Ok(changes)
    }

    pub fn sync<P: SyncProvider>(&mut self, engine: &SyncEngine<P>) -> Result<SyncReport> {
        let report = engine.run(&mut self.vault)?;
        self.absorb(&report)?;
        Ok(report)
    }

    pub fn sync_stage<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<Staged> {
        Ok(engine.stage(&self.vault)?)
    }

    pub fn sync_finish<P: SyncProvider>(
        &mut self,
        engine: &SyncEngine<P>,
        fetched: Fetched,
    ) -> Result<SyncReport> {
        let report = engine.commit_local(&mut self.vault, fetched)?;
        self.absorb(&report)?;
        Ok(report)
    }

    fn absorb(&mut self, report: &SyncReport) -> Result<()> {
        let events: Vec<VaultEvent> = report
            .local_updated()
            .map(|id| VaultEvent::Changed(id.clone()))
            .chain(
                report
                    .local_removed()
                    .map(|id| VaultEvent::Removed(id.clone())),
            )
            .collect();
        self.apply(&events)?;
        if report
            .files_local
            .iter()
            .any(|key| key == bookmarks::BOOKMARKS_PATH)
        {
            self.load_bookmarks()?;
        }
        Ok(())
    }

    pub fn git_merge(&mut self, git: &Git) -> Result<Merged> {
        let merged = git.merge()?;
        if !merged.changed.is_empty() {
            self.reload()?;
        }
        Ok(merged)
    }

    pub fn git_discard(
        &mut self,
        git: &Git,
        path: &str,
        from: Option<&str>,
        staged: bool,
    ) -> Result<()> {
        let relative = Path::new(path);
        if !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(kknow_core::Error::OutsideVault(relative.to_path_buf()).into());
        }
        if self.vault.root().join(relative).is_file() {
            self.trash().put(relative)?;
        }
        git.discard(path, from, staged)?;
        self.reload()
    }

    pub fn git_sync(&mut self, git: &Git) -> Result<GitReport> {
        git.fetch()?;
        let merged = self.git_merge(git)?;
        let pushed = git.push()?;
        Ok(GitReport { merged, pushed })
    }

    pub fn conflicts(&self) -> Vec<&NoteId> {
        self.parsed
            .keys()
            .filter(|id| kknow_sync::is_conflict_copy(id))
            .collect()
    }

    pub fn sync_plan<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<SyncPlan> {
        Ok(engine.plan(&self.vault)?)
    }

    pub fn themes(&self, global: Option<&Path>) -> ThemeSet {
        theme::load(Some(self.vault.root()), global)
    }

    pub fn stats(&self) -> VaultStats {
        let mut folders: BTreeSet<&str> = BTreeSet::new();
        let mut bytes = 0;

        for id in self.parsed.keys() {
            if let Some(folder) = id.folder() {
                for (at, _) in folder.match_indices('/') {
                    folders.insert(&folder[..at]);
                }
                folders.insert(folder);
            }

            bytes += std::fs::metadata(self.path_for(id))
                .map(|meta| meta.len())
                .unwrap_or(0);
        }

        VaultStats {
            notes: self.parsed.len(),
            folders: folders.len(),
            bytes,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        Ok(self.search.search(query, limit)?)
    }

    pub fn root(&self) -> &Path {
        self.vault.root()
    }

    pub fn len(&self) -> usize {
        self.parsed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parsed.is_empty()
    }

    pub fn graph(&self) -> &LinkGraph {
        &self.graph
    }

    pub fn notes(&self) -> impl ExactSizeIterator<Item = (&NoteId, &ParsedNote)> {
        self.parsed.iter()
    }

    pub fn note(&self, id: &NoteId) -> Option<&Note> {
        self.vault.get(id)
    }

    pub fn parsed(&self, id: &NoteId) -> Option<&ParsedNote> {
        self.parsed.get(id)
    }

    pub fn contains(&self, id: &NoteId) -> bool {
        self.parsed.contains_key(id)
    }

    pub fn path_for(&self, id: &NoteId) -> PathBuf {
        self.vault.path_for(id)
    }

    pub fn read(&self, id: &NoteId) -> Result<String> {
        let content = self.vault.read(id)?;
        if kknow_sync::locked::is_locked(content.as_bytes()) {
            return Err(Error::NoteLocked(id.clone()));
        }
        Ok(content)
    }

    pub fn find(&self, raw: &str) -> Option<&NoteId> {
        let id = NoteId::parse(raw)?;
        if let Some((found, _)) = self.parsed.get_key_value(&id) {
            return Some(found);
        }

        let name = id.name().to_lowercase();
        self.parsed
            .keys()
            .find(|candidate| candidate.name().to_lowercase() == name)
    }

    pub fn save(&mut self, id: &NoteId, content: &str) -> Result<()> {
        self.store(id, content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    fn store(&mut self, id: &NoteId, content: &str) -> Result<()> {
        let locked = self.locked.contains(id);
        self.store_as(id, content, locked)
    }

    fn store_as(&mut self, id: &NoteId, content: &str, locked: bool) -> Result<()> {
        if locked {
            let sealed = self.notes_key()?.seal(content)?;
            self.vault.write(id, &sealed)?;
            self.ingest(id, &sealed)?;
            return self.anchor_bookmarks(id, &sealed);
        }
        self.vault.write(id, content)?;
        self.ingest(id, content)?;
        self.anchor_bookmarks(id, content)
    }

    fn anchor_bookmarks(&mut self, id: &NoteId, content: &str) -> Result<()> {
        if self.locked.contains(id) {
            return self.edit_bookmarks(|bookmarks| {
                bookmarks.forget(id);
            });
        }
        self.edit_bookmarks(|bookmarks| bookmarks.reanchor(id, content))
    }

    pub fn save_if_unchanged(&mut self, id: &NoteId, content: &str, base: &str) -> Result<Saved> {
        let path = self.path_for(id);
        let locked = self.locked.contains(id);
        let disk = match std::fs::read_to_string(&path) {
            Ok(disk) if kknow_sync::locked::is_locked(disk.as_bytes()) => {
                Some(self.notes_key()?.open(&disk)?)
            }
            Ok(disk) => Some(disk),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(Error::io(&path, source)),
        };

        match disk {
            Some(disk) if disk != base && disk != content => {
                let copy = conflict_copy(id, content);
                self.store_as(&copy, content, locked)?;
                self.search.commit()?;
                self.rebuild_graph();
                Ok(Saved::Conflict { copy, disk })
            }
            _ => {
                self.save(id, content)?;
                Ok(Saved::Written)
            }
        }
    }

    pub fn tasks(&self) -> Vec<(&NoteId, &Task)> {
        self.parsed
            .iter()
            .flat_map(|(id, note)| note.tasks.iter().map(move |task| (id, task)))
            .collect()
    }

    pub fn set_task(&mut self, id: &NoteId, line: usize, done: bool) -> Result<String> {
        let content = self.read(id)?;
        self.write_task(id, content, line, done)
    }

    pub fn set_task_unlocked(&mut self, id: &NoteId, line: usize, done: bool) -> Result<String> {
        let content = self.open_text(id)?;
        self.write_task(id, content, line, done)
    }

    fn write_task(&mut self, id: &NoteId, content: String, line: usize, done: bool) -> Result<String> {
        let changed = kknow_index::parse::set_task(&content, line, done)
            .ok_or_else(|| Error::NotATask(id.clone(), line))?;
        if changed != content {
            self.save(id, &changed)?;
        }
        Ok(changed)
    }

    pub fn agent_comments(&self) -> Vec<(&NoteId, &AgentComment)> {
        self.parsed
            .iter()
            .flat_map(|(id, note)| note.agent_comments.iter().map(move |comment| (id, comment)))
            .collect()
    }

    pub fn resolve_agent_comment(
        &mut self,
        id: &NoteId,
        line: usize,
        text: &str,
    ) -> Result<String> {
        let content = self.read(id)?;
        let changed = kknow_index::parse::remove_agent_comment(&content, line, text)
            .ok_or_else(|| Error::NotAnAgentComment(id.clone(), line, text.trim().to_string()))?;
        self.save(id, &changed)?;
        Ok(changed)
    }

    pub fn create(&mut self, raw_name: &str) -> Result<NoteId> {
        let id = NoteId::parse(raw_name).ok_or_else(|| Error::InvalidName(raw_name.to_string()))?;
        if self.contains(&id) {
            return Err(Error::AlreadyExists(id));
        }

        let content = format!("# {}\n\n", id.name());
        self.save(&id, &content)?;
        Ok(id)
    }

    pub fn delete(&mut self, id: &NoteId) -> Result<()> {
        self.vault.remove(id)?;
        self.parsed.remove(id);
        self.hashes.remove(id);
        self.locked.remove(id);
        self.search.remove(id)?;
        self.search.commit()?;
        self.rebuild_graph();
        self.edit_bookmarks(|bookmarks| {
            bookmarks.forget(id);
        })
    }

    pub fn rename(&mut self, from: &NoteId, to: &NoteId) -> Result<Relinked> {
        if from == to {
            return Ok(Relinked::default());
        }
        if self.contains(to) {
            return Err(Error::AlreadyExists(to.clone()));
        }
        self.vault.rename(from, to)?;
        self.moved(from, to)?;
        let moves = [(from.clone(), to.clone())];
        self.edit_bookmarks(|bookmarks| bookmarks.rename(&moves))?;
        self.move_proposals(&moves)?;
        self.move_threads(&moves)?;
        let relinked = self.relink(&moves)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    pub fn duplicate(&mut self, id: &NoteId) -> Result<NoteId> {
        let copy = self.free_copy_name(id)?;
        self.vault.copy(id, &copy)?;
        let content = self.vault.read(&copy)?;
        self.ingest(&copy, &content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(copy)
    }

    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<Relinked> {
        let moved = self.vault.rename_folder(from, to)?;
        for (old, new) in &moved {
            self.moved(old, new)?;
        }
        self.edit_bookmarks(|bookmarks| bookmarks.rename(&moved))?;
        self.move_proposals(&moved)?;
        self.move_threads(&moved)?;
        if moved.is_empty() {
            return Ok(Relinked::default());
        }
        let relinked = self.relink(&moved)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    fn relink(&mut self, moves: &[(NoteId, NoteId)]) -> Result<Relinked> {
        let map: HashMap<NoteId, NoteId> = moves.iter().cloned().collect();
        let back: HashMap<&NoteId, &NoteId> = moves.iter().map(|(old, new)| (new, old)).collect();
        let names: BTreeSet<String> = moves
            .iter()
            .map(|(_, new)| new.name().to_lowercase())
            .collect();

        let mut sources: BTreeSet<&NoteId> = moves.iter().map(|(old, _)| old).collect();
        let namesakes = self
            .parsed
            .keys()
            .filter(|id| names.contains(&id.name().to_lowercase()))
            .map(|id| back.get(id).copied().unwrap_or(id));
        for target in moves.iter().map(|(old, _)| old).chain(namesakes) {
            sources.extend(self.graph.backlinks(target).iter().map(|edge| &edge.from));
        }

        let after = Resolver::new(self.parsed.keys());
        let relink = Relink {
            before: &self.resolver,
            after: &after,
            moves: &map,
        };
        let mut edits = Vec::new();
        for old in sources {
            let id = map.get(old).unwrap_or(old);
            let Some(parsed) = self.parsed.get(id) else {
                continue;
            };
            let content = self.vault.read(id)?;
            if let Some((text, links)) = relink.apply(old, &content, parsed) {
                edits.push((id.clone(), text, links));
            }
        }

        let mut relinked = Relinked::default();
        for (id, text, links) in edits {
            self.store(&id, &text)?;
            relinked.notes += 1;
            relinked.links += links;
        }
        Ok(relinked)
    }

    fn moved(&mut self, from: &NoteId, to: &NoteId) -> Result<()> {
        self.parsed.remove(from);
        self.hashes.remove(from);
        self.locked.remove(from);
        self.search.remove(from)?;
        let content = self.vault.read(to)?;
        self.ingest(to, &content)
    }

    fn free_copy_name(&self, id: &NoteId) -> Result<NoteId> {
        let folder = id
            .folder()
            .map(|folder| format!("{folder}/"))
            .unwrap_or_default();
        let name = id.name();
        for attempt in 1.. {
            let raw = match attempt {
                1 => format!("{folder}{name} copy"),
                n => format!("{folder}{name} copy {n}"),
            };
            let candidate = NoteId::parse(&raw).ok_or_else(|| Error::InvalidName(raw.clone()))?;
            if !self.contains(&candidate) && !self.vault.path_for(&candidate).exists() {
                return Ok(candidate);
            }
        }
        unreachable!("the endless name search ended")
    }

    pub fn folders(&self) -> Result<Vec<String>> {
        Ok(self.vault.folders()?)
    }

    pub fn create_folder(&self, raw: &str) -> Result<()> {
        Ok(self.vault.create_folder(raw)?)
    }

    pub fn delete_folder(&mut self, raw: &str) -> Result<()> {
        let removed = self.vault.remove_folder(raw)?;
        for id in &removed {
            self.parsed.remove(id);
            self.hashes.remove(id);
            self.locked.remove(id);
            self.search.remove(id)?;
        }
        if !removed.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }
        self.edit_bookmarks(|bookmarks| {
            for id in &removed {
                bookmarks.forget(id);
            }
        })
    }

    pub fn trash(&self) -> Trash {
        self.vault.trash()
    }

    pub fn restore(&mut self, entry: &str) -> Result<Vec<NoteId>> {
        let restored = self.vault.restore(entry)?;
        for id in &restored {
            let content = self.vault.read(id)?;
            self.ingest(id, &content)?;
        }
        if !restored.is_empty() {
            self.search.commit()?;
            self.rebuild_graph();
        }
        Ok(restored)
    }

    pub fn backlinks(&self, id: &NoteId) -> &[Edge] {
        self.graph.backlinks(id)
    }

    pub fn backlink_lines(&self, id: &NoteId) -> Vec<BacklinkLine<'_>> {
        let mut edges: Vec<&Edge> = self.backlinks(id).iter().collect();
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.span.start.cmp(&b.span.start)));

        let mut lines = Vec::with_capacity(edges.len());
        let mut source: Option<(&NoteId, String)> = None;
        for edge in edges {
            if source.as_ref().is_none_or(|(from, _)| *from != &edge.from) {
                source = Some((&edge.from, self.read(&edge.from).unwrap_or_default()));
            }
            let Some((_, text)) = &source else { continue };
            if let Some(context) = LineContext::around(text, &edge.span) {
                lines.push(BacklinkLine { edge, context });
            }
        }
        lines
    }

    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.graph.outgoing(id)
    }

    pub fn render(&self, id: &NoteId, content: &str) -> String {
        kknow_index::to_html_with_embeds(content, id, &self.resolver, self)
    }

    pub fn image(&self, from: &NoteId, target: &str) -> Option<PathBuf> {
        let note = self.path_for(from);
        let dir = note.parent().unwrap_or(self.root());
        kknow_core::image::resolve(self.root(), dir, target)
    }

    pub fn resolve(&self, target: &str, from: &NoteId) -> Option<NoteId> {
        self.resolver.resolve(target, from)
    }

    pub fn link_target(&self, from: &NoteId, link: &str) -> Option<LinkTarget<'_>> {
        let link = link.split('|').next().unwrap_or_default();
        let (target, subpath) = match link.split_once('#') {
            Some((target, subpath)) => (target, Some(subpath.trim())),
            None => (link, None),
        };
        let id = if target.trim().is_empty() {
            self.parsed.get_key_value(from)?.0.clone()
        } else {
            self.resolve(target, from)?
        };
        let parsed = self.parsed.get(&id)?;
        let line = subpath
            .filter(|sub| !sub.is_empty())
            .and_then(|sub| subpath_line(parsed, sub));
        Some(LinkTarget {
            id,
            line,
            headings: &parsed.headings,
            blocks: &parsed.blocks,
        })
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

        let content = workspace.set_task(&plan, 3, true).unwrap();
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
        let error = workspace.set_task(&plan, 3, false).unwrap_err();
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
        let base = workspace.read(&id).unwrap();

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
        let base = workspace.read(&id).unwrap();
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
        assert_eq!(workspace.read(&copy).unwrap(), "# Моя правка\n");
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
        let base = workspace.read(&id).unwrap();
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

        assert_eq!(workspace.read(&id).unwrap(), "# 2026-07-26\n\n");
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
            workspace.read(&NoteId::parse("Index").unwrap()).unwrap(),
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
        assert_eq!(workspace.read(&index).unwrap(), "# Оглавление\n\n[[kknow]]");
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
            workspace.read(&note).unwrap(),
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
            workspace.read(&index).unwrap(),
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
            workspace.read(&first).unwrap(),
            workspace.read(&id).unwrap()
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
