use std::path::{Path, PathBuf};

use kknow_app::files::{self as loose, FileSaved};
use serde::Serialize;
use tauri::State;
use tauri::ipc::Response;

use crate::commands::LinkRef;
use crate::state::{AppState, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Opened {
    Note { vault: String, id: String },
    File { path: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FileEntry {
    path: String,
    name: String,
    folder: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FileView {
    id: String,
    title: String,
    content: String,
    html: String,
    backlinks: Vec<LinkRef>,
    outgoing: Vec<LinkRef>,
    tags: Vec<String>,
    file: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum FileSaveOutcome {
    Saved { note: FileView },
    Conflict { note: FileView, copy: String },
}

fn entry(path: &Path) -> FileEntry {
    FileEntry {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder: path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn file_view(path: &Path, content: String) -> FileView {
    let tags = kknow_index::parse(&content)
        .tags
        .iter()
        .map(|tag| tag.name.clone())
        .collect();
    FileView {
        id: path.to_string_lossy().into_owned(),
        title: loose::title(path, &content),
        html: loose::render(path, &content),
        content,
        backlinks: Vec::new(),
        outgoing: Vec::new(),
        tags,
        file: true,
    }
}

pub fn open_path(state: &AppState, path: &Path) -> Result<Opened> {
    let canonical = loose::markdown_file(path)?;

    if let Some((vault, id)) = state.locate(&canonical) {
        return Ok(Opened::Note { vault, id });
    }
    let path = state.remember_file(&canonical)?;
    Ok(Opened::File {
        path: path.to_string_lossy().into_owned(),
    })
}

#[tauri::command(async)]
pub fn list_files(state: State<'_, AppState>) -> Vec<FileEntry> {
    state.files().iter().map(|path| entry(path)).collect()
}

#[tauri::command(async)]
pub fn open_file(state: State<'_, AppState>, path: String) -> Result<Opened> {
    open_path(&state, Path::new(&path))
}

#[tauri::command]
pub fn take_opened_file(state: State<'_, AppState>) -> Option<Opened> {
    state.take_pending()
}

#[tauri::command(async)]
pub fn load_file(state: State<'_, AppState>, path: String) -> Result<FileView> {
    let path = state.known_file(&path)?;
    let content = loose::read(&path)?;
    Ok(file_view(&path, content))
}

#[tauri::command(async)]
pub fn save_file(
    state: State<'_, AppState>,
    path: String,
    content: String,
    base: Option<String>,
) -> Result<FileSaveOutcome> {
    let path = match state.known_file(&path) {
        Ok(path) => path,
        Err(Error::FileGone(raw)) => {
            let path = PathBuf::from(raw);
            loose::write(&path, &content)?;
            let path = state.remember_file(&path)?;
            return Ok(FileSaveOutcome::Saved {
                note: file_view(&path, content),
            });
        }
        Err(other) => return Err(other),
    };

    let Some(base) = base else {
        loose::write(&path, &content)?;
        return Ok(FileSaveOutcome::Saved {
            note: file_view(&path, content),
        });
    };

    Ok(match loose::save_if_unchanged(&path, &content, &base)? {
        FileSaved::Written => FileSaveOutcome::Saved {
            note: file_view(&path, content),
        },
        FileSaved::Conflict { copy, disk } => {
            let copy = state.remember_file(&copy)?;
            FileSaveOutcome::Conflict {
                note: file_view(&path, disk),
                copy: copy.to_string_lossy().into_owned(),
            }
        }
    })
}

#[tauri::command(async)]
pub fn preview_file(state: State<'_, AppState>, path: String, content: String) -> Result<String> {
    let path = state.known_file(&path)?;
    Ok(loose::render(&path, &content))
}

#[tauri::command(async)]
pub fn file_image(state: State<'_, AppState>, path: String, target: String) -> Result<Response> {
    let path = state.known_file(&path)?;
    let image = loose::image(&path, &target).ok_or(Error::ImageNotFound(target))?;
    let bytes = kknow_core::image::read(&image).map_err(kknow_app::Error::from)?;
    Ok(Response::new(bytes))
}

#[tauri::command(async)]
pub fn forget_file(state: State<'_, AppState>, path: String) -> Vec<FileEntry> {
    state.forget_file(Path::new(&path));
    state.files().iter().map(|path| entry(path)).collect()
}

#[tauri::command(async)]
pub fn reveal_file(state: State<'_, AppState>, path: String) -> Result<()> {
    let path = state.known_file(&path)?;
    crate::commands::reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state;
    use std::fs;
    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    fn app(state: AppState) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    #[test]
    fn a_loose_file_is_remembered_and_saved() {
        let home = tempfile::tempdir().unwrap();
        let readme = home.path().join("README.md");
        fs::write(&readme, "# Проект\n").unwrap();
        let app = app(state::testing(&[]));
        let state = app.state::<AppState>();

        let Opened::File { path } = open_path(&state, &readme).unwrap() else {
            panic!("ожидался отдельный файл");
        };
        assert_eq!(list_files(app.state()).len(), 1);

        let view = load_file(app.state(), path.clone()).unwrap();
        assert_eq!(view.title, "Проект");
        assert!(view.file);

        let outcome = save_file(
            app.state(),
            path.clone(),
            "# Проект\n\nдописано\n".into(),
            Some(view.content),
        )
        .unwrap();
        assert!(matches!(outcome, FileSaveOutcome::Saved { .. }));
        assert_eq!(
            fs::read_to_string(&readme).unwrap(),
            "# Проект\n\nдописано\n"
        );
    }

    #[test]
    fn a_file_from_a_known_vault_opens_as_its_note() {
        let notes = tempfile::tempdir().unwrap();
        fs::create_dir_all(notes.path().join("Проекты")).unwrap();
        let note = notes.path().join("Проекты/kknow.md");
        fs::write(&note, "# kknow\n").unwrap();
        let state = state::testing(&[notes.path()]);
        let vault = state.entries()[0].id.clone();

        let opened = open_path(&state, &note).unwrap();

        assert_eq!(
            opened,
            Opened::Note {
                vault,
                id: "Проекты/kknow".into()
            }
        );
        assert!(state.files().is_empty(), "в отдельные файлы не попал");
    }

    #[test]
    fn only_opened_files_can_be_read_or_written() {
        let home = tempfile::tempdir().unwrap();
        let stranger = home.path().join("чужой.md");
        fs::write(&stranger, "не трогать\n").unwrap();
        let app = app(state::testing(&[]));
        let path = stranger.to_string_lossy().into_owned();

        assert!(matches!(
            load_file(app.state(), path.clone()),
            Err(Error::UnknownFile(_))
        ));
        assert!(save_file(app.state(), path, "затёрто".into(), None).is_err());
        assert_eq!(fs::read_to_string(&stranger).unwrap(), "не трогать\n");
    }

    #[test]
    fn a_vanished_file_is_forgotten_when_opened() {
        let home = tempfile::tempdir().unwrap();
        let readme = home.path().join("README.md");
        fs::write(&readme, "# Проект\n").unwrap();
        let app = app(state::testing(&[]));
        let Opened::File { path } = open_path(&app.state(), &readme).unwrap() else {
            panic!("ожидался отдельный файл");
        };

        fs::remove_file(&readme).unwrap();

        assert!(matches!(
            load_file(app.state(), path),
            Err(Error::FileGone(_))
        ));
        assert!(list_files(app.state()).is_empty());
    }

    #[test]
    fn unsaved_edits_bring_a_vanished_file_back() {
        let home = tempfile::tempdir().unwrap();
        let readme = home.path().join("README.md");
        fs::write(&readme, "было\n").unwrap();
        let app = app(state::testing(&[]));
        let Opened::File { path } = open_path(&app.state(), &readme).unwrap() else {
            panic!("ожидался отдельный файл");
        };

        fs::remove_file(&readme).unwrap();
        save_file(app.state(), path, "правка\n".into(), Some("было\n".into())).unwrap();

        assert_eq!(fs::read_to_string(&readme).unwrap(), "правка\n");
        assert_eq!(list_files(app.state()).len(), 1, "снова в списке");
    }

    #[test]
    fn an_outside_edit_leaves_a_copy_in_the_list() {
        let home = tempfile::tempdir().unwrap();
        let readme = home.path().join("README.md");
        fs::write(&readme, "было\n").unwrap();
        let app = app(state::testing(&[]));
        let Opened::File { path } = open_path(&app.state(), &readme).unwrap() else {
            panic!("ожидался отдельный файл");
        };

        fs::write(&readme, "чужая правка\n").unwrap();
        let outcome = save_file(
            app.state(),
            path,
            "моя правка\n".into(),
            Some("было\n".into()),
        )
        .unwrap();

        let FileSaveOutcome::Conflict { note, copy } = outcome else {
            panic!("ожидался конфликт");
        };
        assert_eq!(note.content, "чужая правка\n");
        assert_eq!(fs::read_to_string(&copy).unwrap(), "моя правка\n");
        assert_eq!(list_files(app.state()).len(), 2, "копия видна в списке");
    }

    #[test]
    fn a_pending_request_is_taken_once() {
        let app = app(state::testing(&[]));
        app.state::<AppState>().set_pending(Opened::File {
            path: "/tmp/README.md".into(),
        });

        assert!(take_opened_file(app.state()).is_some());
        assert!(take_opened_file(app.state()).is_none());
    }
}
