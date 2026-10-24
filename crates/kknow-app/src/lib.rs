//! An open vault with its parse results. Both the CLI and the desktop app use
//! it.
//!
//! This crate holds the app state, but not the way to show it.

pub mod bookmarks;
pub mod commands;
pub mod files;
pub mod keymap;
pub mod settings;
pub mod theme;
pub mod vaults;
pub mod watch;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use kknow_core::{Note, NoteId, Vault};
use kknow_index::{Edge, LinkGraph, ParsedNote, Relink, Resolver, SearchHit, SearchIndex};
use kknow_sync::{
    CONFLICT_MARK, Fetched, Plan as SyncPlan, Report as SyncReport, Staged, SyncEngine,
    SyncProvider,
};

use crate::bookmarks::{Bookmark, Bookmarks, Mark};
use crate::theme::ThemeSet;
use crate::watch::{VaultEvent, VaultWatcher};

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
    Run(#[from] kknow_run::Error),

    #[error("не удалось следить за каталогом: {0}")]
    Watch(#[from] notify::Error),

    #[error("некорректное имя заметки: {0}")]
    InvalidName(String),

    #[error("заметка уже существует: {0}")]
    AlreadyExists(NoteId),

    #[error("хранилище не найдено: {0}")]
    UnknownVault(String),

    #[error("не заметка и не папка этого хранилища: {0}")]
    UnknownTarget(String),

    #[error("это не каталог: {0}")]
    NotADirectory(PathBuf),

    #[error("это не markdown-файл: {0}")]
    NotAMarkdownFile(PathBuf),

    #[error("этот каталог уже в списке хранилищ: {0}")]
    DuplicateVault(PathBuf),

    #[error("хранилища не могут лежать одно в другом: {0} внутри {1}")]
    NestedVault(PathBuf, PathBuf),

    #[error("io error at {path}: {source}")]
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

/// What changed after file system events were applied.
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

/// Result of a careful save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// The file was as the editor saw it, so it is written.
    Written,
    /// The file changed outside. It stays as is, and the editor text goes to
    /// `copy` next to it, so no version is lost.
    Conflict { copy: NoteId, disk: String },
}

/// Links that a rename or a move rewrote in other notes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Relinked {
    pub notes: usize,
    pub links: usize,
}

/// Vault summary: number of notes, folders and bytes.
///
/// It is calculated on request, not stored: only the settings window and
/// `kknow stats` need it, and not often. Updating it on each edit is one more
/// way to get out of sync with the disk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VaultStats {
    pub notes: usize,
    pub folders: usize,
    pub bytes: u64,
}

/// The vault, parsed notes, the link graph and the search index.
pub struct Workspace {
    vault: Vault,
    parsed: BTreeMap<NoteId, ParsedNote>,
    /// Hash of the last known content. It shows that a change from disk is
    /// our own write.
    hashes: HashMap<NoteId, u64>,
    graph: LinkGraph,
    resolver: Resolver,
    search: SearchIndex,
    /// Vault bookmarks. They go to `.kknow/bookmarks.json` on each change:
    /// there are few of them, and a lost bookmark is annoying.
    bookmarks: Bookmarks,
}

impl Workspace {
    /// Opens the vault, parses the notes and builds the search index.
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
        };
        workspace.load_bookmarks()?;
        workspace.reparse_all()?;
        Ok(workspace)
    }

    /// Starts to watch the directory. [`Workspace::apply`] applies the events.
    pub fn watch(&self) -> Result<VaultWatcher> {
        Ok(VaultWatcher::start(self.vault.root())?)
    }

    /// Reloads the whole vault from disk.
    pub fn reload(&mut self) -> Result<()> {
        self.vault.rescan()?;
        self.load_bookmarks()?;
        self.reparse_all()
    }

    /// `Bookmarks::load` moves a broken bookmarks file aside and starts an empty
    /// list, so a broken file does not stop the vault from opening.
    fn load_bookmarks(&mut self) -> Result<()> {
        self.bookmarks = Bookmarks::load(&Bookmarks::path(self.vault.root()))?.bookmarks;
        Ok(())
    }

    /// Edits bookmarks and writes them only if something changed: a save of a
    /// note without bookmarks must not touch the file.
    fn edit_bookmarks(&mut self, edit: impl FnOnce(&mut Bookmarks)) -> Result<()> {
        let before = self.bookmarks.clone();
        edit(&mut self.bookmarks);
        if self.bookmarks == before {
            return Ok(());
        }
        self.bookmarks.save(&Bookmarks::path(self.vault.root()))
    }

    /// Bookmarks of notes that exist in the vault, in the order they were set.
    /// Bookmarks of missing notes are kept but not shown: git, for example, can
    /// remove a file and bring it back.
    pub fn bookmarks(&self) -> Vec<&Bookmark> {
        self.bookmarks
            .all()
            .iter()
            .filter(|bookmark| NoteId::parse(&bookmark.note).is_some_and(|id| self.contains(&id)))
            .collect()
    }

    /// Bookmarks of the note in `content`: lines with names, sorted and
    /// without duplicates. Of two bookmarks on one line, a named one wins.
    ///
    /// If a bookmark's line text is not found, it keeps its old number, clamped
    /// to the text. So the user can see it and remove it.
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

    /// Sets or removes a bookmark on line `line` of the note as it is on disk.
    /// `true` means the bookmark is now set.
    pub fn toggle_bookmark(&mut self, id: &NoteId, line: usize) -> Result<bool> {
        let content = self.read(id)?;
        let mut set = false;
        self.edit_bookmarks(|bookmarks| set = bookmarks.toggle(id, line, &content))?;
        Ok(set)
    }

    /// Names the bookmark on line `line` of the note, or removes the name
    /// (`None`). `false` means there is no bookmark on that line.
    pub fn name_bookmark(&mut self, id: &NoteId, line: usize, name: Option<&str>) -> Result<bool> {
        let mut found = false;
        self.edit_bookmarks(|bookmarks| found = bookmarks.rename_mark(id, line, name))?;
        Ok(found)
    }

    /// Puts the note bookmarks on exactly these lines of `content`, with these
    /// names ([`Bookmarks::place`]). The window uses this to report where edits
    /// moved them.
    pub fn set_bookmarks(&mut self, id: &NoteId, marks: &[Mark], content: &str) -> Result<()> {
        self.edit_bookmarks(|bookmarks| bookmarks.place(id, marks, content))
    }

    fn reparse_all(&mut self) -> Result<()> {
        let ids: Vec<NoteId> = self.vault.ids().cloned().collect();

        self.parsed.clear();
        self.hashes.clear();
        // An in-memory index is cheaper to rebuild than to clean of documents
        // for notes that are gone.
        self.search = SearchIndex::new()?;

        for id in ids {
            let content = self.vault.read(&id)?;
            self.ingest(&id, &content)?;
        }

        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    /// Parses the note content and updates everything that depends on it,
    /// except the graph: the graph is built from all notes at once.
    fn ingest(&mut self, id: &NoteId, content: &str) -> Result<()> {
        let parsed = kknow_index::parse(content);

        let title = parsed.title().unwrap_or_else(|| id.name()).to_string();
        let tags: Vec<String> = parsed.tags.iter().map(|tag| tag.name.clone()).collect();
        self.search.upsert(id, &title, &parsed.text, &tags)?;

        self.hashes.insert(id.clone(), hash(content));
        self.parsed.insert(id.clone(), parsed);
        Ok(())
    }

    /// Rebuilds the graph from the parsed notes.
    ///
    /// It walks only the links and does not read the disk. A link to a missing
    /// note can become valid, so everything must be recalculated.
    fn rebuild_graph(&mut self) {
        self.graph = LinkGraph::build(self.parsed.iter().map(|(id, note)| (id.clone(), note)));
        self.resolver = Resolver::new(self.parsed.keys());
    }

    /// Applies the changes that the watcher found.
    pub fn apply(&mut self, events: &[VaultEvent]) -> Result<Changes> {
        let mut changes = Changes::default();

        for event in events {
            match event {
                VaultEvent::Changed(id) => {
                    if self.vault.refresh(id)?.is_none() {
                        continue;
                    }
                    let content = self.vault.read(id)?;

                    // A write from the app itself needs no second parse: the
                    // content is already known.
                    if self.hashes.get(id) == Some(&hash(&content)) {
                        continue;
                    }

                    self.ingest(id, &content)?;
                    self.edit_bookmarks(|bookmarks| bookmarks.reanchor(id, &content))?;
                    changes.updated.push(id.clone());
                }
                VaultEvent::Removed(id) => {
                    self.vault.refresh(id)?;
                    if self.parsed.remove(id).is_some() {
                        self.hashes.remove(id);
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

    /// Syncs the vault with the remote storage.
    ///
    /// Notes from the cloud go the same way as edits from an external editor:
    /// for the app, sync is one more source of changes on disk.
    pub fn sync<P: SyncProvider>(&mut self, engine: &SyncEngine<P>) -> Result<SyncReport> {
        let report = engine.run(&mut self.vault)?;
        self.absorb(&report)?;
        Ok(report)
    }

    /// Prepares a sync: the plan and the content to upload.
    ///
    /// This is the first of three parts. The network part runs without the
    /// vault: the window releases the lock for that time, so an edit does not
    /// wait for sync. The CLI shares nothing, so it uses [`Workspace::sync`].
    pub fn sync_stage<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<Staged> {
        Ok(engine.stage(&self.vault)?)
    }

    /// Writes what sync downloaded and updates the parse, graph and search.
    pub fn sync_finish<P: SyncProvider>(
        &mut self,
        engine: &SyncEngine<P>,
        fetched: Fetched,
    ) -> Result<SyncReport> {
        let report = engine.commit_local(&mut self.vault, fetched)?;
        self.absorb(&report)?;
        Ok(report)
    }

    /// Notes that sync wrote go the same way as edits from an external editor.
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
        Ok(())
    }

    /// Copies that conflicts left; the user must resolve them.
    ///
    /// They come from files in the directory, not from the sync log: a copy is
    /// a file, so the list survives a restart and always matches the disk.
    pub fn conflicts(&self) -> Vec<&NoteId> {
        self.parsed
            .keys()
            .filter(|id| kknow_sync::is_conflict_copy(id))
            .collect()
    }

    /// What sync would do now, without changes.
    pub fn sync_plan<P: SyncProvider>(&self, engine: &SyncEngine<P>) -> Result<SyncPlan> {
        Ok(engine.plan(&self.vault)?)
    }

    /// Themes of this vault together with shared ones from `global`.
    ///
    /// They are read from disk each time: people edit a theme in an editor next
    /// to the app and do not want to restart for a new shade. The shared path
    /// comes from outside for the same reason as the commands path: tests must
    /// not read the real folder of the person who runs them.
    pub fn themes(&self, global: Option<&Path>) -> ThemeSet {
        theme::load(Some(self.vault.root()), global)
    }

    /// Vault summary.
    ///
    /// The size comes from disk, not from the parsed text: the user compares it
    /// with the file manager. A file that cannot be read is not counted: a
    /// summary must not fail because of one note on an unplugged disk.
    pub fn stats(&self) -> VaultStats {
        let mut folders: BTreeSet<&str> = BTreeSet::new();
        let mut bytes = 0;

        for id in self.parsed.keys() {
            if let Some(folder) = id.folder() {
                // `a/b/c` is in both `a` and `a/b`: the tree shows both levels,
                // so count them the same way.
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
        Ok(self.vault.read(id)?)
    }

    /// Finds a note by ID, or by file name if the ID does not match.
    /// So `kknow show kknow` finds `Projects/Rust/kknow`.
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

    /// Saves the note and updates the parse, graph and search.
    pub fn save(&mut self, id: &NoteId, content: &str) -> Result<()> {
        self.store(id, content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(())
    }

    /// Writes the note and parses it. The caller commits the search and
    /// rebuilds the graph.
    fn store(&mut self, id: &NoteId, content: &str) -> Result<()> {
        self.vault.write(id, content)?;
        self.ingest(id, content)?;
        self.edit_bookmarks(|bookmarks| bookmarks.reanchor(id, content))
    }

    /// Saves only if the file on disk is still `base`, the text the editor
    /// started from.
    ///
    /// Otherwise something else changed the note (an external editor, an agent,
    /// git), and a blind write would erase those edits. Then the disk stays as
    /// is, and `content` goes to a conflict copy, the same way sync does it.
    ///
    /// A tiny gap stays between the read and the write: the file system cannot
    /// compare and write in one step. But this protects against the main case,
    /// an edit made while the note was open.
    pub fn save_if_unchanged(&mut self, id: &NoteId, content: &str, base: &str) -> Result<Saved> {
        let path = self.path_for(id);
        let disk = match std::fs::read_to_string(&path) {
            Ok(disk) => Some(disk),
            // Deleted outside: the write brings the note back and keeps the edits.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(Error::io(&path, source)),
        };

        match disk {
            Some(disk) if disk != base && disk != content => {
                let copy = conflict_copy(id, content);
                self.save(&copy, content)?;
                Ok(Saved::Conflict { copy, disk })
            }
            _ => {
                self.save(id, content)?;
                Ok(Saved::Written)
            }
        }
    }

    /// Creates an empty note with a heading from its name.
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
        self.search.remove(id)?;
        self.search.commit()?;
        self.rebuild_graph();
        self.edit_bookmarks(|bookmarks| {
            bookmarks.forget(id);
        })
    }

    /// Moves the note to a new ID: a new name, a new folder or both.
    ///
    /// Links to the note follow it: see [`Workspace::relink`].
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
        let relinked = self.relink(&moves)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    /// Copies the note next to it with the first free name: `Name copy`, then
    /// `Name copy 2`, `copy 3`… Returns the ID of the copy.
    pub fn duplicate(&mut self, id: &NoteId) -> Result<NoteId> {
        let copy = self.free_copy_name(id)?;
        self.vault.copy(id, &copy)?;
        let content = self.vault.read(&copy)?;
        self.ingest(&copy, &content)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(copy)
    }

    /// Moves a folder with all its notes; search and graph get the new IDs.
    /// Links to the notes follow them: see [`Workspace::relink`].
    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<Relinked> {
        let moved = self.vault.rename_folder(from, to)?;
        for (old, new) in &moved {
            self.moved(old, new)?;
        }
        self.edit_bookmarks(|bookmarks| bookmarks.rename(&moved))?;
        if moved.is_empty() {
            return Ok(Relinked::default());
        }
        let relinked = self.relink(&moved)?;
        self.search.commit()?;
        self.rebuild_graph();
        Ok(relinked)
    }

    /// Rewrites links that the moves broke or turned to a different note.
    ///
    /// Call it after [`Workspace::moved`] and before the graph rebuild: the old
    /// graph and resolver tell where each link pointed. Only possible sources
    /// are read: the moved notes, their backlinks and the backlinks of notes
    /// with a new name. Other links cannot change their target.
    ///
    /// The notes go through [`Workspace::store`], so the watcher sees our own
    /// write, and search and graph get the new text.
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

    /// Records the move of one note. The index and the graph use the ID, so
    /// remove the old entry and parse the new one. The caller rebuilds the
    /// graph once for the whole operation.
    fn moved(&mut self, from: &NoteId, to: &NoteId) -> Result<()> {
        self.parsed.remove(from);
        self.hashes.remove(from);
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
        unreachable!("бесконечный перебор имён закончился")
    }

    /// Vault folders, including empty ones: notes alone cannot give them.
    pub fn folders(&self) -> Result<Vec<String>> {
        Ok(self.vault.folders()?)
    }

    pub fn create_folder(&self, raw: &str) -> Result<()> {
        Ok(self.vault.create_folder(raw)?)
    }

    /// Deletes a folder with all its notes and removes them from search and
    /// graph.
    pub fn delete_folder(&mut self, raw: &str) -> Result<()> {
        let removed = self.vault.remove_folder(raw)?;
        for id in &removed {
            self.parsed.remove(id);
            self.hashes.remove(id);
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

    pub fn backlinks(&self, id: &NoteId) -> &[Edge] {
        self.graph.backlinks(id)
    }

    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.graph.outgoing(id)
    }

    /// Renders the text as note `id`, with links resolved against the vault.
    ///
    /// The text is a separate argument, so the preview can show unsaved edits.
    pub fn render(&self, id: &NoteId, content: &str) -> String {
        kknow_index::to_html(content, id, &self.resolver)
    }

    /// Image that note `from` refers to: a path from the note, a path from the
    /// root, or a file name anywhere in the vault. It never leaves the vault.
    pub fn image(&self, from: &NoteId, target: &str) -> Option<PathBuf> {
        let note = self.path_for(from);
        let dir = note.parent().unwrap_or(self.root());
        kknow_core::image::resolve(self.root(), dir, target)
    }

    /// Resolves a link target relative to the note.
    pub fn resolve(&self, target: &str, from: &NoteId) -> Option<NoteId> {
        self.resolver.resolve(target, from)
    }
}

/// Name of the copy for text that did not go into the file: the note ID,
/// [`CONFLICT_MARK`] and a short hash.
///
/// A hash, not a time: a second save of the same text goes to the same copy
/// and does not create new ones.
fn conflict_copy(id: &NoteId, content: &str) -> NoteId {
    let short = format!("{:016x}", hash(content));
    NoteId::parse(&format!("{id}{CONFLICT_MARK}{})", &short[..8]))
        .expect("имя копии отличается от исходного только суффиксом")
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
        // The new nested path gives three folders, because intermediate levels
        // count; plus `Projects` from the fixture. A note in the root adds no
        // folder.
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
    fn rename_moves_the_note_in_search_and_graph() {
        let (_dir, mut workspace) = workspace();
        let from = NoteId::parse("Projects/kknow").unwrap();
        let to = NoteId::parse("Архив/kknow").unwrap();

        workspace.rename(&from, &to).unwrap();

        assert!(!workspace.contains(&from));
        assert!(workspace.contains(&to));
        assert_eq!(found(&workspace, "синхронизацию"), ["Архив/kknow"]);
        // `[[Projects/kknow]]` in the index note follows the note.
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

        // The short link still finds the note, so the file stays as it was.
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
        // The rewrite is our own write: the watcher must not report it.
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
        // `[[Projects/kknow]]` is the tail of the new path, so the link stays
        // valid.
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
        // The watcher still reports the write. It must not look like a change,
        // or the UI reloads the note while the user types.
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
}
