use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use kknow_app::Workspace;
use kknow_app::files::OpenFiles;
use kknow_app::vaults::{LabelColor, VaultEntry, VaultRegistry};
use kknow_run::CommandSet;
use serde::{Serialize, Serializer};

use crate::errors::ErrorView;
use crate::files::Opened;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no notes folder is open")]
    NoVault,

    #[error("note is not found: {0}")]
    NotFound(String),

    #[error("sync is not set up")]
    NotConfigured,

    #[error("a sync of this vault is already running")]
    SyncBusy,

    #[error("cannot show the file: {0}")]
    Reveal(String),

    #[error("cannot open the file: {0}")]
    Open(String),

    #[error("cannot find the settings folder: there is no place for commands and shared themes")]
    NoConfig,

    #[error("cannot share the theme: {0}")]
    ShareTheme(String),

    #[error("the file is not open in kknow: {0}")]
    UnknownFile(String),

    #[error("the file is gone and is removed from the list: {0}")]
    FileGone(String),

    #[error("image is not found: {0}")]
    ImageNotFound(String),

    #[error("the image did not come as bytes")]
    NoImageBytes,

    #[error("cannot open the window: {0}")]
    Window(String),

    #[error("the new version is not downloaded yet")]
    UpdateNotReady,

    #[error("cannot install the update: {0}")]
    Update(String),

    #[cfg_attr(test, allow(dead_code))]
    #[error("keychain: {0}")]
    Keychain(String),

    #[error("no path to git: set it in Settings, General")]
    GitPathNotSet,

    #[error(
        "the repository token is not found: enter it in the vault settings or choose another sign-in method"
    )]
    NoGitToken,

    #[error(transparent)]
    App(#[from] kknow_app::Error),

    #[error(transparent)]
    Sync(#[from] kknow_sync::Error),

    #[error(transparent)]
    Run(#[from] kknow_run::Error),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorView::from(self).serialize(serializer)
    }
}

pub struct VaultSlot {
    pub entry: VaultEntry,
    workspace: RwLock<Workspace>,
    closed: AtomicBool,
    syncing: AtomicBool,
}

pub struct SyncTurn<'a> {
    flag: &'a AtomicBool,
}

impl Drop for SyncTurn<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

impl VaultSlot {
    fn open(entry: VaultEntry) -> Result<Self> {
        let workspace = Workspace::open(&entry.path)?;
        Ok(VaultSlot {
            entry,
            workspace: RwLock::new(workspace),
            closed: AtomicBool::new(false),
            syncing: AtomicBool::new(false),
        })
    }

    pub fn read<R>(&self, action: impl FnOnce(&Workspace) -> Result<R>) -> Result<R> {
        action(&self.reader())
    }

    pub fn write<R>(&self, action: impl FnOnce(&mut Workspace) -> Result<R>) -> Result<R> {
        action(&mut self.writer())
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn begin_sync(&self) -> Option<SyncTurn<'_>> {
        self.syncing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| SyncTurn {
                flag: &self.syncing,
            })
    }

    pub fn is_syncing(&self) -> bool {
        self.syncing.load(Ordering::SeqCst)
    }

    fn reader(&self) -> RwLockReadGuard<'_, Workspace> {
        self.workspace.read().expect("the vault state is poisoned")
    }

    fn writer(&self) -> RwLockWriteGuard<'_, Workspace> {
        self.workspace.write().expect("the vault state is poisoned")
    }
}

#[derive(Default)]
pub struct AppState {
    registry_file: OnceLock<Option<PathBuf>>,
    commands_file: OnceLock<Option<PathBuf>>,
    themes_dir: OnceLock<Option<PathBuf>>,
    registry: Mutex<VaultRegistry>,
    open: Mutex<HashMap<String, Arc<VaultSlot>>>,
    #[allow(clippy::type_complexity)]
    on_open: OnceLock<Box<dyn Fn(Arc<VaultSlot>) + Send + Sync>>,
    files_file: OnceLock<Option<PathBuf>>,
    files: Mutex<OpenFiles>,
    pending: Mutex<Option<Opened>>,
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn start_run(&self, id: &str) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        lock(&self.runs).insert(id.to_string(), stop.clone());
        stop
    }

    pub fn end_run(&self, id: &str) {
        lock(&self.runs).remove(id);
    }

    pub fn stop_run(&self, id: &str) -> bool {
        match lock(&self.runs).get(id) {
            Some(stop) => {
                stop.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn use_registry(&self, file: Option<PathBuf>, registry: VaultRegistry) {
        set_once(&self.registry_file, file);
        *lock(&self.registry) = registry;
    }

    pub fn use_commands(&self, file: Option<PathBuf>) {
        set_once(&self.commands_file, file);
    }

    pub fn use_themes(&self, dir: Option<PathBuf>) {
        set_once(&self.themes_dir, dir);
    }

    pub fn themes_path(&self) -> Option<PathBuf> {
        self.themes_dir.get().cloned().flatten()
    }

    pub fn commands_path(&self) -> Option<PathBuf> {
        self.commands_file.get().cloned().flatten()
    }

    pub fn commands(&self) -> Result<CommandSet> {
        let Some(path) = self.commands_file.get().and_then(Option::as_ref) else {
            return Ok(CommandSet::default());
        };
        Ok(CommandSet::load(path)?)
    }

    pub fn on_open(&self, hook: impl Fn(Arc<VaultSlot>) + Send + Sync + 'static) {
        if self.on_open.set(Box::new(hook)).is_err() {
            panic!("the open handler is set once");
        }
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

    pub fn by_hotkey(&self, number: usize) -> Option<String> {
        self.registry()
            .by_hotkey(number)
            .map(|entry| entry.id.clone())
    }

    pub fn set_active(&self, id: &str) -> Result<VaultEntry> {
        let mut registry = self.registry();
        registry.set_active(id)?;
        let entry = registry.get(id).cloned().expect("just checked");
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

    pub fn remove(&self, id: &str) -> Result<VaultEntry> {
        let mut registry = self.registry();
        let removed = registry.remove(id)?;
        drop(registry);

        if let Some(slot) = lock(&self.open).remove(id) {
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

    pub fn set_color(&self, id: &str, color: &str) -> Result<()> {
        let color = LabelColor::ALL
            .into_iter()
            .find(|known| known.as_str() == color)
            .ok_or_else(|| kknow_app::Error::UnknownVault(color.to_string()))?;

        self.registry().set_color(id, color)?;
        self.persist();
        Ok(())
    }

    pub fn set_labels(&self, id: &str, labels: &[String]) -> Result<()> {
        self.registry()
            .set_labels(id, labels.iter().map(String::as_str))?;
        self.persist();
        Ok(())
    }

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

        let fresh = Arc::new(VaultSlot::open(entry.clone())?);

        let mut open = lock(&self.open);
        if let Some(existing) = open.get(&entry.id) {
            return Ok(existing.clone());
        }
        open.insert(entry.id.clone(), fresh.clone());
        drop(open);

        if let Some(hook) = self.on_open.get() {
            hook(fresh.clone());
        }
        Ok(fresh)
    }

    pub fn opened(&self, id: &str) -> Option<Arc<VaultSlot>> {
        lock(&self.open).get(id).cloned()
    }

    pub fn search_order(&self) -> Vec<VaultEntry> {
        let registry = self.registry();
        let active = registry.active().map(|entry| entry.id.clone());

        let mut order: Vec<VaultEntry> = registry.entries().to_vec();
        order.sort_by_key(|entry| Some(&entry.id) != active.as_ref());
        order
    }

    pub fn use_files(&self, file: Option<PathBuf>, files: OpenFiles) {
        set_once(&self.files_file, file);
        *lock(&self.files) = files;
    }

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

    pub fn remember_file(&self, path: &std::path::Path) -> Result<PathBuf> {
        let added = self.files_list().add(path)?;
        self.persist_files();
        Ok(added)
    }

    pub fn forget_file(&self, path: &std::path::Path) {
        if self.files_list().remove(path) {
            self.persist_files();
        }
    }

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

    pub fn locate(&self, path: &std::path::Path) -> Option<(String, String)> {
        self.registry()
            .locate(path)
            .map(|(entry, id)| (entry.id.clone(), id.as_str().to_string()))
    }

    pub fn set_pending(&self, opened: Opened) {
        *lock(&self.pending) = Some(opened);
    }

    pub fn take_pending(&self) -> Option<Opened> {
        lock(&self.pending).take()
    }

    fn files_list(&self) -> MutexGuard<'_, OpenFiles> {
        lock(&self.files)
    }

    fn persist_files(&self) {
        let Some(path) = self.files_file.get().and_then(Option::as_ref) else {
            return;
        };
        if let Err(error) = self.files_list().save(path) {
            eprintln!("cannot write the list of open files: {error}");
        }
    }

    fn registry(&self) -> MutexGuard<'_, VaultRegistry> {
        lock(&self.registry)
    }

    fn persist(&self) {
        let Some(path) = self.registry_file.get().and_then(Option::as_ref) else {
            return;
        };
        if let Err(error) = self.registry().save(path) {
            eprintln!("cannot write the vault list: {error}");
        }
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("the state is poisoned")
}

fn set_once(cell: &OnceLock<Option<PathBuf>>, value: Option<PathBuf>) {
    if cell.set(value).is_err() {
        panic!("the path is set once, at start");
    }
}

#[cfg(test)]
pub fn testing(roots: &[&std::path::Path]) -> AppState {
    let state = AppState::default();
    let mut registry = VaultRegistry::default();
    for root in roots {
        registry
            .add(root, None)
            .expect("the folder is added to the registry");
    }
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

    #[test]
    fn only_one_sync_turn_lives_at_a_time() {
        let dir = vault("Turn");
        let state = testing(&[dir.path()]);
        let slot = state.slot(None).unwrap();

        let turn = slot.begin_sync().expect("the first turn starts");
        assert!(slot.is_syncing());
        assert!(slot.begin_sync().is_none());

        drop(turn);
        assert!(!slot.is_syncing());
        assert!(slot.begin_sync().is_some());
    }

    #[test]
    fn a_turn_ends_when_the_sync_panics() {
        let dir = vault("Panic");
        let state = testing(&[dir.path()]);
        let slot = state.slot(None).unwrap();

        let copy = slot.clone();
        let result = std::thread::spawn(move || {
            let _turn = copy.begin_sync().unwrap();
            panic!("sync failed");
        })
        .join();

        assert!(result.is_err());
        assert!(!slot.is_syncing());
    }

    #[test]
    fn readers_of_a_vault_run_in_parallel() {
        fn assert_sync<T: Send + Sync>() {}
        assert_sync::<Workspace>();

        let dir = vault("Parallel");
        let state = testing(&[dir.path()]);
        let slot = state.slot(None).unwrap();
        let (tell, hear) = std::sync::mpsc::channel::<()>();

        let waiting = slot.clone();
        let first = std::thread::spawn(move || {
            waiting
                .read(|_| Ok(hear.recv_timeout(Duration::from_secs(2)).is_ok()))
                .unwrap()
        });
        std::thread::sleep(Duration::from_millis(200));
        tell.send(()).unwrap();

        assert!(first.join().unwrap(), "the second reader got in");
    }
}
