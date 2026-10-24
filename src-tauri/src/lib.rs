mod association;
#[cfg(test)]
mod bindings;
mod commands;
mod errors;
mod files;
mod keymap;
mod live;
#[cfg(target_os = "macos")]
mod menu;
mod run;
mod secrets;
mod settings;
mod state;
mod sync;
mod sync_status;
mod updater;

use kknow_app::files::OpenFiles;
use kknow_app::keymap::Keymap;
use kknow_app::settings::AppSettings;
use kknow_app::vaults::{self, VaultRegistry};
use state::AppState;
use tauri::Manager;

/// How long to wait for the last sync when the window closes.
pub(crate) const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Paths from command-line arguments: `kknow ~/notes`, `kknow README.md` and
/// `open -a kknow --args ~/notes ~/work`.
///
/// We silently skip anything that is not a folder or a file. Finder adds its
/// own arguments at start, for example `-psn_0_12345`, and they are not errors.
fn paths_from_args() -> Vec<std::path::PathBuf> {
    std::env::args()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

/// Accepts a file opened from outside and keeps it until the window asks.
///
/// A file from a known vault makes that vault active at once. On a cold start
/// the window opens the active vault anyway, so the note is where the user
/// expects it. A refusal (not markdown) goes to stderr, as for a folder.
fn accept_file(state: &AppState, path: &std::path::Path) -> bool {
    match files::open_path(state, path) {
        Ok(opened) => {
            if let files::Opened::Note { vault, .. } = &opened {
                if let Err(error) = state.set_active(vault) {
                    eprintln!("не удалось сделать хранилище активным ({vault}): {error}");
                }
            }
            state.set_pending(opened);
            true
        }
        Err(error) => {
            eprintln!("файл не принят ({}): {error}", path.display());
            false
        }
    }
}

/// Accepts a folder to open and makes it active.
///
/// One path for all sources: a command-line argument, "Open With" from the
/// file manager, and `open -a kknow ~/notes`. A known folder does not get a
/// second entry: the existing entry becomes active. Otherwise `kknow .` in a
/// known folder adds a duplicate each time.
/// We do not hide the refusal reason. Silence here looks the same as "the event
/// did not arrive", and these two problems have different fixes.
fn adopt(state: &AppState, path: &std::path::Path) -> Option<String> {
    let id = match state.add(path.to_path_buf(), None) {
        Ok(entry) => entry.id,
        Err(refusal) => {
            // There are three refusal reasons: not a folder, already in the
            // list, or inside another vault. Only a folder that is really in
            // the list becomes active.
            let known = path.canonicalize().ok().and_then(|canonical| {
                state
                    .entries()
                    .into_iter()
                    .find(|entry| entry.path == canonical)
                    .map(|entry| entry.id)
            });

            match known {
                Some(id) => id,
                None => {
                    eprintln!("каталог не принят ({}): {refusal}", path.display());
                    return None;
                }
            }
        }
    };

    if let Err(error) = state.set_active(&id) {
        eprintln!("не удалось сделать хранилище активным ({id}): {error}");
        return None;
    }
    Some(id)
}

/// Folders and files the system asks the app to open.
///
/// macOS sends them as an event, not as command-line arguments, both to a
/// running app and to a starting one. The window learns about the new vault
/// from the `vault-opened` event. If the window is not loaded yet, the event is
/// lost. That is fine: on load the window opens the active vault anyway.
#[cfg(target_os = "macos")]
fn open_paths<R: tauri::Runtime>(app: &tauri::AppHandle<R>, urls: &[tauri::Url]) {
    use tauri::Emitter;

    let state = app.state::<AppState>();
    let (folders, files): (Vec<_>, Vec<_>) = urls
        .iter()
        .filter_map(|url| url.to_file_path().ok())
        .partition(|path| path.is_dir());

    let opened: Vec<String> = folders
        .iter()
        .filter_map(|path| adopt(&state, path))
        .collect();
    // The user can select several files. All go to the list, but only the last
    // one opens: there is only one pending request.
    let accepted = files
        .iter()
        .filter(|path| accept_file(&state, path))
        .count();

    // The last accepted folder stays active: that is the one the user wants.
    if let Some(id) = opened.last() {
        let _ = app.emit("vault-opened", id);
    }
    // The window takes the file itself. On a cold start the event is lost,
    // but the pending request waits for the load.
    if accepted > 0 {
        let _ = app.emit("file-opened", ());
    }
    if opened.is_empty() && accepted == 0 {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }
}

pub fn run() {
    // Build the state before the app is built, not in `setup`.
    //
    // Tauri runs the `setup` hook inside the event loop (on `RunEvent::Ready`),
    // but "open folder in app" on a cold start arrives before it. The event
    // handler then saw an empty state: the folder went to a temporary list,
    // the disk write was silently skipped (no file was set yet), and then
    // `setup` overwrote everything with data from disk. No error and no vault.
    // The registry and settings do not need Tauri, so there is no reason to
    // wait for it.
    let state = AppState::default();
    let settings = settings::SettingsState::default();
    let keymap = keymap::KeymapState::default();

    // The vault list is shared with the CLI. If the app sees it for the first
    // time, the folder saved by the previous version moves into it.
    let config = vaults::config_dir().unwrap_or_default();
    match vaults::open(&config) {
        Ok(loaded) => {
            if let Some(problem) = loaded.problem {
                eprintln!("{problem}");
            }
            state.use_registry(Some(VaultRegistry::path(&config)), loaded.registry);
        }
        Err(error) => eprintln!("не удалось прочитать список хранилищ: {error}"),
    }

    // Settings live next to the registry. Language and theme are shared by
    // both windows, so they use one file.
    match AppSettings::load(&AppSettings::path(&config)) {
        Ok(loaded) => {
            if let Some(problem) = loaded.problem {
                eprintln!("{problem}");
            }
            settings.use_file(Some(AppSettings::path(&config)), loaded.settings);
        }
        Err(error) => eprintln!("не удалось прочитать настройки: {error}"),
    }
    // The keys too: the window reads the file itself on each focus.
    keymap.use_file(vaults::config_dir().map(Keymap::path));

    // Loose files live there too: they belong to the window, not to a folder.
    // Missing files are forgotten on read, and the list is rewritten at once.
    let files_path = OpenFiles::path(&config);
    match OpenFiles::load(&files_path) {
        Ok(loaded) => {
            if let Some(problem) = loaded.problem {
                eprintln!("{problem}");
            }
            if !loaded.forgotten.is_empty() {
                if let Err(error) = loaded.files.save(&files_path) {
                    eprintln!("не удалось записать список открытых файлов: {error}");
                }
            }
            state.use_files(Some(files_path), loaded.files);
        }
        Err(error) => eprintln!("не удалось прочитать список открытых файлов: {error}"),
    }

    // Commands live there too: they are a machine setting, not a vault setting.
    // The path is set here, not looked up inside the commands, so tests can
    // replace it with a temporary file and keep the real one untouched.
    state.use_commands(kknow_app::commands::config_path());
    // Global themes are also a machine setting: every vault sees them.
    state.use_themes(vaults::config_dir().map(kknow_app::theme::global_dir));

    // Folders from the command line go to the list and open first, so
    // `kknow ~/notes` works with the registry. A file goes to the loose files
    // and waits for the window.
    for path in paths_from_args() {
        if path.is_dir() {
            adopt(&state, &path);
        } else {
            accept_file(&state, &path);
        }
    }

    // The menu is built before the window loads, so it takes the language
    // from the settings read above.
    #[cfg(target_os = "macos")]
    let locale = settings.get().locale;

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(move |handle| menu::build(handle, &locale))
        .on_menu_event(|app, event| {
            let updates = event.id() == menu::UPDATES_ITEM;
            if event.id() == menu::SETTINGS_ITEM || updates {
                if let Err(error) = commands::open_settings(app.clone(), None) {
                    eprintln!("не удалось открыть настройки: {error}");
                }
            }
            // "General" shows the status, so the user sees how the check
            // goes, without a dialog.
            if updates {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    updater::check(&app).await;
                });
            }
        });

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .manage(sync_status::SyncStatuses::default())
        .manage(settings)
        .manage(keymap)
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>();

            let version = app.package_info().version.to_string();
            app.manage(updater::Updates::new(updater::is_release(
                &version,
                cfg!(debug_assertions),
            )));
            updater::schedule(handle.clone());

            // The state starts the watcher and the scheduler itself: vaults
            // open lazily and do not need to know about the window. An open
            // vault has two helpers, the file system watcher and the auto-sync
            // scheduler. Both live as long as the slot.
            let watcher = handle.clone();
            state.on_open(move |slot| {
                live::watch(watcher.clone(), slot.clone());
                sync_status::schedule(watcher.clone(), slot);
            });

            // Open the active vault at once: the app must start ready to work,
            // not on an empty folder picker. It happens here, not above: an
            // open vault needs the watcher and the scheduler, and they need
            // the app handle.
            if state.is_open() {
                if let Err(error) = state.slot(None) {
                    eprintln!("не удалось открыть активное хранилище: {error}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_vaults,
            commands::add_vault,
            commands::remove_vault,
            commands::activate_vault,
            commands::vault_by_hotkey,
            commands::rename_vault,
            commands::set_vault_color,
            commands::vault_stats,
            commands::open_settings,
            commands::current_vault,
            commands::reload_vault,
            commands::list_notes,
            commands::load_note,
            commands::save_note,
            commands::preview,
            commands::note_image,
            commands::save_attachment,
            commands::attach_file,
            commands::create_note,
            commands::delete_note,
            commands::list_folders,
            commands::create_folder,
            commands::delete_folder,
            commands::list_trash,
            commands::restore_trash,
            commands::trash_note,
            commands::delete_trash_entry,
            commands::empty_trash,
            commands::reveal_note,
            commands::reveal_folder,
            commands::open_note,
            commands::rename_note,
            commands::set_bookmarks,
            commands::list_bookmarks,
            commands::list_tasks,
            commands::set_task,
            commands::duplicate_note,
            commands::rename_folder,
            commands::search,
            commands::search_all,
            commands::list_themes,
            commands::share_theme,
            commands::reveal_themes,
            run::list_commands,
            run::run_command,
            run::edit_commands,
            run::save_commands,
            run::reveal_commands,
            sync::sync_settings,
            sync::save_sync_settings,
            sync::forget_sync_settings,
            sync::sync_now,
            sync::sync_status,
            sync::sync_statuses,
            settings::get_settings,
            settings::set_settings,
            keymap::get_keymap,
            keymap::set_keymap,
            keymap::reveal_keymap,
            updater::update_status,
            updater::check_update,
            updater::restart_to_update,
            files::list_files,
            files::open_file,
            files::take_opened_file,
            files::load_file,
            files::save_file,
            files::preview_file,
            files::file_image,
            files::forget_file,
            files::reveal_file,
            association::markdown_association,
            association::claim_markdown,
        ])
        .on_window_event(|window, event| {
            // Before close, run a last sync for vaults with auto-sync. The
            // wait is short: sync is idempotent, the next start finishes it,
            // and the user must not see a frozen window.
            if matches!(event, tauri::WindowEvent::CloseRequested { .. })
                && sync_status::start_closing()
            {
                sync_status::sync_on_close(window.app_handle(), CLOSE_WAIT);
            }
        })
        // Use `build` and then the loop, not `run`: only this way we see
        // `RunEvent::Opened`, which macOS uses to pass a folder opened outside.
        .build(tauri::generate_context!())
        .expect("не удалось запустить приложение")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                open_paths(app, urls);
            }
            // The user quit without the "Restart" button: the next start is
            // the new version anyway.
            if let tauri::RunEvent::Exit = event {
                updater::install_on_exit(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn an_opened_folder_becomes_the_active_vault() {
        let home = tempfile::tempdir().unwrap();
        let notes = home.path().join("Заметки");
        fs::create_dir_all(&notes).unwrap();
        let state = state::testing(&[]);

        let id = adopt(&state, &notes).expect("каталог принят");

        assert_eq!(state.active().unwrap().id, id);
        assert_eq!(state.entries().len(), 1);
    }

    /// A second open of the same folder adds no duplicate: `kknow .` in a known
    /// folder is common.
    #[test]
    fn opening_a_known_folder_only_switches_to_it() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join("Первое");
        let second = home.path().join("Второе");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let state = state::testing(&[]);
        let known = adopt(&state, &first).unwrap();
        adopt(&state, &second).unwrap();

        assert_eq!(
            adopt(&state, &first).unwrap(),
            known,
            "тот же идентификатор"
        );
        assert_eq!(state.entries().len(), 2, "дубля не появилось");
        assert_eq!(state.active().unwrap().id, known);
    }

    /// A file from outside does not become a vault. It goes to the loose files
    /// and waits for the window as a pending request.
    #[test]
    fn an_opened_file_waits_for_the_window() {
        let home = tempfile::tempdir().unwrap();
        let readme = home.path().join("README.md");
        fs::write(&readme, "# Проект\n").unwrap();
        let state = state::testing(&[]);

        assert!(accept_file(&state, &readme));

        assert!(state.entries().is_empty(), "хранилище не заведено");
        assert_eq!(state.files().len(), 1);
        assert!(matches!(
            state.take_pending(),
            Some(files::Opened::File { .. })
        ));
    }

    /// A file from another vault makes that vault active, so the window opens
    /// it at start.
    #[test]
    fn a_file_from_another_vault_switches_to_it() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let note = second.path().join("план.md");
        fs::write(&note, "# план\n").unwrap();
        let state = state::testing(&[first.path(), second.path()]);
        let wanted = state.entries()[1].id.clone();

        assert!(accept_file(&state, &note));

        assert_eq!(state.active().unwrap().id, wanted);
        assert!(state.files().is_empty());
    }

    #[test]
    fn a_picture_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let picture = home.path().join("схема.png");
        fs::write(&picture, "png").unwrap();
        let state = state::testing(&[]);

        assert!(!accept_file(&state, &picture));
        assert!(state.take_pending().is_none());
    }

    /// A file cannot replace the active vault: `open -a kknow file.md` must
    /// leave everything as it was.
    #[test]
    fn a_file_is_not_a_vault() {
        let home = tempfile::tempdir().unwrap();
        let notes = home.path().join("Заметки");
        fs::create_dir_all(&notes).unwrap();
        let file = home.path().join("Заметки/одна.md");
        fs::write(&file, "# одна\n").unwrap();

        let state = state::testing(&[]);
        let known = adopt(&state, &notes).unwrap();

        assert!(adopt(&state, &file).is_none());
        assert_eq!(state.entries().len(), 1);
        assert_eq!(state.active().unwrap().id, known);
    }

    /// A sync command runs on the main thread, and the window does not respond
    /// until it returns. Only a getter of memory may stay sync; any command
    /// that touches the disk, the network or may open a vault must be async.
    #[test]
    fn only_memory_getters_are_sync_commands() {
        const SYNC: &[&str] = &[
            "list_vaults",
            "vault_by_hotkey",
            "take_opened_file",
            "get_settings",
            "get_keymap",
            "update_status",
        ];
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut blocking = Vec::new();
        for file in fs::read_dir(dir).unwrap() {
            let source = fs::read_to_string(file.unwrap().path()).unwrap();
            let mut lines = source.lines();
            while let Some(line) = lines.next() {
                if line.trim() != "#[tauri::command]" {
                    continue;
                }
                let signature = lines.next().unwrap_or_default();
                if signature.contains("async fn") {
                    continue;
                }
                let name = signature
                    .split("fn ")
                    .nth(1)
                    .and_then(|rest| rest.split(['(', '<']).next())
                    .unwrap_or(signature);
                if !SYNC.contains(&name) {
                    blocking.push(name.to_string());
                }
            }
        }
        assert!(
            blocking.is_empty(),
            "these commands block the window, add (async): {blocking:?}"
        );
    }
}
