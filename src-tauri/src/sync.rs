use kknow_app::Workspace;
use kknow_crypto::DataKey;
use kknow_sync::{GitSettings, Provider, RemoteConfig, S3Settings, SyncEngine, SyncProvider};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};

use std::sync::Arc;

use tauri::Manager;

use crate::live::{self, VaultChanged};
use crate::secrets;
use crate::state::{AppState, Error, Result, VaultSlot};
use crate::sync_status::{self, SyncState, SyncStatus, SyncStatuses};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncSettings {
    #[serde(flatten)]
    config: RemoteConfig,
    location: String,
    has_secret: bool,
    needs_secret: bool,
    has_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EncryptionOn {
    recovery_code: String,
    settings: SyncSettings,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(rename = "SyncConflict"))]
pub struct ConflictView {
    id: String,
    copy: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncReport {
    uploaded: usize,
    downloaded: usize,
    removed_local: usize,
    removed_remote: usize,
    pub(crate) pushed: usize,
    conflicts: Vec<ConflictView>,
}

fn tidy(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn config_of(workspace: &Workspace) -> Result<Option<RemoteConfig>> {
    Ok(RemoteConfig::load(&RemoteConfig::path(workspace.root()))?)
}

impl SyncSettings {
    #[cfg(all(test, unix))]
    pub fn location(&self) -> &str {
        &self.location
    }

    #[cfg(all(test, unix))]
    pub fn has_secret(&self) -> bool {
        self.has_secret
    }
}

pub(crate) fn settings_of(config: RemoteConfig) -> Result<SyncSettings> {
    let location = config.location();
    let needs_secret = config.needs_secret();

    Ok(SyncSettings {
        has_secret: needs_secret && secrets::load(&location)?.is_some(),
        needs_secret,
        has_key: config.encrypted && secrets::load(&key_entry(&location))?.is_some(),
        location,
        config,
    })
}

fn key_entry(location: &str) -> String {
    format!("{location} encryption key")
}

fn data_key(config: &RemoteConfig) -> Result<Option<DataKey>> {
    if !config.encrypted {
        return Ok(None);
    }
    match secrets::load(&key_entry(&config.location()))? {
        Some(text) => Ok(Some(
            DataKey::import(&text).map_err(kknow_sync::Error::from)?,
        )),
        None => Ok(None),
    }
}

fn secret(config: &RemoteConfig) -> Result<Option<String>> {
    if config.needs_secret() {
        secrets::load(&config.location())
    } else {
        Ok(None)
    }
}

fn storage(config: &RemoteConfig) -> Result<Box<dyn SyncProvider>> {
    Ok(kknow_sync::storage(config, secret(config)?)?)
}

#[tauri::command(async)]
pub fn sync_encrypt(
    state: State<'_, AppState>,
    passphrase: String,
    vault: Option<String>,
) -> Result<EncryptionOn> {
    let slot = state.slot(vault.as_deref())?;
    let mut config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let (key, code) = kknow_sync::encrypted::init(&*storage(&config)?, &passphrase)?;
    secrets::store(&key_entry(&config.location()), &key.export())?;

    config.encrypted = true;
    slot.read(|workspace| Ok(config.save(&RemoteConfig::path(workspace.root()))?))?;
    Ok(EncryptionOn {
        recovery_code: code.display().to_string(),
        settings: settings_of(config)?,
    })
}

#[tauri::command(async)]
pub fn sync_unlock(
    state: State<'_, AppState>,
    secret: String,
    vault: Option<String>,
) -> Result<SyncSettings> {
    let slot = state.slot(vault.as_deref())?;
    let mut config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let key = kknow_sync::encrypted::unlock(&*storage(&config)?, &secret)?;
    secrets::store(&key_entry(&config.location()), &key.export())?;

    config.encrypted = true;
    slot.read(|workspace| Ok(config.save(&RemoteConfig::path(workspace.root()))?))?;
    settings_of(config)
}

#[tauri::command(async)]
pub fn sync_change_passphrase(
    state: State<'_, AppState>,
    current: String,
    new: String,
    vault: Option<String>,
) -> Result<()> {
    let slot = state.slot(vault.as_deref())?;
    let config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let key = kknow_sync::encrypted::change_passphrase(&*storage(&config)?, &current, &new)?;
    secrets::store(&key_entry(&config.location()), &key.export())
}

fn engine(config: &RemoteConfig) -> Result<SyncEngine<Box<dyn SyncProvider>>> {
    let key = data_key(config)?;
    Ok(kknow_app::merge::sync_engine(
        config,
        secret(config)?,
        key.as_ref(),
    )?)
}

#[tauri::command(async)]
pub fn sync_settings(
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<Option<SyncSettings>> {
    let config = state.slot(vault.as_deref())?.read(config_of)?;
    config.map(settings_of).transpose()
}

#[tauri::command(async)]
pub fn save_sync_settings(
    state: State<'_, AppState>,
    settings: RemoteConfig,
    secret: Option<String>,
    vault: Option<String>,
) -> Result<SyncSettings> {
    let slot = state.slot(vault.as_deref())?;
    let old = slot.read(config_of)?;

    let mut config = RemoteConfig {
        provider: tidy_provider(settings.provider),
        policy: settings.policy,
        auto_sync_minutes: settings.auto_sync_minutes,
        encrypted: false,
    };
    config.encrypted = old
        .as_ref()
        .is_some_and(|old| old.encrypted && old.location() == config.location());

    slot.read(|workspace| Ok(config.save(&RemoteConfig::path(workspace.root()))?))?;

    match tidy(secret).filter(|_| config.needs_secret()) {
        Some(secret) => secrets::store(&config.location(), &secret)?,
        None => carry_secret(old.as_ref(), &config)?,
    }
    settings_of(config)
}

fn carry_secret(old: Option<&RemoteConfig>, new: &RemoteConfig) -> Result<()> {
    let Some(old) = old else { return Ok(()) };
    let same_kind = std::mem::discriminant(&old.provider) == std::mem::discriminant(&new.provider);
    if !same_kind || !old.needs_secret() || !new.needs_secret() {
        return Ok(());
    }
    let (from, to) = (old.location(), new.location());
    if from == to || secrets::load(&to)?.is_some() {
        return Ok(());
    }
    if let Some(secret) = secrets::load(&from)? {
        secrets::store(&to, &secret)?;
    }
    Ok(())
}

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
        Provider::Git(git) => Provider::Git(GitSettings {
            url: git.url.trim().to_string(),
            auth: git.auth,
            env: tidy(git.env),
        }),
    }
}

#[tauri::command(async)]
pub fn forget_sync_settings(state: State<'_, AppState>, vault: Option<String>) -> Result<()> {
    let slot = state.slot(vault.as_deref())?;
    let Some(config) = slot.read(config_of)? else {
        return Ok(());
    };

    if config.needs_secret() {
        secrets::forget(&config.location())?;
    }
    if config.encrypted {
        secrets::forget(&key_entry(&config.location()))?;
    }
    slot.read(|workspace| Ok(RemoteConfig::forget(&RemoteConfig::path(workspace.root()))?))
}

#[tauri::command(async)]
pub fn sync_now<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    vault: Option<String>,
) -> Result<SyncReport> {
    let slot = state.slot(vault.as_deref())?;
    exchange(&app, &slot)
}

pub fn auto_sync_minutes(slot: &VaultSlot) -> u32 {
    slot.read(config_of)
        .ok()
        .flatten()
        .map(|config| config.auto_sync())
        .unwrap_or(0)
}

pub fn exchange<R: Runtime>(app: &AppHandle<R>, slot: &Arc<VaultSlot>) -> Result<SyncReport> {
    let _turn = slot.begin_sync().ok_or(Error::SyncBusy)?;
    exchange_in_turn(app, slot)
}

fn exchange_in_turn<R: Runtime>(app: &AppHandle<R>, slot: &Arc<VaultSlot>) -> Result<SyncReport> {
    let id = slot.entry.id.clone();
    let statuses = app.state::<SyncStatuses>();

    let config = slot.read(config_of)?.ok_or(Error::NotConfigured)?;
    let location = config.location();

    if config.is_git() {
        return exchange_git(app, slot, &config);
    }

    let engine = match engine(&config) {
        Ok(engine) => engine,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    let staged = match slot.read(|workspace| Ok(workspace.sync_stage(&engine)?)) {
        Ok(staged) => staged,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    statuses.update(app, &id, |status| {
        status.state = SyncState::Syncing;
        status.location = Some(location.clone());
        status.auto_sync_minutes = config.auto_sync();
        status.done = 0;
        status.total = staged.len();
        status.message = None;
    });

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

    let report = match slot.write(|workspace| Ok(workspace.sync_finish(&engine, fetched)?)) {
        Ok(report) => report,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };

    let threaded = slot
        .read(|workspace| Ok(workspace.notes_of_shared(&report.files_local)))
        .unwrap_or_default();
    let mut updated: Vec<String> = report.local_updated().map(ToString::to_string).collect();
    for note in threaded {
        let note = note.as_str().to_string();
        if !updated.contains(&note) {
            updated.push(note);
        }
    }
    let changed = VaultChanged {
        vault: id.clone(),
        updated,
        removed: report.local_removed().map(ToString::to_string).collect(),
    };
    if !changed.is_empty() {
        let _ = app.emit(live::EVENT, changed);
    }

    succeeded(app, slot, &config)?;

    Ok(SyncReport {
        uploaded: report.uploaded.len(),
        downloaded: report.downloaded.len(),
        removed_local: report.removed_local.len(),
        removed_remote: report.removed_remote.len(),
        pushed: 0,
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

fn exchange_git<R: Runtime>(
    app: &AppHandle<R>,
    slot: &Arc<VaultSlot>,
    config: &RemoteConfig,
) -> Result<SyncReport> {
    let id = slot.entry.id.clone();
    let statuses = app.state::<SyncStatuses>();
    let location = config.location();

    statuses.update(app, &id, |status| {
        status.state = SyncState::Syncing;
        status.location = Some(location.clone());
        status.done = 0;
        status.total = 0;
        status.message = None;
    });

    let (merged, pushed) = match crate::git::exchange(app, slot, config) {
        Ok(done) => done,
        Err(error) => return Err(failed(app, &statuses, &id, &location, error)),
    };
    succeeded(app, slot, config)?;

    Ok(SyncReport {
        downloaded: merged.changed.len(),
        pushed,
        conflicts: merged
            .conflicts
            .into_iter()
            .map(|conflict| ConflictView {
                id: note_or_path(&conflict.path),
                copy: note_or_path(&conflict.copy),
            })
            .collect(),
        ..SyncReport::default()
    })
}

fn note_or_path(path: &str) -> String {
    kknow_core::NoteId::from_relative_path(std::path::Path::new(path))
        .map_or_else(|| path.to_string(), |id| id.as_str().to_string())
}

fn succeeded<R: Runtime>(
    app: &AppHandle<R>,
    slot: &VaultSlot,
    config: &RemoteConfig,
) -> Result<()> {
    let conflicts = slot.read(|workspace| Ok(workspace.conflicts().len()))?;
    app.state::<SyncStatuses>()
        .update(app, &slot.entry.id, |status| {
            status.state = if conflicts > 0 {
                SyncState::Conflict
            } else {
                SyncState::Idle
            };
            status.done = status.total;
            status.at = Some(sync_status::now_millis());
            status.conflicts = conflicts;
            status.message = None;
            status.location = Some(config.location());
            status.auto_sync_minutes = config.auto_sync();
        });
    Ok(())
}

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
        status.auto_sync_minutes = config.as_ref().map_or(0, RemoteConfig::auto_sync);

        if status.state == SyncState::Syncing {
            return;
        }
        status.state = match (config.is_some(), conflicts > 0, status.state) {
            (false, _, _) => SyncState::Off,
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
            .manage(SyncStatuses::default())
            .build(mock_context(noop_assets()))
            .expect("мок-приложение собрано")
    }

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Index.md"), "# Оглавление\n").unwrap();
        dir
    }

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

    fn folder_form(path: &std::path::Path) -> RemoteConfig {
        RemoteConfig::folder(path)
    }

    fn s3_of(settings: &SyncSettings) -> &S3Settings {
        match &settings.config.provider {
            Provider::S3(s3) => s3,
            _ => panic!("ожидалась настройка S3"),
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
    fn a_new_endpoint_keeps_the_stored_secret() {
        let dir = vault();
        let app = app(dir.path());
        save_sync_settings(
            app.state(),
            form("new-endpoint"),
            Some("secret".into()),
            None,
        )
        .unwrap();

        let mut moved = form("new-endpoint");
        if let Provider::S3(s3) = &mut moved.provider {
            s3.endpoint = Some("https://s3.example.com".into());
            s3.prefix = Some("notes".into());
        }
        let saved = save_sync_settings(app.state(), moved, None, None).unwrap();

        assert_eq!(saved.location, "https://s3.example.com/new-endpoint/notes");
        assert!(saved.has_secret, "the key follows the new address");
        assert_eq!(
            secrets::load(&saved.location).unwrap().as_deref(),
            Some("secret")
        );
        assert_eq!(
            secrets::load("http://localhost:9000/new-endpoint/")
                .unwrap()
                .as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn a_key_typed_with_the_new_endpoint_wins() {
        let dir = vault();
        let app = app(dir.path());
        save_sync_settings(app.state(), form("typed-wins"), Some("old".into()), None).unwrap();

        let mut moved = form("typed-wins");
        if let Provider::S3(s3) = &mut moved.provider {
            s3.endpoint = Some("https://typed-wins.example.com".into());
        }
        let saved = save_sync_settings(app.state(), moved, Some("new".into()), None).unwrap();

        assert_eq!(
            secrets::load(&saved.location).unwrap().as_deref(),
            Some("new")
        );
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
        let saved = save_sync_settings(app.state(), form("forget"), None, None).unwrap();
        assert!(!saved.has_secret);
    }

    #[test]
    fn a_folder_needs_no_keychain() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());

        let saved = save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();

        assert!(!saved.needs_secret);
        assert!(!saved.has_secret);
        assert_eq!(saved.location, cloud.path().to_string_lossy());

        let report = sync_now(app.handle().clone(), app.state(), None).unwrap();
        assert_eq!(report.uploaded, 1);
        assert!(cloud.path().join("Index.md").is_file());
    }

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

        let staged = slot
            .read(|workspace| Ok(workspace.sync_stage(&engine).unwrap()))
            .unwrap();

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
        assert!(cloud.path().join("Index.md").is_file());
    }

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

        fs::write(
            dir.path().join("Index (conflict 3f9a1c2b).md"),
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

    #[test]
    fn a_failed_exchange_leaves_the_error_visible() {
        let dir = vault();
        let app = app(dir.path());
        let taken = dir.path().join("Index.md");
        save_sync_settings(app.state(), RemoteConfig::folder(&taken), None, None).unwrap();

        assert!(sync_now(app.handle().clone(), app.state(), None).is_err());

        let status = app
            .state::<SyncStatuses>()
            .get(&app.state::<AppState>().active().unwrap().id);
        assert_eq!(status.state, SyncState::Error);
        assert!(status.message.is_some());

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

    #[test]
    fn an_encrypted_folder_from_turn_on_to_a_second_device() {
        let cloud = tempfile::tempdir().unwrap();
        let laptop_dir = vault();
        let laptop = app(laptop_dir.path());
        save_sync_settings(laptop.state(), folder_form(cloud.path()), None, None).unwrap();

        let on = sync_encrypt(laptop.state(), "correct horse".into(), None).unwrap();
        assert!(on.settings.config.encrypted && on.settings.has_key);
        sync_now(laptop.handle().clone(), laptop.state(), None).unwrap();
        assert!(!cloud.path().join("Index.md").exists());
        assert!(cloud.path().join(".kknow/key.json").is_file());

        let desktop_dir = tempfile::tempdir().unwrap();
        let desktop = app(desktop_dir.path());
        save_sync_settings(desktop.state(), folder_form(cloud.path()), None, None).unwrap();
        let error = sync_now(desktop.handle().clone(), desktop.state(), None).unwrap_err();
        assert_eq!(crate::errors::ErrorView::from(&error).code, "storageLocked");

        let error = sync_unlock(desktop.state(), "wrong horse".into(), None).unwrap_err();
        assert_eq!(
            crate::errors::ErrorView::from(&error).code,
            "wrongPassphrase"
        );
        let unlocked = sync_unlock(desktop.state(), on.recovery_code.clone(), None).unwrap();
        assert!(unlocked.config.encrypted);
        let report = sync_now(desktop.handle().clone(), desktop.state(), None).unwrap();
        assert_eq!(report.downloaded, 1);
        assert!(desktop_dir.path().join("Index.md").is_file());

        sync_change_passphrase(
            desktop.state(),
            "correct horse".into(),
            "battery staple".into(),
            None,
        )
        .unwrap();
        assert!(sync_unlock(laptop.state(), "battery staple".into(), None).is_ok());
        sync_now(laptop.handle().clone(), laptop.state(), None).unwrap();
    }

    #[test]
    fn a_new_address_is_not_encrypted_by_inheritance() {
        let cloud = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let dir = vault();
        let app = app(dir.path());
        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();
        sync_encrypt(app.state(), "correct horse".into(), None).unwrap();

        let same = save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();
        assert!(same.config.encrypted);
        let moved = save_sync_settings(app.state(), folder_form(other.path()), None, None).unwrap();
        assert!(!moved.config.encrypted);
    }

    #[test]
    fn a_busy_vault_refuses_a_second_exchange_and_keeps_its_status() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();
        let slot = app.state::<AppState>().slot(None).unwrap();
        let before = app.state::<SyncStatuses>().get(&slot.entry.id);

        let turn = slot.begin_sync().unwrap();
        let refused = sync_now(app.handle().clone(), app.state(), None);

        let Err(error) = refused else {
            panic!("a busy vault must refuse");
        };
        assert!(matches!(error, Error::SyncBusy));
        assert_eq!(crate::errors::ErrorView::from(&error).code, "syncBusy");
        assert_eq!(app.state::<SyncStatuses>().get(&slot.entry.id), before);

        drop(turn);
        sync_now(app.handle().clone(), app.state(), None).unwrap();
    }

    #[test]
    fn the_close_sync_waits_for_a_running_sync_and_does_not_start_another() {
        let dir = vault();
        let cloud = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        save_sync_settings(app.state(), folder_form(cloud.path()), None, None).unwrap();
        let config_file = RemoteConfig::path(dir.path());
        let stored = fs::read_to_string(&config_file).unwrap();
        fs::write(
            &config_file,
            stored.replacen("{", "{\"auto_sync_minutes\": 5,", 1),
        )
        .unwrap();
        let slot = app.state::<AppState>().slot(None).unwrap();
        let before = app.state::<SyncStatuses>().get(&slot.entry.id);

        let turn = slot.begin_sync().unwrap();
        let started = std::time::Instant::now();
        let waited = std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                drop(turn);
            });
            sync_status::sync_on_close(app.handle(), std::time::Duration::from_secs(5));
            started.elapsed()
        });

        assert!(
            waited >= std::time::Duration::from_millis(250),
            "{waited:?}"
        );
        assert!(waited < std::time::Duration::from_secs(3), "{waited:?}");
        assert_eq!(app.state::<SyncStatuses>().get(&slot.entry.id), before);
    }
}
