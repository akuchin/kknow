use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use kknow_core::{NoteId, is_markdown_extension};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, NoCache, new_debouncer_opt};

const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VaultEvent {
    Changed(NoteId),
    Removed(NoteId),
}

pub struct VaultWatcher {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
    events: Receiver<DebounceEventResult>,
    root: PathBuf,
}

impl VaultWatcher {
    pub fn start(root: &Path) -> notify::Result<Self> {
        let root = root.canonicalize()?;

        let (sender, events) = channel();
        let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, _>(
            DEBOUNCE,
            None,
            sender,
            NoCache,
            notify::Config::default(),
        )?;
        debouncer.watch(&root, RecursiveMode::Recursive)?;

        Ok(VaultWatcher {
            _debouncer: debouncer,
            events,
            root,
        })
    }

    pub fn recv(&self) -> Option<Vec<VaultEvent>> {
        loop {
            let batch = self.events.recv().ok()?;
            let events = self.convert(batch);
            if !events.is_empty() {
                return Some(events);
            }
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<VaultEvent>> {
        let batch = self.events.recv_timeout(timeout).ok()?;

        let mut events = self.convert(batch);
        events.extend(self.try_recv());
        events.sort();
        events.dedup();

        (!events.is_empty()).then_some(events)
    }

    pub fn try_recv(&self) -> Vec<VaultEvent> {
        let mut events = BTreeSet::new();
        while let Ok(batch) = self.events.try_recv() {
            events.extend(self.convert(batch));
        }
        events.into_iter().collect()
    }

    fn convert(&self, batch: DebounceEventResult) -> Vec<VaultEvent> {
        let Ok(batch) = batch else {
            return Vec::new();
        };

        let mut events = BTreeSet::new();
        for path in batch
            .into_iter()
            .flat_map(|event| event.event.paths.clone())
        {
            if let Some(event) = self.classify(&path) {
                events.insert(event);
            }
        }
        events.into_iter().collect()
    }

    fn classify(&self, path: &Path) -> Option<VaultEvent> {
        let relative = path.strip_prefix(&self.root).ok()?;

        let is_hidden = relative.components().any(|part| {
            part.as_os_str()
                .to_str()
                .is_some_and(|s| s.starts_with('.'))
        });
        if is_hidden {
            return None;
        }

        let extension = path.extension()?.to_str()?;
        if !is_markdown_extension(extension) {
            return None;
        }

        let id = NoteId::from_relative_path(relative)?;
        Some(if path.is_file() {
            VaultEvent::Changed(id)
        } else {
            VaultEvent::Removed(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn wait(watcher: &VaultWatcher) -> Vec<VaultEvent> {
        for _ in 0..40 {
            let events = watcher.try_recv();
            if !events.is_empty() {
                return events;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Vec::new()
    }

    #[test]
    fn reports_created_and_deleted_notes() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = VaultWatcher::start(dir.path()).unwrap();

        fs::write(dir.path().join("Новая.md"), "# Новая").unwrap();
        assert_eq!(
            wait(&watcher),
            [VaultEvent::Changed(NoteId::parse("Новая").unwrap())]
        );

        fs::remove_file(dir.path().join("Новая.md")).unwrap();
        assert_eq!(
            wait(&watcher),
            [VaultEvent::Removed(NoteId::parse("Новая").unwrap())]
        );
    }

    #[test]
    fn a_move_to_the_trash_is_only_a_removal() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Черновик.md"), "# Черновик").unwrap();
        let mut vault = kknow_core::Vault::open(dir.path()).unwrap();
        let watcher = VaultWatcher::start(dir.path()).unwrap();
        std::thread::sleep(Duration::from_millis(300));

        vault.remove(&NoteId::parse("Черновик").unwrap()).unwrap();

        assert_eq!(
            wait(&watcher),
            [VaultEvent::Removed(NoteId::parse("Черновик").unwrap())]
        );
    }

    #[test]
    fn ignores_other_files_and_hidden_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".trash")).unwrap();
        let watcher = VaultWatcher::start(dir.path()).unwrap();

        fs::write(dir.path().join("картинка.png"), "не заметка").unwrap();
        fs::write(dir.path().join(".trash/cache.md"), "служебное").unwrap();
        std::thread::sleep(Duration::from_millis(600));

        assert!(watcher.try_recv().is_empty());
    }
}
