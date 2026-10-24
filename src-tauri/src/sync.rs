//! Sync from the window: settings, run and report.
//!
//! The algorithm is in `kknow-sync`. The decision "what to do with files that
//! arrived from remote" is in `kknow-app`. This module translates between them
//! and the UI: it builds settings from the form, gets the secret from the
//! keyring, and shows the report.
//!
//! While sync writes notes to disk, it cannot share the vault with autosave.
//! Edits in the editor are safe (the text is in the window), but their save
//! waits for the write step. The UI indicator exists for this reason.

use kknow_app::Workspace;
use kknow_sync::{Provider, RemoteConfig, S3Settings, SyncEngine, SyncProvider};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};

use std::sync::Arc;

use tauri::Manager;

use crate::live::{self, VaultChanged};
use crate::secrets;
use crate::state::{AppState, Error, Result, VaultSlot};
use crate::sync_status::{self, SyncState, SyncStatus, SyncStatuses};

/// Sync settings as the UI sees them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncSettings {
    #[serde(flatten)]
    config: RemoteConfig,
    /// Remote storage address as a string, to show to the user.
    location: String,
    /// The secret key is already in the keyring. The form cannot show it, but
    /// it must say that the key exists.
    has_secret: bool,
    /// Whether a secret is needed at all. A folder has none, so the form hides
    /// the key fields for it.
    needs_secret: bool,
}

/// A conflict: both sides edited the same note.
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "SyncConflict"))]
pub struct ConflictView {
    /// The note that keeps the local version.
    id: String,
    /// The copy that holds the remote version.
    copy: String,
}

/// What the sync did.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncReport {
    uploaded: usize,
    downloaded: usize,
    removed_local: usize,
    removed_remote: usize,
    conflicts: Vec<ConflictView>,
}

/// An empty form field means "not set", not an empty string.
fn tidy(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn config_of(workspace: &Workspace) -> Result<Option<RemoteConfig>> {
    Ok(RemoteConfig::load(&RemoteConfig::path(workspace.root()))?)
}

fn settings_of(config: RemoteConfig) -> Result<SyncSettings> {
    let location = config.location();
    let needs_secret = config.needs_secret();

    Ok(SyncSettings {
        // A folder has no secret, so there is no need to ask the keyring.
        has_secret: needs_secret && secrets::load(&location)?.is_some(),
        needs_secret,
        location,
        config,
    })
}

/// Connects to remote storage with the saved settings.
///
/// The core builds the provider: the window and the CLI must read the settings
/// in the same way. The secret comes from the keyring, and only when needed.
fn engine(config: &RemoteConfig) -> Result<SyncEngine<Box<dyn SyncProvider>>> {
    let secret = if config.needs_secret() {
        secrets::load(&config.location())?
    } else {
        None
    };

    Ok(kknow_sync::engine(config, secret)?)
}

/// Sync settings of the vault, if sync is configured.
#[tauri::command(async)]
pub fn sync_settings(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Option<SyncSettings>> {
    let config = state.slot(vault.as_deref())?.read(config_of)?;
    config.map(settings_of).transpose()
}

/// Saves the settings. The secret goes to the keyring, the rest to the vault.
///
/// `secret` = `None` means "keep the stored key". The form sends an empty field
/// each time the user does not type the key again.
#[tauri::command(async)]
pub fn save_sync_settings(
    state: State<'_, AppState>,
    settings: RemoteConfig,
    secret: Option<String>,
    vault: Option<String>,
) -> Result<SyncSettings> {
    let config = RemoteConfig {
        provider: tidy_provider(settings.provider),
        policy: settings.policy,
        auto_sync_minutes: settings.auto_sync_minutes,
    };

    // Settings first: if the bucket settings are not saved, there is no address
    // to store the key under.
    state
        .slot(vault.as_deref())?
        .read(|workspace| Ok(config.save(&RemoteConfig::path(workspace.root()))?))?;

    if let Some(secret) = tidy(secret).filter(|_| config.needs_secret()) {
        secrets::store(&config.location(), &secret)?;
    }
    settings_of(config)
}

/// Applies [`tidy`] to each provider field.
fn tidy_provider(provider: Provider) -> Provider {
    match provider {
        Provider::S3(s3) => Provider::S3(S3Settings {
            bucket: s3.bucket.trim().to_string(),
            endpoint: tidy(s3.endpoint),
            region: tidy(s3.region),
            prefix: tidy(s3.prefix),
            access_key_id: tidy(s3.access_key_id),
        }),
        Provider::Folder { path } => Provider::Folder { path },
    }
}

/// Turns off sync: forgets both the settings and the secret key.
///
/// The snapshot of the last sync stays. If the user connects the storage
/// again, it prevents treating all notes as conflicts.
#[tauri::command(async)]
pub fn forget_sync_settings(state: State<'_, AppState>, vault: Option<String>) -> Result<()> {
    let slot = state.slot(vault.as_deref())?;
    let Some(config) = slot.read(config_of)? else {
        return Ok(());
    };

    if config.needs_secret() {
        secrets::forget(&config.location())?;
    }
    slot.read(|workspace| Ok(RemoteConfig::forget(&RemoteConfig::path(workspace.root()))?))
}

/// What sync would do now, without changes.
///
/// Same as `kknow sync --dry-run` in the CLI: a way to check the address and
/// keys without a write. The window does not call it yet, because there is no
/// place to show the plan. The function stays: it is cheap and returns with a
/// "show plan" feature.
#[allow(dead_code)]
pub fn sync_preview(state: State<'_, AppState>, vault: Option<String>) -> Result<Vec<String>> {
    let slot = state.slot(vault.as_deref())?;
    let config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let engine = engine(&config)?;

    slot.read(|workspace| {
        let plan = workspace.sync_plan(&engine)?;
        Ok(plan.actions().iter().map(ToString::to_string).collect())
    })
}

/// Syncs the vault with its remote storage.
#[tauri::command(async)]
pub fn sync_now<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<SyncReport> {
    let slot = state.slot(vault.as_deref())?;
    exchange(&app, &slot)
}

/// Auto-sync interval of this vault, in minutes.
///
/// The scheduler reads it from disk on each tick: the setting can change, and
/// a thread restart is not necessary.
pub fn auto_sync_minutes(slot: &VaultSlot) -> u32 {
    slot.read(config_of)
        .ok()
        .flatten()
        .map(|config| config.auto_sync_minutes)
        .unwrap_or(0)
}

/// The full sync: staging under the lock, network without it, write under it
/// again.
///
/// Sync has three parts for this reason. Before, the vault was locked for the
/// whole time, including the network, and a note save waited for the end.
pub fn exchange<R: Runtime>(app: &AppHandle<R>, slot: &Arc<VaultSlot>) -> Result<SyncReport> {
    let id = slot.entry.id.clone();
    let statuses = app.state::<SyncStatuses>();

    let config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let location = config.location();

    // The connection is part of the sync too. Unavailable storage must show in
    // the status, not escape as an error past the status bar.
    let engine = match engine(&config) {
        Ok(engine) => engine,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    // Stage under the lock: the plan and the content to upload.
    let staged = match slot.read(|workspace| Ok(workspace.sync_stage(&engine)?)) {
        Ok(staged) => staged,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    statuses.update(app, &id, |status| {
        status.state = SyncState::Syncing;
        status.location = Some(location.clone());
        status.auto_sync_minutes = config.auto_sync_minutes;
        status.done = 0;
        status.total = staged.len();
        status.message = None;
    });

    // Network. The vault is free during this step: editor edits save without
    // a wait for the sync.
    let progress_app = app.clone();
    let progress_id = id.clone();
    let fetched = engine.transfer(staged, &mut |progress| {
        app.state::<SyncStatuses>()
            .update(&progress_app, &progress_id, |status| {
                status.done = progress.done;
                status.total = progress.total;
            });
    });

    let fetched = match fetched {
        Ok(fetched) => fetched,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error.into())),
    };

    // Only the write of downloaded files takes the lock again.
    let report = match slot.write(|workspace| Ok(workspace.sync_finish(&engine, fetched)?)) {
        Ok(report) => report,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    // Remote notes reach the UI with the same event as external editor edits:
    // the window has one update path, not two. The watcher does not report
    // them: the index already has these files, so they do not look like a new
    // change.
    let changed = VaultChanged {
        vault: id.clone(),
        updated: report.local_updated().map(ToString::to_string).collect(),
        removed: report.local_removed().map(ToString::to_string).collect(),
    };
    if !changed.is_empty() {
        let _ = app.emit(live::EVENT, changed);
    }

    let conflicts = slot.read(|workspace| Ok(workspace.conflicts().len()))?;
    statuses.update(app, &id, |status| {
        status.state = if conflicts > 0 {
            SyncState::Conflict
        } else {
            SyncState::Idle
        };
        status.done = status.total;
        status.at = Some(sync_status::now_millis());
        status.conflicts = conflicts;
        status.message = None;
        status.location = Some(location.clone());
        status.auto_sync_minutes = config.auto_sync_minutes;
    });

    Ok(SyncReport {
        uploaded: report.uploaded.len(),
        downloaded: report.downloaded.len(),
        removed_local: report.removed_local.len(),
        removed_remote: report.removed_remote.len(),
        conflicts: report
            .conflicts
            .iter()
            .map(|conflict| ConflictView {
                id: conflict.id.as_str().to_string(),
                copy: conflict.copy.as_str().to_string(),
            })
            .collect(),
    })
}

/// A sync failure. It lives in the status bar segment, not in a modal dialog,
/// and stays visible until the next successful attempt.
fn failed<R: Runtime>(
    app: &AppHandle<R>,
    statuses: &SyncStatuses,
    vault: &str,
    location: &str,
    error: Error,
) -> Error {
    statuses.update(app, vault, |status| {
        status.state = SyncState::Error;
        status.message = Some((&error).into());
        status.location = Some(location.to_string());
    });
    error
}

/// Sync status of one vault; without an ID, of the active vault.
#[tauri::command(async)]
pub fn sync_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    statuses: State<'_, SyncStatuses>,
    vault: Option<String>,
) -> Result<SyncStatus> {
    let slot = state.slot(vault.as_deref())?;
    Ok(refresh_status(&app, &statuses, &slot))
}

/// Statuses of all vaults that were opened.
///
/// The status bar chip also shows conflicts in other vaults, so the window
/// needs the full list, not only the active vault.
#[tauri::command(async)]
pub fn sync_statuses<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    statuses: State<'_, SyncStatuses>,
) -> Vec<SyncStatus> {
    for entry in state.entries() {
        if let Some(slot) = state.opened(&entry.id) {
            refresh_status(&app, &statuses, &slot);
        }
    }
    statuses.all()
}

/// Recalculates what is visible without a sync: whether sync is configured
/// and how many conflict copies the vault has.
fn refresh_status<R: Runtime>(
    app: &AppHandle<R>,
    statuses: &SyncStatuses,
    slot: &Arc<VaultSlot>,
) -> SyncStatus {
    let config = slot.read(config_of).ok().flatten();
    let conflicts = slot
        .read(|workspace| Ok(workspace.conflicts().len()))
        .unwrap_or(0);

    statuses.update(app, &slot.entry.id, |status| {
        status.conflicts = conflicts;
        status.location = config.as_ref().map(|config| config.location());
        status.auto_sync_minutes = config.as_ref().map_or(0, |config| config.auto_sync_minutes);

        // Do not touch a running sync: it has its own status and progress.
        if status.state == SyncState::Syncing {
            return;
        }
        status.state = match (config.is_some(), conflicts > 0, status.state) {
            (false, _, _) => SyncState::Off,
            // The error stays until the next successful sync.
            (true, _, SyncState::Error) => SyncState::Error,
            (true, true, _) => SyncState::Conflict,
            (true, false, _) => SyncState::Idle,
        };
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use kknow_sync::ConflictPolicy;
    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    fn app(root: &std::path::Path) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(crate::state::testing(&[root]))
            // Sync statuses are a separate managed state, as `run()` does in
            // the real app.
            .manage(SyncStatuses::default())
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано")
    }

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();
        dir
    }

    /// Each test has its own bucket. In the test build, secrets live in shared
    /// process memory, and the storage address is their key.
    fn form(bucket: &str) -> RemoteConfig {
        RemoteConfig {
            policy: ConflictPolicy::PreferLocal,
            ..RemoteConfig::s3(S3Settings {
                bucket: bucket.into(),
                endpoint: Some("http://localhost:9000".into()),
                prefix: Some("  ".into()),
                access_key_id: Some("minioadmin".into()),
                ..S3Settings::default()
            })
        }
    }

    /// Settings for a shared folder: no bucket and no keys.
    fn folder_form(path: &std::path::Path) -> RemoteConfig {
        RemoteConfig::folder(path)
    }

    fn s3_of(settings: &SyncSettings) -> &S3Settings {
        match &settings.config.provider {
            Provider::S3(s3) => s3,
            Provider::Folder { .. } => panic!("ожидалась настройка S3"),
        }
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let dir = vault();
        let app = app(dir.path());

        let saved = save_sync_settings(app.state(), form("round-trip"), Some("тайна".into()), None)
            .unwrap();

        assert_eq!(saved.location, "http://localhost:9000/round-trip/");
        assert!(saved.has_secret);

        let loaded = sync_settings(app.state(), None).unwrap().unwrap();
        assert_eq!(loaded.config, saved.config);
        assert_eq!(loaded.config.policy, ConflictPolicy::PreferLocal);
        assert!(loaded.has_secret);
    }

    #[test]
    fn empty_fields_become_absent_ones() {
        let dir = vault();
        let app = app(dir.path());

        let saved = save_sync_settings(app.state(), form("empty-fields"), None, None).unwrap();

        assert_eq!(s3_of(&saved).prefix, None, "пробелы — это не префикс");
        assert_eq!(s3_of(&saved).region, None);
        assert!(!saved.has_secret, "пустой ключ не сохраняется");
    }

    #[test]
    fn the_secret_stays_out_of_the_vault() {
        let dir = vault();
        let app = app(dir.path());

        save_sync_settings(
            app.state(),
            form("out-of-vault"),
            Some("тайна".into()),
            None,
        )
        .unwrap();

        let written = fs::read_to_string(dir.path().join(".kknow/remote.json")).unwrap();
        assert!(!written.contains("тайна"), "{written}");
    }

    #[test]
    fn saving_again_keeps_the_stored_secret() {
        let dir = vault();
        let app = app(dir.path());
        save_sync_settings(
            app.state(),
            form("keeps-secret"),
            Some("тайна".into()),
            None,
        )
        .unwrap();

        // The form sends an empty field until the user types the key again.
        let saved = save_sync_settings(app.state(), form("keeps-secret"), None, None).unwrap();

        assert!(saved.has_secret);
    }

    #[test]
    fn forgetting_removes_the_settings_and_the_secret() {
        let dir = vault();
        let app = app(dir.path());
        save_sync_settings(app.state(), form("forget"), Some("тайна".into()), None).unwrap();

        forget_sync_settings(app.state(), None).unwrap();

        assert!(sync_settings(app.state(), None).unwrap().is_none());
        assert!(!dir.path().join(".kknow/remote.json").exists());
        // The user can add the settings again: the old key must not come back.
        let saved = save_sync_settings(app.state(), form("forget"), None, None).unwrap();
        assert!(!saved.has_secret);
    }

    /// A folder has no secrets: it does not touch the keyring, and the form
    /// does not need the key fields.
    #[test]
    fn a_folder_needs_no_keychain() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());

        let saved = save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();

        assert!(!saved.needs_secret);
        assert!(!saved.has_secret);
        assert_eq!(saved.location, cloud.path().to_string_lossy());

        // Sync through the folder really runs, without network or keys.
        let report = sync_now(app.handle().clone(), app.state(), None).unwrap();
        assert_eq!(report.uploaded, 1);
        assert!(cloud.path().join("notes/Index.md").is_file());
    }

    /// This is why sync has parts: during the network step the vault is free,
    /// and a note saves without a wait for the sync.
    #[test]
    fn a_note_can_be_saved_while_the_exchange_is_on_the_network() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();

        let state = app.state::<AppState>();
        let slot = state.slot(None).unwrap();
        let config = slot.read(config_of).unwrap().unwrap();
        let engine = engine(&config).unwrap();

        // Staging holds the lock. Then the "network" step starts.
        let staged = slot
            .read(|workspace| Ok(workspace.sync_stage(&engine).unwrap()))
            .unwrap();

        // During the transfer the window saves an edit: the lock is free.
        let saved = std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                slot.write(|workspace| {
                    let id = kknow_core::NoteId::parse("Index").unwrap();
                    workspace.save(&id, "# Оглавление\n\nправка во время обмена\n")?;
                    Ok(())
                })
            });
            let fetched = engine.transfer(staged, &mut |_| {}).unwrap();
            let outcome = writer.join().expect("поток записи не паниковал");
            slot.write(|workspace| Ok(workspace.sync_finish(&engine, fetched)?))
                .unwrap();
            outcome
        });

        assert!(saved.is_ok(), "сохранение не должно ждать конца обмена");
        assert!(cloud.path().join("notes/Index.md").is_file());
    }

    /// The status is visible without a sync: whether sync is configured and
    /// how many conflict copies the vault has.
    #[test]
    fn status_says_off_conflict_and_idle() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let handle = app.handle().clone();

        let before = sync_status(handle.clone(), app.state(), app.state(), None).unwrap();
        assert_eq!(before.state, SyncState::Off);

        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();
        let ready = sync_status(handle.clone(), app.state(), app.state(), None).unwrap();
        assert_eq!(ready.state, SyncState::Idle);
        assert_eq!(
            ready.location.as_deref(),
            Some(cloud.path().to_string_lossy().as_ref())
        );

        // A conflict copy is a file in the folder; the list comes from files.
        fs::write(
            dir.path().join("Index (конфликт 3f9a1c2b).md"),
            "# чужая версия\n",
        )
        .unwrap();
        app.state::<AppState>()
            .slot(None)
            .unwrap()
            .write(|workspace| Ok(workspace.reload()?))
            .unwrap();

        let conflicted = sync_status(handle, app.state(), app.state(), None).unwrap();
        assert_eq!(conflicted.state, SyncState::Conflict);
        assert_eq!(conflicted.conflicts, 1);
    }

    /// A sync reports itself: progress reaches the end, and the time of the
    /// last success is saved.
    #[test]
    fn a_finished_exchange_reports_progress_and_time() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();

        sync_now(app.handle().clone(), app.state(), None).unwrap();

        let status = app
            .state::<SyncStatuses>()
            .get(&app.state::<AppState>().active().unwrap().id);
        assert_eq!(status.state, SyncState::Idle);
        assert_eq!(status.done, status.total);
        assert!(status.total > 0, "обмену было что делать");
        assert!(status.at.is_some(), "время удачного обмена запомнено");
    }

    /// A storage failure shows in the status and stays until the next success.
    #[test]
    fn a_failed_exchange_leaves_the_error_visible() {
        let dir = vault();
        let app = app(dir.path());
        // A folder that cannot open: a file takes the path.
        let taken = dir.path().join("Index.md");
        save_sync_settings(app.state(), RemoteConfig::folder(&taken), None, None).unwrap();

        assert!(sync_now(app.handle().clone(), app.state(), None).is_err());

        let status = app
            .state::<SyncStatuses>()
            .get(&app.state::<AppState>().active().unwrap().id);
        assert_eq!(status.state, SyncState::Error);
        assert!(status.message.is_some());

        // A recalculation does not clear the error: it stays until a success.
        let again = sync_status(app.handle().clone(), app.state(), app.state(), None).unwrap();
        assert_eq!(again.state, SyncState::Error);
    }

    #[test]
    fn forgetting_nothing_is_not_an_error() {
        let dir = vault();
        let app = app(dir.path());

        assert!(forget_sync_settings(app.state(), None).is_ok());
    }

    #[test]
    fn an_unconfigured_vault_says_so_instead_of_connecting() {
        let dir = vault();
        let app = app(dir.path());

        assert!(sync_settings(app.state(), None).unwrap().is_none());
        assert!(matches!(
            sync_now(app.handle().clone(), app.state(), None),
            Err(Error::NotConfigured)
        ));
        assert!(matches!(
            sync_preview(app.state(), None),
            Err(Error::NotConfigured)
        ));
    }

    #[test]
    fn a_bucketless_form_is_refused() {
        let dir = vault();
        let app = app(dir.path());

        let settings = RemoteConfig::s3(S3Settings {
            bucket: " ".into(),
            ..S3Settings::default()
        });

        assert!(save_sync_settings(app.state(), settings, None, None).is_err());
        assert!(!dir.path().join(".kknow/remote.json").exists());
    }
}
