//! User keys: `keybindings.json` next to the settings.
//!
//! The settings window writes the file, and an event sends the new keys to
//! both windows, like the settings. People also edit the file by hand, so
//! `get_keymap` reads it from disk each time: the window asks on focus and
//! sees a hand edit without a watcher.

use std::path::PathBuf;
use std::sync::Mutex;

use kknow_app::keymap::Keymap;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::state::{Error, Result};

/// Event name for the windows.
pub const EVENT: &str = "keymap-changed";

/// The file with the keys. `None` means no file: tests and a system without a
/// settings folder keep the keys in memory.
#[derive(Default)]
pub struct KeymapState {
    file: Mutex<Option<PathBuf>>,
    memory: Mutex<Keymap>,
}

/// The keys for the windows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeymapView {
    pub keys: Keymap,
    /// The file path, for the hint on the keys page.
    pub file: Option<String>,
    /// Skipped entries of a hand-edited file.
    pub problems: Vec<String>,
}

impl KeymapState {
    pub fn use_file(&self, file: Option<PathBuf>) {
        *self.file.lock().expect("клавиши испорчены") = file;
    }

    fn read(&self) -> KeymapView {
        let file = self.file.lock().expect("клавиши испорчены").clone();
        let Some(path) = file else {
            return KeymapView {
                keys: self.memory.lock().expect("клавиши испорчены").clone(),
                file: None,
                problems: Vec::new(),
            };
        };
        let (keys, problems) = match Keymap::load(&path) {
            Ok(loaded) => (loaded.keymap, loaded.problems),
            Err(error) => (Keymap::default(), vec![error.to_string()]),
        };
        KeymapView {
            keys,
            file: Some(path.display().to_string()),
            problems,
        }
    }

    /// Writes the keys. A write error goes to the window: unlike the
    /// language, a key that is not on disk is lost on the next focus.
    fn write(&self, keys: Keymap) -> Result<KeymapView> {
        let file = self.file.lock().expect("клавиши испорчены").clone();
        match file {
            Some(path) => keys.save(&path)?,
            None => *self.memory.lock().expect("клавиши испорчены") = keys,
        }
        Ok(self.read())
    }
}

#[tauri::command]
pub fn get_keymap(state: State<'_, KeymapState>) -> KeymapView {
    state.read()
}

/// Saves the changed keys and sends them to both windows.
#[tauri::command(async)]
pub fn set_keymap<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, KeymapState>,
    keys: Keymap,
) -> Result<KeymapView> {
    let saved = state.write(keys)?;
    let _ = app.emit(EVENT, saved.clone());
    Ok(saved)
}

/// Shows the file in Finder. A missing file is created empty first, so the
/// user has a file to open and edit.
#[tauri::command(async)]
pub fn reveal_keymap(state: State<'_, KeymapState>) -> Result<()> {
    let path = state
        .file
        .lock()
        .expect("клавиши испорчены")
        .clone()
        .ok_or(Error::NoConfig)?;

    if !path.exists() {
        Keymap::default().save(&path)?;
    }
    crate::commands::reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    fn app() -> tauri::App<tauri::test::MockRuntime> {
        mock_builder()
            .manage(KeymapState::default())
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    fn keys() -> Keymap {
        Keymap(BTreeMap::from([(
            "note.new".to_string(),
            vec!["Mod-Shift-n".to_string()],
        )]))
    }

    #[test]
    fn keys_survive_a_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = Keymap::path(dir.path());
        let app = app();
        app.state::<KeymapState>().use_file(Some(path.clone()));

        let saved = set_keymap(app.handle().clone(), app.state(), keys()).unwrap();

        assert_eq!(saved.keys, keys());
        assert_eq!(get_keymap(app.state()), saved);
        assert_eq!(Keymap::load(&path).unwrap().keymap, keys());
    }

    /// The window asks on focus, so a hand edit shows without a restart.
    #[test]
    fn a_hand_edit_shows_on_the_next_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = Keymap::path(dir.path());
        let app = app();
        app.state::<KeymapState>().use_file(Some(path.clone()));
        assert_eq!(get_keymap(app.state()).keys, Keymap::default());

        std::fs::write(
            &path,
            r#"{"note.new": ["Mod-Shift-n"], "note.save": "Mod-s"}"#,
        )
        .unwrap();
        let read = get_keymap(app.state());

        assert_eq!(read.keys, keys());
        assert_eq!(read.problems.len(), 1, "{:?}", read.problems);
    }

    #[test]
    fn keys_without_a_file_still_change() {
        let app = app();

        set_keymap(app.handle().clone(), app.state(), keys()).unwrap();

        assert_eq!(get_keymap(app.state()).keys, keys());
        assert_eq!(get_keymap(app.state()).file, None);
    }
}
