use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use kknow_app::Workspace;
use kknow_app::files::OpenFiles;
use kknow_app::vaults::{LabelColor, VaultEntry, VaultRegistry};
use kknow_run::CommandSet;
use serde::{Serialize, Serializer};

use crate::files::Opened;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("каталог с заметками не открыт")]
    NoVault,

    #[error("заметка не найдена: {0}")]
    NotFound(String),

    #[error("синхронизация не настроена")]
    NotConfigured,

    #[error("не удалось показать файл: {0}")]
    Reveal(String),

    #[error("не удалось открыть файл: {0}")]
    Open(String),

    #[error("не найден каталог настроек: команды и общие темы негде хранить")]
    NoConfig,

    #[error("не удалось сделать тему общей: {0}")]
    ShareTheme(String),

    #[error("файл не открыт в kknow: {0}")]
    UnknownFile(String),

    #[error("файла больше нет, он убран из списка: {0}")]
    FileGone(String),

    #[error("картинка не найдена: {0}")]
    ImageNotFound(String),

    #[error("не удалось открыть окно: {0}")]
    Window(String),

    // In the test build, secrets live in memory, so this error cannot happen.
    #[cfg_attr(test, allow(dead_code))]
    #[error("связка ключей: {0}")]
    Keychain(String),

    #[error(transparent)]
    App(#[from] kknow_app::Error),

    #[error(transparent)]
    Sync(#[from] kknow_sync::Error),

    #[error(transparent)]
    Run(#[from] kknow_run::Error),
}

/// The error goes to the frontend as a string: that is all the status needs.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// An open vault: the registry entry and the parsed workspace.
///
/// Each vault has its own lock, not one lock for the app. A network sync in
/// one vault must not delay a note save in another.
pub struct VaultSlot {
    pub entry: VaultEntry,
    workspace: Mutex<Workspace>,
    /// The vault was removed from the list: the watcher must exit.
    closed: AtomicBool,
}

impl VaultSlot {
    fn open(entry: VaultEntry) -> Result<Self> {
        let workspace = Workspace::open(&entry.path)?;
        Ok(VaultSlot {
            entry,
            workspace: Mutex::new(workspace),
            closed: AtomicBool::new(false),
        })
    }

    pub fn read<R>(&self, action: impl FnOnce(&Workspace) -> Result<R>) -> Result<R> {
        action(&self.lock())
    }

    pub fn write<R>(&self, action: impl FnOnce(&mut Workspace) -> Result<R>) -> Result<R> {
        action(&mut self.lock())
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A poisoned mutex means a panic in another command. The state is not
    /// reliable, so we must not continue with it.
    fn lock(&self) -> MutexGuard<'_, Workspace> {
        self.workspace
            .lock()
            .expect("состояние хранилища испорчено")
    }
}

/// App vaults: the list from the registry and the vaults that are open.
///
/// Vaults open lazily (on switch, search or sync) and do not close again. Each
/// has its own search index in memory, and a rebuild on each visit costs more
/// than keeping it.
#[derive(Default)]
pub struct AppState {
    /// Where to write the registry. `None` means nowhere (tests use this).
    registry_file: Mutex<Option<PathBuf>>,
    /// Where to read commands from. `None` means no commands.
    ///
    /// The caller sets the path; this code does not look it up. The real file
    /// is in the settings of the person who runs the tests, and `cargo test`
    /// must not touch it. The keyring has the same reason.
    commands_file: Mutex<Option<PathBuf>>,
    /// Global themes folder. `None` means no global themes; same reason as for
    /// commands.
    themes_dir: Mutex<Option<PathBuf>>,
    registry: Mutex<VaultRegistry>,
    open: Mutex<HashMap<String, Arc<VaultSlot>>>,
    /// What to start for a vault that just opened.
    ///
    /// The watcher needs an `AppHandle`. It comes with the window and is generic
    /// over the runtime. Storing it in the state would make all commands
    /// generic over the runtime, so a closure goes here at start.
    #[allow(clippy::type_complexity)]
    on_open: Mutex<Option<Box<dyn Fn(Arc<VaultSlot>) + Send + Sync>>>,
    /// Where to write the loose files list. `None` means nowhere (tests).
    files_file: Mutex<Option<PathBuf>>,
    /// Loose files opened without a vault.
    files: Mutex<OpenFiles>,
    /// An "open" request from outside that the window did not take yet.
    ///
    /// On a cold start, a double click on a file arrives before the window
    /// loads and listens to events, so the event is lost. The request waits
    /// here until the window takes it: on load or on the event, whichever
    /// comes first.
    pending: Mutex<Option<Opened>>,
}

impl AppState {
    /// Sets the loaded registry and the file to write it to.
    /// `None` for the file means write nothing: tests use this.
    pub fn use_registry(&self, file: Option<PathBuf>, registry: VaultRegistry) {
        *self.registry_file.lock().expect("состояние испорчено") = file;
        *self.registry.lock().expect("состояние испорчено") = registry;
    }

    /// Sets the commands file; `None` means the app has no commands.
    pub fn use_commands(&self, file: Option<PathBuf>) {
        *self.commands_file.lock().expect("состояние испорчено") = file;
    }

    /// Sets the global themes folder; `None` means no global themes.
    pub fn use_themes(&self, dir: Option<PathBuf>) {
        *self.themes_dir.lock().expect("состояние испорчено") = dir;
    }

    pub fn themes_path(&self) -> Option<PathBuf> {
        self.themes_dir.lock().expect("состояние испорчено").clone()
    }

    /// Path of the commands file, to show in the file manager.
    pub fn commands_path(&self) -> Option<PathBuf> {
        self.commands_file
            .lock()
            .expect("состояние испорчено")
            .clone()
    }

    /// Commands of this machine.
    ///
    /// They are read from disk on each request, not kept in memory. Users edit
    /// the file by hand in another window and will not restart the app for a
    /// new command. Themes are read the same way.
    pub fn commands(&self) -> Result<CommandSet> {
        let file = self.commands_file.lock().expect("состояние испорчено");
        let Some(path) = file.as_ref() else {
            return Ok(CommandSet::default());
        };
        Ok(CommandSet::load(path)?)
    }

    pub fn on_open(&self, hook: impl Fn(Arc<VaultSlot>) + Send + Sync + 'static) {
        *self.on_open.lock().expect("состояние испорчено") = Some(Box::new(hook));
    }

    pub fn entries(&self) -> Vec<VaultEntry> {
        self.registry().entries().to_vec()
    }

    pub fn active(&self) -> Option<VaultEntry> {
        self.registry().active().cloned()
    }

    pub fn is_open(&self) -> bool {
        self.active().is_some()
    }

    /// Vault ID by its ⌘1…⌘9 number.
    pub fn by_hotkey(&self, number: usize) -> Option<String> {
        self.registry()
            .by_hotkey(number)
            .map(|entry| entry.id.clone())
    }

    pub fn set_active(&self, id: &str) -> Result<VaultEntry> {
        let mut registry = self.registry();
        registry.set_active(id)?;
        let entry = registry.get(id).cloned().expect("только что проверено");
        drop(registry);

        self.persist();
        Ok(entry)
    }

    pub fn add(&self, path: PathBuf, name: Option<String>) -> Result<VaultEntry> {
        let mut registry = self.registry();
        let entry = registry.add(&path, name)?.clone();
        drop(registry);

        self.persist();
        Ok(entry)
    }

    /// Forgets a vault: removes the registry entry and the slot in memory, and
    /// stops the watcher. Files on disk stay.
    pub fn remove(&self, id: &str) -> Result<VaultEntry> {
        let mut registry = self.registry();
        let removed = registry.remove(id)?;
        drop(registry);

        if let Some(slot) = self.open.lock().expect("состояние испорчено").remove(id)
        {
            slot.close();
        }
        self.persist();
        Ok(removed)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<()> {
        self.registry().rename(id, name)?;
        self.persist();
        Ok(())
    }

    /// Changes the label color. The color name comes from the window. We refuse
    /// unknown names, otherwise the label becomes invisible.
    pub fn set_color(&self, id: &str, color: &str) -> Result<()> {
        let color = LabelColor::ALL
            .into_iter()
            .find(|known| known.as_str() == color)
            .ok_or_else(|| kknow_app::Error::UnknownVault(color.to_string()))?;

        self.registry().set_color(id, color)?;
        self.persist();
        Ok(())
    }

    /// Vault by ID; `None` means the active vault. Opens a closed vault.
    pub fn slot(&self, id: Option<&str>) -> Result<Arc<VaultSlot>> {
        let entry = match id {
            Some(id) => self
                .registry()
                .get(id)
                .cloned()
                .ok_or_else(|| kknow_app::Error::UnknownVault(id.to_string()))?,
            None => self.active().ok_or(Error::NoVault)?,
        };

        if let Some(slot) = self.opened(&entry.id) {
            return Ok(slot);
        }

        // Parse and index without the lock on the open map. Otherwise opening a
        // large vault delays commands to all other vaults.
        let fresh = Arc::new(VaultSlot::open(entry.clone())?);

        let mut open = self.open.lock().expect("состояние испорчено");
        // Someone else can open the vault while we read the disk. Then their
        // slot wins: two `Workspace`s for one folder mean two watchers and two
        // inconsistent parsed copies.
        if let Some(existing) = open.get(&entry.id) {
            return Ok(existing.clone());
        }
        open.insert(entry.id.clone(), fresh.clone());
        drop(open);

        if let Some(hook) = self.on_open.lock().expect("состояние испорчено").as_ref()
        {
            hook(fresh.clone());
        }
        Ok(fresh)
    }

    /// An open vault, which we can query without a disk read.
    pub fn opened(&self, id: &str) -> Option<Arc<VaultSlot>> {
        self.open
            .lock()
            .expect("состояние испорчено")
            .get(id)
            .cloned()
    }

    /// Vault order for a search in all vaults: the active vault first.
    ///
    /// Tantivy scores from different indexes are not strictly comparable, so
    /// results show in groups by vault, not in one mixed list.
    pub fn search_order(&self) -> Vec<VaultEntry> {
        let registry = self.registry();
        let active = registry.active().map(|entry| entry.id.clone());

        let mut order: Vec<VaultEntry> = registry.entries().to_vec();
        order.sort_by_key(|entry| Some(&entry.id) != active.as_ref());
        order
    }

    /// Sets the loaded loose files list and the file to write it to.
    pub fn use_files(&self, file: Option<PathBuf>, files: OpenFiles) {
        *self.files_file.lock().expect("состояние испорчено") = file;
        *self.files.lock().expect("состояние испорчено") = files;
    }

    /// Loose files. Files missing from disk are forgotten here.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files = self.files_list();
        let forgotten = files.prune();
        let entries = files.entries().to_vec();
        drop(files);

        if !forgotten.is_empty() {
            self.persist_files();
        }
        entries
    }

    /// Remembers a loose file and returns its canonical path.
    pub fn remember_file(&self, path: &std::path::Path) -> Result<PathBuf> {
        let added = self.files_list().add(path)?;
        self.persist_files();
        Ok(added)
    }

    /// Forgets a loose file. The file stays on disk.
    pub fn forget_file(&self, path: &std::path::Path) {
        if self.files_list().remove(path) {
            self.persist_files();
        }
    }

    /// Path of a loose file, if it is in the list.
    ///
    /// The window can read and write only opened files: picked in a dialog,
    /// dropped on the window, or opened from Finder. Otherwise the save command
    /// could write to any path the frontend sends.
    pub fn known_file(&self, raw: &str) -> Result<PathBuf> {
        let path = std::path::Path::new(raw);
        if !self.files_list().contains(path) {
            return Err(Error::UnknownFile(raw.to_string()));
        }
        if !path.is_file() {
            self.forget_file(path);
            return Err(Error::FileGone(raw.to_string()));
        }
        Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
    }

    /// The vault that contains the file, and the note ID of the file there.
    pub fn locate(&self, path: &std::path::Path) -> Option<(String, String)> {
        self.registry()
            .locate(path)
            .map(|(entry, id)| (entry.id.clone(), id.as_str().to_string()))
    }

    /// Keeps an "open this" request until the window takes it.
    pub fn set_pending(&self, opened: Opened) {
        *self.pending.lock().expect("состояние испорчено") = Some(opened);
    }

    /// Takes the pending request. A second call returns nothing.
    pub fn take_pending(&self) -> Option<Opened> {
        self.pending.lock().expect("состояние испорчено").take()
    }

    fn files_list(&self) -> MutexGuard<'_, OpenFiles> {
        self.files.lock().expect("состояние испорчено")
    }

    /// Writes the loose files list. A write error goes to the log, as for the
    /// registry.
    fn persist_files(&self) {
        let file = self.files_file.lock().expect("состояние испорчено");
        let Some(path) = file.as_ref() else {
            return;
        };
        if let Err(error) = self.files_list().save(path) {
            eprintln!("не удалось записать список открытых файлов: {error}");
        }
    }

    fn registry(&self) -> MutexGuard<'_, VaultRegistry> {
        self.registry.lock().expect("состояние испорчено")
    }

    /// Writes the registry. A write error must not fail an action that already
    /// happened in memory, so we report it to the log.
    fn persist(&self) {
        let file = self.registry_file.lock().expect("состояние испорчено");
        let Some(path) = file.as_ref() else {
            return;
        };
        if let Err(error) = self.registry().save(path) {
            eprintln!("не удалось записать список хранилищ: {error}");
        }
    }
}

/// A state with a ready vault list, for command tests.
#[cfg(test)]
pub fn testing(roots: &[&std::path::Path]) -> AppState {
    let state = AppState::default();
    let mut registry = VaultRegistry::default();
    for root in roots {
        registry.add(root, None).expect("каталог добавлен в реестр");
    }
    // No file: a test must leave nothing in the settings of the person who
    // runs it.
    state.use_registry(None, registry);
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn vault(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(format!("{name}.md")), format!("# {name}\n")).unwrap();
        dir
    }

    #[test]
    fn vaults_open_lazily_and_stay_open() {
        let first = vault("Первое");
        let second = vault("Второе");
        let state = testing(&[first.path(), second.path()]);

        let ids: Vec<String> = state.entries().iter().map(|e| e.id.clone()).collect();
        assert!(
            state.opened(&ids[0]).is_none(),
            "до обращения ничего не open"
        );

        let slot = state.slot(None).expect("активное открывается");
        assert_eq!(slot.entry.id, ids[0]);
        assert!(state.opened(&ids[0]).is_some());
        // A second call returns the same slot: two parses of one folder mean
        // two watchers and two diverged copies.
        assert!(Arc::ptr_eq(&slot, &state.slot(Some(&ids[0])).unwrap()));
        assert!(state.opened(&ids[1]).is_none(), "чужое не открывалось");
    }

    #[test]
    fn each_vault_has_its_own_lock() {
        let first = vault("Первое");
        let second = vault("Второе");
        let state = testing(&[first.path(), second.path()]);
        let ids: Vec<String> = state.entries().iter().map(|e| e.id.clone()).collect();

        let busy = state.slot(Some(&ids[0])).unwrap();
        let free = state.slot(Some(&ids[1])).unwrap();

        // One vault is busy for a long time, as during a sync.
        let holder = std::thread::spawn(move || {
            busy.write(|_| {
                std::thread::sleep(Duration::from_millis(400));
                Ok(())
            })
            .unwrap();
        });
        std::thread::sleep(Duration::from_millis(50));

        let started = Instant::now();
        free.read(|workspace| Ok(workspace.len())).unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "занятое хранилище задержало соседнее"
        );

        holder.join().unwrap();
    }

    #[test]
    fn forgetting_a_vault_closes_its_slot() {
        let first = vault("Первое");
        let second = vault("Второе");
        let state = testing(&[first.path(), second.path()]);
        let ids: Vec<String> = state.entries().iter().map(|e| e.id.clone()).collect();
        let slot = state.slot(Some(&ids[0])).unwrap();

        state.remove(&ids[0]).unwrap();

        assert!(slot.is_closed(), "наблюдателю пора выходить");
        assert!(state.opened(&ids[0]).is_none());
        // The other vault becomes active: the window must show something.
        assert_eq!(state.active().unwrap().id, ids[1]);
    }

    #[test]
    fn search_starts_with_the_active_vault() {
        let first = vault("Первое");
        let second = vault("Второе");
        let state = testing(&[first.path(), second.path()]);
        let ids: Vec<String> = state.entries().iter().map(|e| e.id.clone()).collect();

        state.set_active(&ids[1]).unwrap();
        let order: Vec<String> = state
            .search_order()
            .into_iter()
            .map(|entry| entry.id)
            .collect();

        assert_eq!(order, [ids[1].clone(), ids[0].clone()]);
    }

    #[test]
    fn an_empty_registry_has_no_active_vault() {
        let state = testing(&[]);

        assert!(!state.is_open());
        assert!(matches!(state.slot(None), Err(Error::NoVault)));
    }
}
