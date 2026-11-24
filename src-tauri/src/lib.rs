mod association;
#[cfg(test)]
mod bindings;
mod commands;
mod errors;
mod files;
mod git;
mod i18n;
mod keymap;
mod live;
mod lock;
mod mcp_http;
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

pub(crate) const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

fn paths_from_args() -> Vec<std::path::PathBuf> {
    std::env::args()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

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

fn adopt(state: &AppState, path: &std::path::Path) -> Option<String> {
    let id = match state.add(path.to_path_buf(), None) {
        Ok(entry) => entry.id,
        Err(refusal) => {
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
    let accepted = files
        .iter()
        .filter(|path| accept_file(&state, path))
        .count();

    if let Some(id) = opened.last() {
        let _ = app.emit("vault-opened", id);
    }
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
    let state = AppState::default();
    let settings = settings::SettingsState::default();
    let keymap = keymap::KeymapState::default();

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

    match AppSettings::load(&AppSettings::path(&config)) {
        Ok(loaded) => {
            if let Some(problem) = loaded.problem {
                eprintln!("{problem}");
            }
            settings.use_file(Some(AppSettings::path(&config)), loaded.settings);
        }
        Err(error) => eprintln!("не удалось прочитать настройки: {error}"),
    }
    keymap.use_file(vaults::config_dir().map(Keymap::path));

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

    state.use_commands(kknow_app::commands::config_path());
    state.use_themes(vaults::config_dir().map(kknow_app::theme::global_dir));

    for path in paths_from_args() {
        if path.is_dir() {
            adopt(&state, &path);
        } else {
            accept_file(&state, &path);
        }
    }

    #[cfg(target_os = "macos")]
    let locale = settings.get().locale;

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(move |handle| menu::build(handle, &locale))
        .on_menu_event(|app, event| {
            let updates = event.id() == menu::UPDATES_ITEM;
            if event.id() == menu::SETTINGS_ITEM || updates {
                if let Err(error) = commands::open_settings(app.clone(), app.state(), None) {
                    eprintln!("не удалось открыть настройки: {error}");
                }
            }
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
        .manage(mcp_http::McpServer::load(vaults::config_dir()))
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>();

            let version = app.package_info().version.to_string();
            let exe = std::env::current_exe().ok();
            app.manage(updater::Updates::new(updater::initial_status(
                &version,
                cfg!(debug_assertions),
                exe.as_deref(),
            )));
            updater::schedule(handle.clone());

            let watcher = handle.clone();
            state.on_open(move |slot| {
                live::watch(watcher.clone(), slot.clone());
                sync_status::schedule(watcher.clone(), slot);
            });

            if state.is_open() {
                if let Err(error) = state.slot(None) {
                    eprintln!("не удалось открыть активное хранилище: {error}");
                }
            }

            app.state::<mcp_http::McpServer>().start(handle.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lock::lock_state,
            lock::set_lock_password,
            lock::unlock_notes,
            lock::change_lock_password,
            lock::lock_all,
            lock::lock_note,
            lock::remove_lock,
            commands::list_vaults,
            commands::add_vault,
            commands::remove_vault,
            commands::activate_vault,
            commands::vault_by_hotkey,
            commands::rename_vault,
            commands::set_vault_color,
            commands::set_vault_labels,
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
            commands::link_target,
            commands::note_backlinks,
            commands::unlinked_mentions,
            commands::link_mention,
            commands::link_preview,
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
            commands::review_proposal,
            commands::dismiss_proposal,
            commands::reply_thread,
            commands::duplicate_note,
            commands::rename_folder,
            commands::search,
            commands::search_all,
            commands::list_themes,
            commands::share_theme,
            commands::reveal_themes,
            mcp_http::mcp_status,
            mcp_http::set_mcp,
            run::list_commands,
            run::run_command,
            run::stop_command,
            run::edit_commands,
            run::save_commands,
            run::reveal_commands,
            sync::sync_settings,
            sync::save_sync_settings,
            sync::forget_sync_settings,
            sync::sync_encrypt,
            sync::sync_unlock,
            sync::sync_change_passphrase,
            sync::sync_now,
            sync::sync_status,
            sync::sync_statuses,
            git::git_locate,
            git::git_version,
            git::connect_git,
            git::git_status,
            git::git_stage,
            git::git_unstage,
            git::git_commit,
            git::git_diff,
            git::git_discard,
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
            if matches!(event, tauri::WindowEvent::CloseRequested { .. })
                && sync_status::start_closing()
            {
                sync_status::sync_on_close(window.app_handle(), CLOSE_WAIT);
            }
        })
        .build(tauri::generate_context!())
        .expect("не удалось запустить приложение")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                open_paths(app, urls);
            }
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

    #[test]
    fn only_memory_getters_are_sync_commands() {
        const SYNC: &[&str] = &[
            "list_vaults",
            "vault_by_hotkey",
            "take_opened_file",
            "get_settings",
            "get_keymap",
            "update_status",
            "mcp_status",
            "stop_command",
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
