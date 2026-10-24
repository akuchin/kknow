//! Commands that the frontend calls. All logic lives in `kknow-app`; this
//! module only translates between it and JSON.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use kknow_app::bookmarks::Mark;
use kknow_app::theme::{self, ThemeSet};
use kknow_app::vaults::VaultEntry;
use kknow_app::{Saved, Workspace};
use kknow_core::NoteId;
use kknow_index::Edge;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};

use crate::state::{AppState, Error, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    /// Vault ID: later commands and events refer to the vault by it.
    id: String,
    name: String,
    root: String,
    color: String,
    note_count: usize,
}

/// A vault in the switcher list.
///
/// There is no note count here: it needs the vault to be opened and parsed,
/// and the list must show at once on the first click.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultView {
    id: String,
    name: String,
    path: String,
    color: String,
    active: bool,
    /// The folder exists. An external disk can be disconnected.
    available: bool,
}

/// Vault totals for the settings window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatsView {
    notes: usize,
    folders: usize,
    bytes: u64,
}

/// A global search result: a normal result plus the vault it came from.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalResult {
    vault: String,
    vault_name: String,
    color: String,
    id: String,
    title: String,
    folder: Option<String>,
    snippet: String,
}

#[derive(Serialize)]
pub struct NoteSummary {
    id: String,
    title: String,
    folder: Option<String>,
    tags: Vec<String>,
}

#[derive(Serialize)]
pub struct LinkRef {
    id: String,
    title: String,
    label: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    id: String,
    title: String,
    folder: Option<String>,
    /// Snippet with highlighted matches; its HTML is already escaped.
    snippet: String,
}

#[derive(Serialize)]
pub struct NoteView {
    id: String,
    title: String,
    content: String,
    html: String,
    backlinks: Vec<LinkRef>,
    outgoing: Vec<LinkRef>,
    tags: Vec<String>,
    /// Bookmarks in this text: 1-based lines with optional names.
    bookmarks: Vec<Mark>,
}

/// A bookmark in the palette list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkView {
    note: String,
    title: String,
    folder: Option<String>,
    line: usize,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// A task in the palette list.
#[derive(Serialize)]
pub struct TaskView {
    note: String,
    title: String,
    folder: Option<String>,
    line: usize,
    text: String,
    done: bool,
}

/// A moved note: its new ID and the number of notes whose links changed.
#[derive(Serialize)]
pub struct RenamedNote {
    id: String,
    relinked: usize,
}

fn info(entry: &VaultEntry, workspace: &Workspace) -> VaultInfo {
    VaultInfo {
        id: entry.id.clone(),
        name: entry.name.clone(),
        color: entry.color.as_str().to_string(),
        root: workspace.root().to_string_lossy().into_owned(),
        note_count: workspace.len(),
    }
}

fn view(entry: &VaultEntry, active: Option<&str>) -> VaultView {
    VaultView {
        active: active == Some(entry.id.as_str()),
        available: entry.available(),
        id: entry.id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().into_owned(),
        color: entry.color.as_str().to_string(),
    }
}

fn title_of(workspace: &Workspace, id: &NoteId) -> String {
    workspace
        .parsed(id)
        .and_then(|parsed| parsed.title())
        .unwrap_or_else(|| id.name())
        .to_string()
}

fn link_ref(workspace: &Workspace, id: &NoteId, label: Option<&String>) -> LinkRef {
    LinkRef {
        id: id.as_str().to_string(),
        title: title_of(workspace, id),
        label: label.cloned(),
    }
}

fn note_view(workspace: &Workspace, id: &NoteId, content: String) -> NoteView {
    let parsed = workspace.parsed(id);
    let bookmarks = workspace.note_bookmarks(id, &content);

    NoteView {
        id: id.as_str().to_string(),
        title: title_of(workspace, id),
        html: workspace.render(id, &content),
        content,
        backlinks: dedup(workspace, workspace.backlinks(id), |edge| &edge.from),
        outgoing: dedup(workspace, workspace.outgoing(id), |edge| &edge.to),
        tags: parsed
            .map(|parsed| parsed.tags.iter().map(|tag| tag.name.clone()).collect())
            .unwrap_or_default(),
        bookmarks,
    }
}

/// Several links can connect the same pair of notes. The list must show the
/// pair once.
fn dedup(workspace: &Workspace, edges: &[Edge], pick: impl Fn(&Edge) -> &NoteId) -> Vec<LinkRef> {
    let mut seen: Vec<&NoteId> = Vec::new();
    let mut refs = Vec::new();

    for edge in edges {
        let id = pick(edge);
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        refs.push(link_ref(workspace, id, edge.label.as_ref()));
    }

    refs
}

fn note_id(workspace: &Workspace, raw: &str) -> Result<NoteId> {
    workspace
        .find(raw)
        .cloned()
        .ok_or_else(|| Error::NotFound(raw.to_string()))
}

/// The vault list for the switcher.
#[tauri::command]
pub fn list_vaults(state: State<'_, AppState>) -> Vec<VaultView> {
    let active = state.active().map(|entry| entry.id);
    state
        .entries()
        .iter()
        .map(|entry| view(entry, active.as_deref()))
        .collect()
}

/// Adds a folder to the vault list and makes it active.
#[tauri::command]
pub fn add_vault(state: State<'_, AppState>, path: String) -> Result<VaultInfo> {
    let entry = state.add(path.into(), None)?;
    activate_vault(state, entry.id)
}

/// Forgets a vault. Notes stay on disk.
#[tauri::command]
pub fn remove_vault(state: State<'_, AppState>, id: String) -> Result<Vec<VaultView>> {
    state.remove(&id)?;
    Ok(list_vaults(state))
}

/// Switches the active vault: the tree, search and status bar use it.
#[tauri::command]
pub fn activate_vault(state: State<'_, AppState>, id: String) -> Result<VaultInfo> {
    let entry = state.set_active(&id)?;
    let slot = state.slot(Some(&entry.id))?;
    slot.read(|workspace| Ok(info(&entry, workspace)))
}

/// Opens the settings window, or brings it to the front if it is open.
///
/// A separate window, not a panel in the main one: the user sees settings next
/// to what they change, and closing them moves nothing. The section comes as a
/// URL hash, so the window does not need a separate command for it.
#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>, section: Option<String>) -> Result<()> {
    let section = section.unwrap_or_else(|| "general".to_string());

    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.eval(format!("location.hash = '{section}'"));
        let _ = window.set_focus();
        return Ok(());
    }

    let url = WebviewUrl::App(format!("settings.html#{section}").into());
    WebviewWindowBuilder::new(&app, SETTINGS_WINDOW, url)
        .title("Настройки — kknow")
        .inner_size(880.0, 580.0)
        .min_inner_size(720.0, 480.0)
        .build()
        .map_err(|error| Error::Window(error.to_string()))?;

    Ok(())
}

/// Settings window label. We find the window by it, so we do not open a second.
pub const SETTINGS_WINDOW: &str = "settings";

/// Renames a vault. Notes and sync settings do not change.
#[tauri::command]
pub fn rename_vault(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<Vec<VaultView>> {
    state.rename(&id, &name)?;
    Ok(list_vaults(state))
}

#[tauri::command]
pub fn set_vault_color(
    state: State<'_, AppState>,
    id: String,
    color: String,
) -> Result<Vec<VaultView>> {
    state.set_color(&id, &color)?;
    Ok(list_vaults(state))
}

/// Vault totals: notes, folders and bytes.
///
/// It counts from the open vault. So the first visit to the settings of another
/// vault opens that vault, as a global search does.
#[tauri::command]
pub fn vault_stats(state: State<'_, AppState>, vault: Option<String>) -> Result<VaultStatsView> {
    let slot = state.slot(vault.as_deref())?;
    slot.read(|workspace| {
        let stats = workspace.stats();
        Ok(VaultStatsView {
            notes: stats.notes,
            folders: stats.folders,
            bytes: stats.bytes,
        })
    })
}

/// The vault for ⌘1…⌘9; a number can have no vault.
#[tauri::command]
pub fn vault_by_hotkey(state: State<'_, AppState>, number: usize) -> Option<String> {
    state.by_hotkey(number)
}

/// Full-text search in one vault. An empty query returns an empty list.
#[tauri::command]
pub fn search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    vault: Option<String>,
) -> Result<Vec<SearchResult>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let hits = workspace.search(&query, limit.unwrap_or(50))?;

        Ok(hits
            .into_iter()
            .map(|hit| SearchResult {
                folder: hit.id.folder().map(str::to_string),
                id: hit.id.as_str().to_string(),
                title: hit.title,
                snippet: hit.snippet,
            })
            .collect())
    })
}

/// Search in all vaults at once.
///
/// Vaults open lazily, so the first such search parses the vaults that were not
/// opened yet. After that they stay in memory.
///
/// Results come in groups: the active vault first, then the others in list
/// order. We must not mix them by score: tantivy computes the score inside one
/// index, so scores from different vaults are not comparable.
/// An unavailable vault (disk disconnected) is skipped: results from the
/// others matter more than an error about one.
#[tauri::command]
pub fn search_all(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<GlobalResult>> {
    // The limit is per vault, not for the whole response. Otherwise one large
    // vault fills the list, and the others get no place.
    let limit = limit.unwrap_or(20);
    let mut found = Vec::new();

    for entry in state.search_order() {
        if !entry.available() {
            continue;
        }

        let slot = match state.slot(Some(&entry.id)) {
            Ok(slot) => slot,
            Err(error) => {
                eprintln!("хранилище {} не открылось: {error}", entry.name);
                continue;
            }
        };

        let hits = slot.read(|workspace| Ok(workspace.search(&query, limit)?))?;
        found.extend(hits.into_iter().map(|hit| GlobalResult {
            vault: entry.id.clone(),
            vault_name: entry.name.clone(),
            color: entry.color.as_str().to_string(),
            folder: hit.id.folder().map(str::to_string),
            id: hit.id.as_str().to_string(),
            title: hit.title,
            snippet: hit.snippet,
        }));
    }

    Ok(found)
}

#[tauri::command]
pub fn current_vault(state: State<'_, AppState>) -> Option<VaultInfo> {
    let entry = state.active()?;
    let slot = state.slot(Some(&entry.id)).ok()?;
    slot.read(|workspace| Ok(info(&entry, workspace))).ok()
}

#[tauri::command]
pub fn reload_vault(state: State<'_, AppState>, vault: Option<String>) -> Result<VaultInfo> {
    let slot = state.slot(vault.as_deref())?;
    slot.write(|workspace| {
        workspace.reload()?;
        Ok(info(&slot.entry, workspace))
    })
}

#[tauri::command]
pub fn list_notes(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<NoteSummary>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .notes()
            .map(|(id, parsed)| NoteSummary {
                id: id.as_str().to_string(),
                title: parsed.title().unwrap_or_else(|| id.name()).to_string(),
                folder: id.folder().map(str::to_string),
                tags: parsed.tags.iter().map(|tag| tag.name.clone()).collect(),
            })
            .collect())
    })
}

#[tauri::command]
pub fn load_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        let content = workspace.read(&id)?;
        Ok(note_view(workspace, &id, content))
    })
}

/// Save result for the window.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved {
        note: NoteView,
    },
    /// The file changed outside. The disk keeps that version, and the window
    /// text goes to the copy `copy`.
    Conflict {
        note: NoteView,
        copy: String,
    },
}

/// Saves a note.
///
/// `base` is the text the window started to edit from. With it, the write
/// replaces only the same file content, never an outside edit. Without it, the
/// write is unconditional.
///
/// `bookmarks` are bookmarks as the editor sees them: lines and names. The
/// editor moves them with edits and knows their position better than a text
/// search.
#[tauri::command]
pub fn save_note(
    state: State<'_, AppState>,
    id: String,
    content: String,
    base: Option<String>,
    bookmarks: Option<Vec<Mark>>,
    vault: Option<String>,
) -> Result<SaveOutcome> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        let written = |workspace: &mut Workspace, content: String| -> Result<SaveOutcome> {
            if let Some(marks) = &bookmarks {
                workspace.set_bookmarks(&id, marks, &content)?;
            }
            Ok(SaveOutcome::Saved {
                note: note_view(workspace, &id, content),
            })
        };
        let Some(base) = base else {
            workspace.save(&id, &content)?;
            return written(workspace, content);
        };
        Ok(match workspace.save_if_unchanged(&id, &content, &base)? {
            Saved::Written => written(workspace, content)?,
            Saved::Conflict { copy, disk } => SaveOutcome::Conflict {
                note: note_view(workspace, &id, disk),
                copy: copy.as_str().to_string(),
            },
        })
    })
}

/// Renders unsaved text for the preview while the user types.
#[tauri::command]
pub fn preview(
    state: State<'_, AppState>,
    id: String,
    content: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.render(&id, &content))
    })
}

/// Image bytes for a note. The response is raw bytes, not JSON: a number array
/// for a screenshot of a few megabytes takes much longer to parse than the
/// image itself.
#[tauri::command]
pub fn note_image(
    state: State<'_, AppState>,
    id: String,
    target: String,
    vault: Option<String>,
) -> Result<Response> {
    let path = state.slot(vault.as_deref())?.read(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace
            .image(&id, &target)
            .ok_or(Error::ImageNotFound(target))
    })?;
    let bytes = kknow_core::image::read(&path).map_err(kknow_app::Error::from)?;
    Ok(Response::new(bytes))
}

/// Saves a pasted image and returns the path for the link.
///
/// The body is raw bytes, for the same reason as in `note_image`. So the name
/// and the standalone file come in headers, percent-encoded: a header holds
/// only ASCII. The window gives only a file name. The core selects the folder:
/// `attachments/` at the vault root, or next to a standalone file.
#[tauri::command]
pub fn save_attachment(state: State<'_, AppState>, request: Request<'_>) -> Result<String> {
    let header = |key: &str| {
        let value = request.headers().get(key)?.to_str().ok()?;
        Some(percent_decode_str(value).decode_utf8_lossy().into_owned())
    };
    let dir = attachment_dir(&state, header("kknow-file").as_deref())?;
    let name = header("kknow-name").unwrap_or_default();
    let saved = match request.body() {
        InvokeBody::Raw(bytes) => kknow_core::image::save(&dir, &name, bytes),
        // The fallback IPC path sends bytes as a JSON array of numbers.
        InvokeBody::Json(value) => {
            let bytes: Vec<u8> =
                serde_json::from_value(value.clone()).map_err(|_| Error::NoImageBytes)?;
            kknow_core::image::save(&dir, &name, &bytes)
        }
    };
    Ok(saved.map_err(kknow_app::Error::from)?)
}

/// Copies an image file dropped on the editor, like `save_attachment`.
///
/// `source` comes from the shell drop event, as in `open_file`. The command
/// only reads it, and only an image of a limited size.
#[tauri::command]
pub fn attach_file(
    state: State<'_, AppState>,
    source: String,
    file: Option<String>,
) -> Result<String> {
    let dir = attachment_dir(&state, file.as_deref())?;
    Ok(kknow_core::image::import(&dir, Path::new(&source)).map_err(kknow_app::Error::from)?)
}

/// Where `attachments/` goes: the active vault root, or the folder of a
/// standalone file. The file must be in the list (`known_file`).
fn attachment_dir(state: &AppState, file: Option<&str>) -> Result<PathBuf> {
    match file {
        Some(file) => {
            let path = state.known_file(file)?;
            let dir = path
                .parent()
                .ok_or_else(|| Error::UnknownFile(file.to_string()))?;
            Ok(dir.to_path_buf())
        }
        None => state
            .slot(None)?
            .read(|workspace| Ok(workspace.root().to_path_buf())),
    }
}

#[tauri::command]
pub fn create_note(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = workspace.create(&name)?;
        let content = workspace.read(&id)?;
        Ok(note_view(workspace, &id, content))
    })
}

/// Vault themes together with global themes.
///
/// The UI puts their variables into `:root`; the core already validated the
/// values. If no vault is open, that is fine: global themes do not depend on
/// it, and a window with loose files needs them too.
#[tauri::command]
pub fn list_themes(state: State<'_, AppState>, vault: Option<String>) -> Result<ThemeSet> {
    let global = state.themes_path();
    match state.slot(vault.as_deref()) {
        Ok(slot) => slot.read(|workspace| Ok(workspace.themes(global.as_deref()))),
        Err(Error::NoVault) if vault.is_none() => Ok(theme::load(None, global.as_deref())),
        Err(error) => Err(error),
    }
}

/// Copies a vault theme to the global folder, so every vault sees it.
///
/// The window sends only `id`. The core builds the path and refuses an `id`
/// that looks like a path to another folder.
#[tauri::command]
pub fn share_theme(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    let global = state.themes_path().ok_or(Error::NoConfig)?;
    let root = state
        .slot(vault.as_deref())?
        .read(|workspace| Ok(workspace.root().to_path_buf()))?;
    theme::share(&root, &id, &global).map_err(|error| Error::ShareTheme(error.to_string()))?;
    Ok(())
}

/// Opens the global themes folder in the file manager. Creates it if missing.
///
/// It opens the folder instead of selecting it: the user comes to put a file
/// there and should not search for it inside "Application Support".
#[tauri::command]
pub fn reveal_themes(state: State<'_, AppState>) -> Result<()> {
    let dir = state.themes_path().ok_or(Error::NoConfig)?;
    fs::create_dir_all(&dir).map_err(|error| Error::Reveal(error.to_string()))?;
    open_default(&dir).map_err(|error| Error::Reveal(error.to_string()))
}

/// Shows the note file in the system file manager; without `id`, the vault
/// folder.
///
/// An own command, not a plugin: a plugin for one action brings an npm package
/// and permissions. Our icons are our own for the same reason.
///
/// The core computes the path from the note ID. The frontend sends only `id`,
/// so this command cannot open a file outside the vault.
#[tauri::command]
pub fn reveal_note(
    state: State<'_, AppState>,
    id: Option<String>,
    vault: Option<String>,
) -> Result<()> {
    let path = state
        .slot(vault.as_deref())?
        .read(|workspace| reveal_target(workspace, id.as_deref()))?;
    reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

fn reveal_target(workspace: &Workspace, id: Option<&str>) -> Result<PathBuf> {
    match id {
        Some(raw) => Ok(workspace.path_for(&note_id(workspace, raw)?)),
        None => Ok(workspace.root().to_path_buf()),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    // -R selects the file in Finder instead of opening it in an editor.
    run(Command::new("open").arg("-R").arg(path))
}

#[cfg(target_os = "windows")]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    run(Command::new("explorer").arg(format!("/select,{}", path.display())))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn reveal(path: &Path) -> std::io::Result<()> {
    // xdg-open cannot select a file, so we show its parent folder.
    run(Command::new("xdg-open").arg(path.parent().unwrap_or(path)))
}

fn run(command: &mut Command) -> std::io::Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(status.to_string()))
    }
}

/// Opens the note file in the app that the system assigns to `.md`.
#[tauri::command]
pub fn open_note(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    let path = state
        .slot(vault.as_deref())?
        .read(|workspace| reveal_target(workspace, Some(&id)))?;
    open_default(&path).map_err(|error| Error::Open(error.to_string()))
}

/// Shows a vault folder in the file manager.
///
/// The path is not taken from outside as is: it must be in the core folder
/// list. So this command can show only what is inside the vault.
#[tauri::command]
pub fn reveal_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    let path = state.slot(vault.as_deref())?.read(|workspace| {
        if !workspace.folders()?.contains(&name) {
            return Err(Error::NotFound(name.clone()));
        }
        Ok(workspace.root().join(&name))
    })?;
    reveal(&path).map_err(|error| Error::Reveal(error.to_string()))
}

#[cfg(target_os = "macos")]
fn open_default(path: &Path) -> std::io::Result<()> {
    run(Command::new("open").arg(path))
}

#[cfg(target_os = "windows")]
fn open_default(path: &Path) -> std::io::Result<()> {
    // The empty quotes are the window title. Without them `start` takes the
    // path as the title.
    run(Command::new("cmd").args(["/C", "start", ""]).arg(path))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_default(path: &Path) -> std::io::Result<()> {
    run(Command::new("xdg-open").arg(path))
}

/// Sets the note bookmarks to exactly these lines of `content`, with these
/// names. The window calls this when a bookmark is added, removed or renamed;
/// the note does not need a save.
#[tauri::command]
pub fn set_bookmarks(
    state: State<'_, AppState>,
    id: String,
    lines: Vec<Mark>,
    content: String,
    vault: Option<String>,
) -> Result<Vec<Mark>> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.set_bookmarks(&id, &lines, &content)?;
        Ok(workspace.note_bookmarks(&id, &content))
    })
}

/// All vault bookmarks, in the order they were added.
#[tauri::command]
pub fn list_bookmarks(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Vec<BookmarkView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .bookmarks()
            .into_iter()
            .filter_map(|bookmark| {
                let id = NoteId::parse(&bookmark.note)?;
                Some(BookmarkView {
                    title: title_of(workspace, &id),
                    folder: id.folder().map(str::to_string),
                    note: bookmark.note.clone(),
                    line: bookmark.line,
                    text: bookmark.text.clone(),
                    name: bookmark.name.clone(),
                })
            })
            .collect())
    })
}

/// Tasks of all vault notes, sorted by note and line.
#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<TaskView>> {
    state.slot(vault.as_deref())?.read(|workspace| {
        Ok(workspace
            .tasks()
            .into_iter()
            .map(|(id, task)| TaskView {
                note: id.as_str().to_string(),
                title: title_of(workspace, id),
                folder: id.folder().map(str::to_string),
                line: task.line,
                text: task.text.clone(),
                done: task.done,
            })
            .collect())
    })
}

/// Marks the task on a note line as done or open and returns the note.
///
/// The core reads the line from disk and refuses it if it is no longer a
/// task. The window saves its edits before the call, so the disk is current.
#[tauri::command]
pub fn set_task(
    state: State<'_, AppState>,
    id: String,
    line: usize,
    done: bool,
    vault: Option<String>,
) -> Result<NoteView> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        let content = workspace.set_task(&id, line, done)?;
        Ok(note_view(workspace, &id, content))
    })
}

/// Renames or moves a note and returns its new ID and the number of notes
/// with rewritten links.
///
/// The new name is the full path from the root, `Folder/Name`. For the core,
/// rename and move are one operation, so the window does not separate them.
#[tauri::command]
pub fn rename_note(
    state: State<'_, AppState>,
    id: String,
    to: String,
    vault: Option<String>,
) -> Result<RenamedNote> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let from = note_id(workspace, &id)?;
        let to = NoteId::parse(&to).ok_or_else(|| kknow_app::Error::InvalidName(to.clone()))?;
        let relinked = workspace.rename(&from, &to)?;
        Ok(RenamedNote {
            id: to.as_str().to_string(),
            relinked: relinked.notes,
        })
    })
}

/// Copies a note next to it and returns the copy ID.
#[tauri::command]
pub fn duplicate_note(
    state: State<'_, AppState>,
    id: String,
    vault: Option<String>,
) -> Result<String> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        Ok(workspace.duplicate(&id)?.as_str().to_string())
    })
}

/// Renames or moves a folder with all its notes. Returns the number of notes
/// with rewritten links.
#[tauri::command]
pub fn rename_folder(
    state: State<'_, AppState>,
    name: String,
    to: String,
    vault: Option<String>,
) -> Result<usize> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.rename_folder(&name, &to)?.notes))
}

#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, id: String, vault: Option<String>) -> Result<()> {
    state.slot(vault.as_deref())?.write(|workspace| {
        let id = note_id(workspace, &id)?;
        workspace.delete(&id)?;
        Ok(())
    })
}

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<String>> {
    state
        .slot(vault.as_deref())?
        .read(|workspace| Ok(workspace.folders()?))
}

#[tauri::command]
pub fn create_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.create_folder(&name)?))
}

#[tauri::command]
pub fn delete_folder(
    state: State<'_, AppState>,
    name: String,
    vault: Option<String>,
) -> Result<()> {
    state
        .slot(vault.as_deref())?
        .write(|workspace| Ok(workspace.delete_folder(&name)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    /// An app on the mock runtime: commands really run, but with no window
    /// and no webview.
    fn app(roots: &[&std::path::Path]) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(crate::state::testing(roots))
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано")
    }

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Projects")).unwrap();
        fs::write(
            dir.path().join("Index.md"),
            "# Оглавление\n\n[[Projects/kknow|мой проект]]\n",
        )
        .unwrap();
        fs::write(dir.path().join("Projects/kknow.md"), "# kknow\n\n#rust\n").unwrap();
        dir
    }

    #[test]
    fn lists_notes_with_titles_folders_and_tags() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let notes = list_notes(app.state(), None).unwrap();

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].id, "Index");
        assert_eq!(notes[0].title, "Оглавление");
        assert_eq!(notes[0].folder, None);
        assert_eq!(notes[1].folder.as_deref(), Some("Projects"));
        assert_eq!(notes[1].tags, ["rust"]);
    }

    #[test]
    fn bookmarks_come_with_the_note_and_move_with_saves() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let marks = set_bookmarks(
            app.state(),
            "Index".into(),
            vec![Mark::new(3, Some("проект"))],
            "# Оглавление\n\n[[Projects/kknow|мой проект]]\n".into(),
            None,
        )
        .unwrap();
        assert_eq!(marks, [Mark::new(3, Some("проект"))]);
        assert_eq!(
            load_note(app.state(), "Index".into(), None)
                .unwrap()
                .bookmarks,
            [Mark::new(3, Some("проект"))]
        );

        // The editor moved the bookmark with the inserted line.
        let SaveOutcome::Saved { note } = save_note(
            app.state(),
            "Index".into(),
            "# Оглавление\n\nновая строка\n[[Projects/kknow|мой проект]]\n".into(),
            None,
            Some(vec![Mark::new(4, Some("проект"))]),
            None,
        )
        .unwrap() else {
            panic!("без base сохранение безусловное");
        };
        assert_eq!(note.bookmarks, [Mark::new(4, Some("проект"))]);

        let listed = list_bookmarks(app.state(), None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Оглавление");
        assert_eq!(listed[0].line, 4);
        assert_eq!(listed[0].text, "[[Projects/kknow|мой проект]]");
        assert_eq!(listed[0].name.as_deref(), Some("проект"));
    }

    #[test]
    fn tasks_are_listed_and_set_through_the_note() {
        let dir = vault();
        fs::write(
            dir.path().join("Index.md"),
            "# Оглавление\n\n- [ ] прочитать\n",
        )
        .unwrap();
        let app = app(&[dir.path()]);

        let listed = list_tasks(app.state(), None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Оглавление");
        assert_eq!((listed[0].line, listed[0].done), (3, false));
        assert_eq!(listed[0].text, "прочитать");

        let note = set_task(app.state(), "Index".into(), 3, true, None).unwrap();
        assert_eq!(note.content, "# Оглавление\n\n- [x] прочитать\n");
        assert!(list_tasks(app.state(), None).unwrap()[0].done);
        assert!(set_task(app.state(), "Index".into(), 1, true, None).is_err());
    }

    #[test]
    fn rename_note_returns_the_new_id_and_moves_the_file() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let renamed = rename_note(
            app.state(),
            "Projects/kknow".into(),
            "Архив/kknow-old".into(),
            None,
        )
        .unwrap();
        let id = renamed.id;

        assert_eq!(id, "Архив/kknow-old");
        assert_eq!(renamed.relinked, 1, "ссылка из оглавления");
        assert!(dir.path().join("Архив/kknow-old.md").is_file());
        assert!(!dir.path().join("Projects/kknow.md").exists());
        assert!(load_note(app.state(), id, None).is_ok());
    }

    #[test]
    fn rename_note_refuses_a_bad_name() {
        let dir = vault();
        let app = app(&[dir.path()]);

        assert!(rename_note(app.state(), "Index".into(), "../away".into(), None).is_err());
        assert!(rename_note(app.state(), "Index".into(), "Projects/kknow".into(), None).is_err());
        assert!(dir.path().join("Index.md").is_file());
    }

    #[test]
    fn duplicate_note_makes_a_copy_next_to_the_note() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let copy = duplicate_note(app.state(), "Projects/kknow".into(), None).unwrap();

        assert_eq!(copy, "Projects/kknow copy");
        assert_eq!(list_notes(app.state(), None).unwrap().len(), 3);
    }

    #[test]
    fn rename_folder_moves_the_notes_inside() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let relinked = rename_folder(app.state(), "Projects".into(), "Work".into(), None).unwrap();

        assert_eq!(relinked, 1, "ссылка из оглавления");
        let notes = list_notes(app.state(), None).unwrap();
        assert!(notes.iter().any(|note| note.id == "Work/kknow"));
        assert!(
            list_folders(app.state(), None)
                .unwrap()
                .contains(&"Work".to_string())
        );
    }

    #[test]
    fn reveal_folder_refuses_paths_outside_the_vault() {
        let dir = vault();
        let app = app(&[dir.path()]);

        assert!(matches!(
            reveal_folder(app.state(), "../".into(), None),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            reveal_folder(app.state(), "Nope".into(), None),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn loads_a_note_with_html_and_backlinks() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let view = load_note(app.state(), "Projects/kknow".into(), None).unwrap();

        assert_eq!(view.title, "kknow");
        assert!(view.content.starts_with("# kknow"));
        assert!(view.html.contains("kknow</h1>"), "{}", view.html);
        assert_eq!(view.backlinks.len(), 1);
        assert_eq!(view.backlinks[0].id, "Index");
        assert_eq!(view.backlinks[0].label.as_deref(), Some("мой проект"));
    }

    #[test]
    fn short_name_finds_the_note() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let view = load_note(app.state(), "kknow".into(), None).unwrap();
        assert_eq!(view.id, "Projects/kknow");
    }

    #[test]
    fn saving_updates_links_immediately() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let SaveOutcome::Saved { note: view } = save_note(
            app.state(),
            "Projects/kknow".into(),
            "# kknow\n\nтеперь ссылаюсь на [[Index]]\n".into(),
            None,
            None,
            None,
        )
        .unwrap() else {
            panic!("без base сохранение безусловное");
        };

        assert_eq!(view.outgoing.len(), 1);
        assert_eq!(view.outgoing[0].id, "Index");

        let index = load_note(app.state(), "Index".into(), None).unwrap();
        assert_eq!(index.backlinks.len(), 1);
    }

    #[test]
    fn saving_over_an_outside_edit_keeps_both_versions() {
        let dir = vault();
        let app = app(&[dir.path()]);
        let base = load_note(app.state(), "Index".into(), None)
            .unwrap()
            .content;
        fs::write(dir.path().join("Index.md"), "# Правка агента\n").unwrap();

        let SaveOutcome::Conflict { note, copy } = save_note(
            app.state(),
            "Index".into(),
            "# Правка в окне\n".into(),
            Some(base),
            None,
            None,
        )
        .unwrap() else {
            panic!("чужая правка должна была остановить запись");
        };

        assert_eq!(note.content, "# Правка агента\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("Index.md")).unwrap(),
            note.content
        );
        let mine = load_note(app.state(), copy, None).unwrap();
        assert_eq!(mine.content, "# Правка в окне\n");
    }

    #[test]
    fn preview_renders_unsaved_text_without_touching_disk() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let html = preview(
            app.state(),
            "Index".into(),
            "черновик [[kknow]]".into(),
            None,
        )
        .unwrap();

        assert!(html.contains("kknow://note/Projects/kknow"), "{html}");
        assert_eq!(
            fs::read_to_string(dir.path().join("Index.md")).unwrap(),
            "# Оглавление\n\n[[Projects/kknow|мой проект]]\n"
        );
    }

    #[test]
    fn creating_and_deleting_a_note() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let created = create_note(app.state(), "Daily/2026-07-18".into(), None).unwrap();
        assert_eq!(created.id, "Daily/2026-07-18");
        assert!(dir.path().join("Daily/2026-07-18.md").is_file());
        assert_eq!(list_notes(app.state(), None).unwrap().len(), 3);

        delete_note(app.state(), "Daily/2026-07-18".into(), None).unwrap();
        assert!(!dir.path().join("Daily/2026-07-18.md").exists());
        assert_eq!(list_notes(app.state(), None).unwrap().len(), 2);
    }

    #[test]
    fn missing_note_returns_an_error_not_a_panic() {
        let dir = vault();
        let app = app(&[dir.path()]);

        assert!(matches!(
            load_note(app.state(), "нет такой".into(), None),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn search_finds_notes_by_content() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let results = search(app.state(), "rust".into(), None, None).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "Projects/kknow");
        assert_eq!(results[0].folder.as_deref(), Some("Projects"));
    }

    #[test]
    fn search_sees_a_note_saved_a_moment_ago() {
        let dir = vault();
        let app = app(&[dir.path()]);

        save_note(
            app.state(),
            "Index".into(),
            "# Оглавление\n\nупомянул компиляторы\n".into(),
            None,
            None,
            None,
        )
        .unwrap();

        let results = search(app.state(), "компиляторы".into(), None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "Index");
    }

    #[test]
    fn search_on_empty_query_returns_nothing() {
        let dir = vault();
        let app = app(&[dir.path()]);

        assert!(
            search(app.state(), "  ".into(), None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn commands_report_an_empty_registry() {
        let app = app(&[]);

        assert!(current_vault(app.state()).is_none());
        assert!(matches!(list_notes(app.state(), None), Err(Error::NoVault)));
    }

    #[test]
    fn themes_come_from_the_vault_and_a_vault_without_them_is_fine() {
        let dir = vault();
        let app = app(&[dir.path()]);

        let own = |set: ThemeSet| -> Vec<theme::Theme> {
            set.themes
                .into_iter()
                .filter(|theme| theme.source != theme::Source::BuiltIn)
                .collect()
        };
        assert!(own(list_themes(app.state(), None).unwrap()).is_empty());

        fs::create_dir_all(dir.path().join(".kknow/themes")).unwrap();
        fs::write(
            dir.path().join(".kknow/themes/sepia.json"),
            r##"{"name":"Сепия","base":"light","variables":{"bg":"#f4ecd8"}}"##,
        )
        .unwrap();

        let themes = own(list_themes(app.state(), None).unwrap());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "sepia");
        assert_eq!(themes[0].variables["bg"], "#f4ecd8");
    }

    #[test]
    fn global_themes_are_seen_from_any_vault_and_without_one() {
        let first = vault();
        let second = vault();
        let global = tempfile::tempdir().unwrap();
        let app = app(&[first.path(), second.path()]);
        app.state::<AppState>()
            .use_themes(Some(global.path().join("themes")));

        fs::create_dir_all(first.path().join(".kknow/themes")).unwrap();
        fs::write(
            first.path().join(".kknow/themes/sepia.json"),
            r#"{"name":"Сепия"}"#,
        )
        .unwrap();
        let first_id = app.state::<AppState>().entries()[0].id.clone();
        let second_id = app.state::<AppState>().entries()[1].id.clone();

        let own = |set: ThemeSet| -> Vec<theme::Theme> {
            set.themes
                .into_iter()
                .filter(|theme| theme.source != theme::Source::BuiltIn)
                .collect()
        };
        assert!(own(list_themes(app.state(), Some(second_id.clone())).unwrap()).is_empty());

        share_theme(app.state(), "sepia".into(), Some(first_id)).unwrap();

        let themes = own(list_themes(app.state(), Some(second_id)).unwrap());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].source, theme::Source::Global);
        assert!(matches!(
            share_theme(app.state(), "../x".into(), None),
            Err(Error::ShareTheme(_))
        ));

        let empty = self::app(&[]);
        empty
            .state::<AppState>()
            .use_themes(Some(global.path().join("themes")));
        assert_eq!(own(list_themes(empty.state(), None).unwrap()).len(), 1);
    }

    /// Tests only path resolution: the real reveal would start Finder for the
    /// person who runs the tests.
    #[test]
    fn reveal_resolves_a_note_path_and_falls_back_to_the_vault() {
        let dir = vault();
        let app = app(&[dir.path()]);
        let state = app.state::<AppState>();

        state
            .slot(None)
            .unwrap()
            .read(|workspace| {
                // Compare the root with what the vault returns: macOS
                // canonicalizes the path, and `dir.path()` differs (`/var`
                // versus `/private/var`).
                assert_eq!(
                    reveal_target(workspace, Some("kknow"))?,
                    workspace.root().join("Projects/kknow.md")
                );
                assert_eq!(reveal_target(workspace, None)?, workspace.root());
                assert!(matches!(
                    reveal_target(workspace, Some("нет такой")),
                    Err(Error::NotFound(_))
                ));
                Ok(())
            })
            .unwrap();
    }

    /// A second vault with its own tree, search and notes.
    fn other_vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Смета.md"),
            "# Смета\n\nрабочие расходы и компиляторы\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn every_command_works_with_the_active_vault_by_default() {
        let mine = vault();
        let work = other_vault();
        let app = app(&[mine.path(), work.path()]);
        let ids: Vec<String> = app
            .state::<AppState>()
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();

        assert_eq!(list_notes(app.state(), None).unwrap().len(), 2);
        assert_eq!(
            list_notes(app.state(), Some(ids[1].clone())).unwrap().len(),
            1,
            "чужое хранилище доступно по имени"
        );

        let switched = activate_vault(app.state(), ids[1].clone()).unwrap();
        assert_eq!(switched.id, ids[1]);
        assert_eq!(list_notes(app.state(), None).unwrap()[0].id, "Смета");
    }

    #[test]
    fn a_vault_can_be_added_and_forgotten() {
        let mine = vault();
        let work = other_vault();
        let app = app(&[mine.path()]);

        let added = add_vault(app.state(), work.path().to_string_lossy().into_owned()).unwrap();
        assert!(list_vaults(app.state()).iter().any(|view| view.active));
        assert_eq!(list_vaults(app.state()).len(), 2);
        // The added vault becomes active at once: the user wants it.
        assert_eq!(current_vault(app.state()).unwrap().id, added.id);

        let left = remove_vault(app.state(), added.id).unwrap();
        assert_eq!(left.len(), 1);
        assert!(work.path().join("Смета.md").is_file(), "файлы остаются");
    }

    #[test]
    fn search_covers_every_vault_and_says_where_it_found() {
        let mine = vault();
        let work = other_vault();
        let app = app(&[mine.path(), work.path()]);
        let ids: Vec<String> = app
            .state::<AppState>()
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();

        save_note(
            app.state(),
            "Index".into(),
            "# Оглавление\n\nсвои компиляторы\n".into(),
            None,
            None,
            None,
        )
        .unwrap();

        let found = search_all(app.state(), "компиляторы".into(), None).unwrap();

        assert_eq!(found.len(), 2, "находки из обоих хранилищ");
        // The active vault comes first. Scores from different indexes are not
        // comparable, so the list sets the order, not the match score.
        assert_eq!(found[0].vault, ids[0]);
        assert_eq!(found[0].id, "Index");
        assert_eq!(found[1].vault, ids[1]);
        assert_eq!(found[1].id, "Смета");
        assert!(!found[1].vault_name.is_empty());
    }

    #[test]
    fn a_global_search_opens_vaults_it_has_not_touched_yet() {
        let mine = vault();
        let work = other_vault();
        let app = app(&[mine.path(), work.path()]);
        let state = app.state::<AppState>();
        let ids: Vec<String> = state
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();

        assert!(state.opened(&ids[1]).is_none(), "второе ещё не открывали");
        search_all(app.state(), "расходы".into(), None).unwrap();

        assert!(state.opened(&ids[1]).is_some(), "поиск открыл его сам");
    }

    #[test]
    fn hotkeys_point_at_vaults_in_list_order() {
        let mine = vault();
        let work = other_vault();
        let app = app(&[mine.path(), work.path()]);
        let ids: Vec<String> = app
            .state::<AppState>()
            .entries()
            .iter()
            .map(|entry| entry.id.clone())
            .collect();

        assert_eq!(vault_by_hotkey(app.state(), 1).as_ref(), Some(&ids[0]));
        assert_eq!(vault_by_hotkey(app.state(), 2).as_ref(), Some(&ids[1]));
        assert!(vault_by_hotkey(app.state(), 3).is_none());
    }

    #[test]
    fn duplicate_links_appear_once_in_backlinks() {
        let dir = vault();
        let app = app(&[dir.path()]);

        save_note(
            app.state(),
            "Index".into(),
            "[[kknow]] и снова [[Projects/kknow]] и ещё ![[kknow]]\n".into(),
            None,
            None,
            None,
        )
        .unwrap();

        let view = load_note(app.state(), "kknow".into(), None).unwrap();
        assert_eq!(view.backlinks.len(), 1);
    }
}
