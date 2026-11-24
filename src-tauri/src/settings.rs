use std::path::PathBuf;
use std::sync::Mutex;

use kknow_app::settings::AppSettings;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::state::Result;

pub const EVENT: &str = "settings-changed";

#[derive(Default)]
struct Stored {
    file: Option<PathBuf>,
    settings: AppSettings,
}

#[derive(Default)]
pub struct SettingsState {
    stored: Mutex<Stored>,
}

impl SettingsState {
    pub fn use_file(&self, file: Option<PathBuf>, settings: AppSettings) {
        *self.lock() = Stored { file, settings };
    }

    pub fn get(&self) -> AppSettings {
        self.lock().settings.clone()
    }

    fn put(&self, next: AppSettings) -> AppSettings {
        let mut stored = self.lock();
        stored.settings = next.clone();
        if let Some(path) = stored.file.as_ref() {
            if let Err(error) = next.save(path) {
                eprintln!("cannot write the settings: {error}");
            }
        }
        next
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Stored> {
        self.stored.lock().expect("settings are poisoned")
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

    #[test]
    fn parallel_saves_leave_the_last_value_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = AppSettings::path(dir.path());
        let app = mock_builder()
            .manage(SettingsState::default())
            .build(mock_context(noop_assets()))
            .unwrap();
        app.state::<SettingsState>()
            .use_file(Some(path.clone()), AppSettings::default());

        let workers: Vec<_> = (0..8)
            .map(|worker| {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    for round in 0..25 {
                        let next = AppSettings {
                            theme: format!("theme-{worker}-{round}"),
                            ..AppSettings::default()
                        };
                        set_settings(handle.clone(), handle.state(), next).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }

        assert_eq!(
            AppSettings::load(&path).unwrap().settings,
            get_settings(app.state())
        );
    }
}
