use std::path::PathBuf;
use std::sync::Mutex;

use kknow_app::settings::AppSettings;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::state::Result;

pub const EVENT: &str = "settings-changed";

#[derive(Default)]
pub struct SettingsState {
    file: Mutex<Option<PathBuf>>,
    settings: Mutex<AppSettings>,
}

impl SettingsState {
    pub fn use_file(&self, file: Option<PathBuf>, settings: AppSettings) {
        *self.file.lock().expect("settings are poisoned") = file;
        *self.settings.lock().expect("settings are poisoned") = settings;
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().expect("settings are poisoned").clone()
    }

    fn put(&self, next: AppSettings) -> AppSettings {
        *self.settings.lock().expect("settings are poisoned") = next.clone();

        let file = self.file.lock().expect("settings are poisoned");
        if let Some(path) = file.as_ref() {
            if let Err(error) = next.save(path) {
                eprintln!("cannot write the settings: {error}");
            }
        }
        next
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> AppSettings {
    state.get()
}

#[tauri::command(async)]
pub fn set_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SettingsState>,
    settings: AppSettings,
) -> Result<AppSettings> {
    let saved = state.put(settings);
    #[cfg(target_os = "macos")]
    crate::menu::relabel(&app, &saved.locale);
    if let Some(window) = app.get_webview_window(crate::commands::SETTINGS_WINDOW) {
        let _ = window.set_title(&crate::i18n::label(
            &saved.locale,
            crate::i18n::SETTINGS_TITLE,
        ));
    }
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
        assert_eq!(AppSettings::load(&path).unwrap().settings, next);
    }

    #[test]
    fn settings_without_a_file_still_change() {
        let app = mock_builder()
            .manage(SettingsState::default())
            .build(mock_context(noop_assets()))
            .unwrap();

        let next = AppSettings {
            line_numbers: false,
            ..AppSettings::default()
        };
        set_settings(app.handle().clone(), app.state(), next.clone()).unwrap();

        assert!(!get_settings(app.state()).line_numbers);
    }
}
