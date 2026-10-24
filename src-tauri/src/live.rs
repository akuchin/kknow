//! Background watching of the notes folder.
//!
//! Edits from an external editor must reach the window on their own. So a
//! separate thread listens to the file system and sends an event to the
//! frontend.

use std::sync::Arc;
use std::time::Duration;

use kknow_app::Changes;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::VaultSlot;

/// Event name for the frontend.
pub const EVENT: &str = "vault-changed";

/// How often the watcher wakes up to check if the vault is closed.
const WAKE: Duration = Duration::from_millis(500);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct VaultChanged {
    /// The vault ID: the window must not redraw for edits in an inactive vault.
    pub vault: String,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

impl VaultChanged {
    pub fn new(vault: &str, changes: Changes) -> Self {
        let ids = |list: Vec<kknow_core::NoteId>| {
            list.into_iter().map(|id| id.as_str().to_string()).collect()
        };
        VaultChanged {
            vault: vault.to_string(),
            updated: ids(changes.updated),
            removed: ids(changes.removed),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.removed.is_empty()
    }
}

/// Starts watching one vault.
///
/// The thread lives while the vault is open. A closed vault (removed from the
/// list) sets its flag, and the thread exits at the next wake-up.
pub fn watch<R: Runtime>(app: AppHandle<R>, slot: Arc<VaultSlot>) {
    std::thread::spawn(move || {
        let watcher = match slot.read(|workspace| Ok(workspace.watch()?)) {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!("не удалось следить за каталогом: {error}");
                return;
            }
        };

        while !slot.is_closed() {
            let Some(events) = watcher.recv_timeout(WAKE) else {
                continue;
            };
            if slot.is_closed() {
                break;
            }

            match slot.write(|workspace| Ok(workspace.apply(&events)?)) {
                Ok(changes) if !changes.is_empty() => {
                    let _ = app.emit(EVENT, VaultChanged::new(&slot.entry.id, changes));
                }
                Ok(_) => {}
                Err(error) => eprintln!("не удалось применить изменения: {error}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    use kknow_core::NoteId;
    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use crate::state::AppState;

    #[test]
    fn background_thread_applies_external_edits() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();

        let app = mock_builder()
            .manage(crate::state::testing(&[dir.path()]))
            .build(mock_context(noop_assets()))
            .unwrap();

        let slot = app.state::<AppState>().slot(None).unwrap();
        watch(app.handle().clone(), slot.clone());
        // FSEvents does not start listening at once. It misses a write made at
        // the same moment, so the test would catch a race, not the watcher.
        std::thread::sleep(Duration::from_millis(800));

        fs::write(dir.path().join("Извне.md"), "# Извне\n\nсоздано снаружи\n").unwrap();

        let id = NoteId::parse("Извне").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let seen = slot
                .read(|workspace| Ok(workspace.contains(&id)))
                .unwrap_or(false);
            if seen {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        panic!("наблюдатель не подхватил новый файл за 5 секунд");
    }

    /// There is one watcher per vault. An edit in one vault must not arrive as
    /// an edit in another, so the event carries the vault ID.
    #[test]
    fn events_say_which_vault_changed() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(first.path().join("Index.md"), "# Первое\n").unwrap();
        fs::write(second.path().join("Index.md"), "# Второе\n").unwrap();

        let app = mock_builder()
            .manage(crate::state::testing(&[first.path(), second.path()]))
            .build(mock_context(noop_assets()))
            .unwrap();

        let state = app.state::<AppState>();
        let ids: Vec<String> = state.entries().iter().map(|e| e.id.clone()).collect();
        let other = state.slot(Some(&ids[1])).unwrap();

        let changed = VaultChanged::new(
            &other.entry.id,
            kknow_app::Changes {
                updated: vec![NoteId::parse("Index").unwrap()],
                removed: Vec::new(),
            },
        );

        assert_eq!(changed.vault, ids[1]);
        assert!(!changed.is_empty());
    }

    #[test]
    fn a_forgotten_vault_stops_its_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_builder()
            .manage(crate::state::testing(&[dir.path()]))
            .build(mock_context(noop_assets()))
            .unwrap();

        let state = app.state::<AppState>();
        let id = state.entries()[0].id.clone();
        let slot = state.slot(Some(&id)).unwrap();
        watch(app.handle().clone(), slot.clone());

        state.remove(&id).unwrap();

        assert!(
            slot.is_closed(),
            "поток обязан выйти на ближайшем пробуждении"
        );
    }
}
