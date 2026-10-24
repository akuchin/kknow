//! Watches the notes directory.
//!
//! The app is not the only thing that edits files: an external editor, git and
//! future sync also do. The watcher is the only entry point for such changes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use kknow_core::{NoteId, is_markdown_extension};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, NoCache, new_debouncer_opt};

/// Time to merge events: one Cmd+S in an editor gives several file system
/// events.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// What happened to a note on disk.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VaultEvent {
    /// The file is created or changed.
    Changed(NoteId),
    /// The file no longer exists.
    Removed(NoteId),
}

pub struct VaultWatcher {
    // The watcher stops when the debouncer is dropped.
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
    events: Receiver<DebounceEventResult>,
    root: PathBuf,
}

impl VaultWatcher {
    pub fn start(root: &Path) -> notify::Result<Self> {
        // Event paths come resolved: on macOS, /var/folders is a symlink to
        // /private/var/folders. Without this step, strip_prefix below matches
        // no path.
        let root = root.canonicalize()?;

        let (sender, events) = channel();
        // No file id cache: with it, the debouncer joins the two halves of a
        // rename. If the old path looks freshly created (FSEvents on macOS
        // keeps that flag for a while), it then reports only a creation at the
        // new path. A move into the hidden trash loses its removal that way.
        // `classify` checks each path on its own, so we need no joined renames.
        // Linux already uses no cache.
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

    /// Waits for the next batch of changes. `None` means the watcher stopped.
    pub fn recv(&self) -> Option<Vec<VaultEvent>> {
        loop {
            let batch = self.events.recv().ok()?;
            let events = self.convert(batch);
            if !events.is_empty() {
                return Some(events);
            }
        }
    }

    /// Waits for changes for at most `timeout`.
    ///
    /// This lets a background thread wake up and check if it must stop, without
    /// waiting for the next file edit.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<VaultEvent>> {
        let batch = self.events.recv_timeout(timeout).ok()?;

        let mut events = self.convert(batch);
        // Take the batches that already arrived: one edit often causes several
        // notifications in a row.
        events.extend(self.try_recv());
        events.sort();
        events.dedup();

        (!events.is_empty()).then_some(events)
    }

    /// Takes changes that already arrived, without blocking the thread.
    pub fn try_recv(&self) -> Vec<VaultEvent> {
        let mut events = BTreeSet::new();
        while let Ok(batch) = self.events.try_recv() {
            events.extend(self.convert(batch));
        }
        events.into_iter().collect()
    }

    fn convert(&self, batch: DebounceEventResult) -> Vec<VaultEvent> {
        let Ok(batch) = batch else {
            // Lost events (for example, a queue overflow) are not a reason to
            // fail: the next edit still arrives, and a reload can resync.
            return Vec::new();
        };

        // BTreeSet also removes duplicates: one file can change twice.
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

    /// Converts a path into a note event and drops files that are not notes.
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
        // A rename comes as a pair of paths: the old one becomes Removed and the
        // new one Changed. So check the file, not the event kind.
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

    /// Waits for events until a timeout: file notifications are not instant.
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
