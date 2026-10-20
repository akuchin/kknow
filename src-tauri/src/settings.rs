//! App settings: language, theme and editor behaviour.
//!
//! They used to live in the main window's `localStorage`. Now there are two
//! windows, and they must agree. The second webview has its own storage and
//! does not report changes. So the source of truth is `settings.json` next to
//! the vault registry. A command changes it, and an event sends it to both
//! windows in the same way.
//!
//! The window keeps a mirror in `localStorage`. It needs the language
//! synchronously at the first moment of loading. Otherwise labels render in
//! one language and then re-render in another.

use std::path::PathBuf;
use std::sync::Mutex;

use kknow_app::settings::AppSettings;
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::state::Result;

/// Event name for the windows.
pub const EVENT: &str = "settings-changed";

/// App settings and the file to write them to.
#[derive(Default)]
pub struct SettingsState {
    /// `None` means write nowhere. Tests use this.
    file: Mutex<Option<PathBuf>>,
    settings: Mutex<AppSettings>,
}

impl SettingsState {
    pub fn use_file(&self, file: Option<PathBuf>, settings: AppSettings) {
        *self.file.lock().expect("настройки испорчены") = file;
        *self.settings.lock().expect("настройки испорчены") = settings;
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().expect("настройки испорчены").clone()
    }

    /// Writes the settings. A write error does not undo the change in memory:
    /// the window must use the new language even if the file did not open.
    fn put(&self, next: AppSettings) -> AppSettings {
        *self.settings.lock().expect("настройки испорчены") = next.clone();

        let file = self.file.lock().expect("настройки испорчены");
        if let Some(path) = file.as_ref() {
            if let Err(error) = next.save(path) {
                eprintln!("не удалось записать настройки: {error}");
            }
        }
        next
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> AppSettings {
    state.get()
}

/// Saves the settings and sends them to both windows.
#[tauri::command]
pub fn set_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SettingsState>,
    settings: AppSettings,
) -> Result<AppSettings> {
    let saved = state.put(settings);
    let _ = app.emit(EVENT, saved.clone());
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tauri::Manager;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    #[test]
    fn settings_survive_a_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = AppSettings::path(dir.path());

        let app = mock_builder()
            .manage(SettingsState::default())
            .build(mock_context(noop_assets()))
            .unwrap();
        app.state::<SettingsState>()
            .use_file(Some(path.clone()), AppSettings::default());

        let next = AppSettings {
            locale: "en".into(),
            theme: "dark".into(),
            ..AppSettings::default()
        };
        let saved = set_settings(app.handle().clone(), app.state(), next.clone()).unwrap();

        assert_eq!(saved, next);
        assert_eq!(get_settings(app.state()), next);
        // The disk has the same values: the second window reads them at start.
        assert_eq!(AppSettings::load(&path).unwrap().settings, next);
    }

    /// Settings change even without a file. Tests and the first start, before
    /// the settings folder is found, must not fail.
    #[test]
    fn settings_without_a_file_still_change() {
        let app = mock_builder()
            .manage(SettingsState::default())
            .build(mock_context(noop_assets()))
            .unwrap();

        let next = AppSettings {
            autosave_ms: 1000,
            ..AppSettings::default()
        };
        set_settings(app.handle().clone(), app.state(), next.clone()).unwrap();

        assert_eq!(get_settings(app.state()).autosave_ms, 1000);
    }
}
